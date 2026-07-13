#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]

extern crate alloc;

use alloc::format;
use frame_support::dispatch::{GetDispatchInfo, PostDispatchInfo};
use pallet_evm::AddressMapping;
use pallet_rwa_pools::{
	Call as PoolsCall, CollateralAsset, PoolInspect, SettlementMode, TrancheId, TrancheInput,
	TrancheTypeInput, MAX_COLLATERALS, MAX_TRANCHES,
};
use precompile_utils::prelude::*;
use sp_core::{H160, U256};
use sp_runtime::{traits::Dispatchable, BoundedVec, FixedU128};
use sp_std::{marker::PhantomData, vec::Vec};

pub(crate) const SELECTOR_LOG_BORROWED: [u8; 32] =
	keccak256!("Borrowed(uint64,uint64,address,address,uint256)");
pub(crate) const SELECTOR_LOG_REPAID: [u8; 32] =
	keccak256!("Repaid(uint64,uint64,address,address,uint256)");
pub(crate) const SELECTOR_LOG_POOL_CREATED: [u8; 32] =
	keccak256!("PoolCreated(uint64,address,uint64,uint64)");

/// A precompile that dispatches pool management requests to pallet-pools.
///
/// `borrow` and `repay` are only callable by the Gateway contract (cross-chain messages).
/// `create_pool` is callable directly by Pool Admin EOAs; it dispatches the pallet extrinsic
/// with `Origin::PoolAdmin`. Spoke-chain vaults are now deployed independently via a factory
/// contract on the Spoke chain, as a prerequisite to `create_pool` on the Hub — this precompile
/// no longer triggers vault deployment via the Gateway.
pub struct PoolsPrecompile<Runtime>(PhantomData<Runtime>);

#[precompile_utils::precompile]
impl<Runtime> PoolsPrecompile<Runtime>
where
	Runtime: pallet_rwa_pools::Config + pallet_evm::Config + frame_system::Config,
	Runtime::RuntimeCall: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	<Runtime::RuntimeCall as Dispatchable>::RuntimeOrigin: From<pallet_rwa_pools::Origin>,
	Runtime::RuntimeCall: From<PoolsCall<Runtime>>,
	pallet_rwa_pools::Pallet<Runtime>: PoolInspect,
	<Runtime as pallet_evm::Config>::AddressMapping: AddressMapping<Runtime::AccountId>,
{
	/// Create a new RWA pool on the Hub and deploy its vaults on the Spoke chain via the Gateway.
	///
	/// Caller must hold the PoolAdmin role for `pool_id`.
	///
	/// @param pool_id                      Hub pool ID
	/// @param borrower_id                  Institution's EVM address
	/// @param epoch_length_secs            Epoch duration in seconds
	/// @param settlement_offset_secs       Seconds before epoch end when the settlement window opens
	/// @param deposit_settlement_approval  true = Approval mode, false = Automatic
	/// @param redeem_settlement_approval   true = Approval mode, false = Automatic
	/// @param collaterals                  Array of (nftContract, nftTokenId) tuples
	/// @param tranches                     Array of (chainId, vaultAddress, isSenior, apr, maxDeposits) tuples; maxDeposits=0 means uncapped
	#[precompile::public("create_pool(uint64,address,uint64,uint64,bool,bool,(address,uint256)[],(uint64,address,bool,uint256,uint256)[])")]
	fn create_pool(
		handle: &mut impl PrecompileHandle,
		pool_id: u64,
		borrower_id: Address,
		epoch_length_secs: u64,
		settlement_offset_secs: u64,
		deposit_settlement_approval: bool,
		redeem_settlement_approval: bool,
		collaterals: Vec<(Address, U256)>,
		tranches: Vec<(u64, Address, bool, U256, U256)>,
	) -> EvmResult {
		let caller = handle.context().caller;
		let pool_admin = Runtime::AddressMapping::into_account_id(caller);
		let borrower_id: H160 = borrower_id.0;
		let borrower = Runtime::AddressMapping::into_account_id(borrower_id);

		let collaterals: BoundedVec<CollateralAsset, sp_core::ConstU32<MAX_COLLATERALS>> =
			collaterals
				.into_iter()
				.map(|(nft_contract, nft_token_id)| CollateralAsset {
					nft_contract: nft_contract.0,
					nft_token_id,
				})
				.collect::<Vec<_>>()
				.try_into()
				.map_err(|_| revert("too many collaterals"))?;

		if tranches.is_empty() {
			return Err(revert("tranches must not be empty"));
		}

		let tranche_inputs: Vec<TrancheInput> = tranches
			.iter()
			.map(|(chain_id, vault_address, is_senior, apr, max_deposits)| {
				let tranche_type = if *is_senior {
					if *apr > U256::from(u128::MAX) {
						return Err(revert("apr overflows u128"));
					}
					TrancheTypeInput::Senior { apr: FixedU128::from_inner(apr.low_u128()) }
				} else {
					TrancheTypeInput::Junior
				};
				Ok(TrancheInput {
					tranche_type,
					tranche_id: TrancheId { chain_id: *chain_id, vault_address: vault_address.0 },
					max_deposits: if max_deposits.is_zero() { None } else { Some(*max_deposits) },
				})
			})
			.collect::<EvmResult<Vec<_>>>()?;
		let tranche_inputs: BoundedVec<TrancheInput, sp_core::ConstU32<MAX_TRANCHES>> =
			tranche_inputs.try_into().map_err(|_| revert("too many tranches"))?;

		let deposit_settlement = if deposit_settlement_approval {
			SettlementMode::Approval
		} else {
			SettlementMode::Automatic
		};
		let redeem_settlement = if redeem_settlement_approval {
			SettlementMode::Approval
		} else {
			SettlementMode::Automatic
		};

		let call = PoolsCall::<Runtime>::create_pool {
			pool_id,
			pool_admin,
			borrower,
			collaterals,
			epoch_length_secs,
			settlement_offset_secs,
			deposit_settlement,
			redeem_settlement,
			tranches: tranche_inputs,
		};

		RuntimeHelper::<Runtime>::try_dispatch(
			handle,
			pallet_rwa_pools::Origin::PoolAdmin.into(),
			call,
			0,
		)?;

		let event = log1(
			handle.context().address,
			SELECTOR_LOG_POOL_CREATED,
			solidity::encode_event_data((
				U256::from(pool_id),
				Address(borrower_id),
				U256::from(epoch_length_secs),
				U256::from(settlement_offset_secs),
			)),
		);
		handle.record_log_costs(&[&event])?;
		event.record(handle)?;

		Ok(())
	}

	/// Draw funds from a tranche treasury.
	///
	/// Only the Gateway contract may call this function.
	///
	/// @param pool_id       the pool ID
	/// @param chain_id      EVM chain ID of the chain where the vault is deployed
	/// @param vault_address ERC-7540 vault contract address on that chain
	/// @param borrower      EVM address of the institution initiating the borrow
	/// @param amount        USDC amount to borrow
	#[precompile::public("borrow(uint64,uint64,address,address,uint256)")]
	fn borrow(
		handle: &mut impl PrecompileHandle,
		pool_id: u64,
		chain_id: u64,
		vault_address: Address,
		borrower: Address,
		amount: U256,
	) -> EvmResult {
		if handle.context().caller != pallet_rwa_pools::Pallet::<Runtime>::gateway_address() {
			return Err(revert("caller is not the gateway"));
		}

		let vault_address: H160 = vault_address.0;
		let borrower: H160 = borrower.0;
		let borrower_account = Runtime::AddressMapping::into_account_id(borrower);
		let call = PoolsCall::<Runtime>::borrow {
			pool_id,
			chain_id,
			vault_address,
			borrower: borrower_account,
			amount,
		};

		RuntimeHelper::<Runtime>::try_dispatch(
			handle,
			pallet_rwa_pools::Origin::Gateway.into(),
			call,
			0,
		)?;

		let event = log1(
			handle.context().address,
			SELECTOR_LOG_BORROWED,
			solidity::encode_event_data((
				U256::from(pool_id),
				U256::from(chain_id),
				Address(vault_address),
				Address(borrower),
				amount,
			)),
		);
		handle.record_log_costs(&[&event])?;
		event.record(handle)?;

		Ok(())
	}

	/// Return funds to a tranche treasury.
	///
	/// Only the Gateway contract may call this function.
	///
	/// @param pool_id       the pool ID
	/// @param chain_id      EVM chain ID of the chain where the vault is deployed
	/// @param vault_address ERC-7540 vault contract address on that chain
	/// @param borrower      EVM address of the institution initiating the repay
	/// @param amount        USDC amount being repaid
	#[precompile::public("repay(uint64,uint64,address,address,uint256)")]
	fn repay(
		handle: &mut impl PrecompileHandle,
		pool_id: u64,
		chain_id: u64,
		vault_address: Address,
		borrower: Address,
		amount: U256,
	) -> EvmResult {
		if handle.context().caller != pallet_rwa_pools::Pallet::<Runtime>::gateway_address() {
			return Err(revert("caller is not the gateway"));
		}

		let vault_address: H160 = vault_address.0;
		let borrower: H160 = borrower.0;
		let borrower_account = Runtime::AddressMapping::into_account_id(borrower);
		let call = PoolsCall::<Runtime>::repay {
			pool_id,
			chain_id,
			vault_address,
			borrower: borrower_account,
			amount,
		};

		RuntimeHelper::<Runtime>::try_dispatch(
			handle,
			pallet_rwa_pools::Origin::Gateway.into(),
			call,
			0,
		)?;

		let event = log1(
			handle.context().address,
			SELECTOR_LOG_REPAID,
			solidity::encode_event_data((
				U256::from(pool_id),
				U256::from(chain_id),
				Address(vault_address),
				Address(borrower),
				amount,
			)),
		);
		handle.record_log_costs(&[&event])?;
		event.record(handle)?;

		Ok(())
	}
}
