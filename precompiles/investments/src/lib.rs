#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]

use frame_support::dispatch::{GetDispatchInfo, PostDispatchInfo};
use pallet_evm::AddressMapping;
use pallet_investments::Call as InvestmentsCall;
use pallet_pools::TrancheId;
use precompile_utils::prelude::*;
use sp_core::{H160, U256};
use sp_runtime::traits::Dispatchable;
use sp_std::marker::PhantomData;

/// A precompile that dispatches invest/redeem order requests to pallet-investments.
///
/// Called by the CCCP receiver contract when a `requestDeposit` or `requestRedeem`
/// message arrives on Bifrost from an external EVM chain.
pub struct InvestmentsPrecompile<Runtime>(PhantomData<Runtime>);

#[precompile_utils::precompile]
impl<Runtime> InvestmentsPrecompile<Runtime>
where
	Runtime: pallet_investments::Config + pallet_evm::Config + frame_system::Config,
	Runtime::RuntimeCall: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	<Runtime::RuntimeCall as Dispatchable>::RuntimeOrigin: From<Option<Runtime::AccountId>>,
	Runtime::RuntimeCall: From<InvestmentsCall<Runtime>>,
	<Runtime as pallet_evm::Config>::AddressMapping: AddressMapping<Runtime::AccountId>,
{
	/// Submit a pending deposit order for epoch settlement.
	///
	/// Called by the receiver contract when a `requestDeposit` CCCP message arrives.
	///
	/// @param pool_id       the pool ID
	/// @param chain_id      EVM chain ID of the chain where the vault is deployed
	/// @param vault_address ERC-7540 vault contract address on that chain
	/// @param investor      investor address on the external chain
	/// @param amount        USDC amount to deposit
	#[precompile::public("submitDepositOrder(uint64,uint64,address,address,uint256)")]
	#[precompile::public("submit_deposit_order(uint64,uint64,address,address,uint256)")]
	fn submit_deposit_order(
		handle: &mut impl PrecompileHandle,
		pool_id: u64,
		chain_id: u64,
		vault_address: Address,
		investor: Address,
		amount: U256,
	) -> EvmResult {
		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);

		let tranche_id = TrancheId { chain_id, vault_address: vault_address.0 };
		let investor: H160 = investor.0;

		let call = InvestmentsCall::<Runtime>::submit_deposit_order {
			pool_id,
			tranche_id,
			investor,
			amount,
		};

		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call, 0)?;
		Ok(())
	}

	/// Submit a pending redeem order for epoch settlement.
	///
	/// Called by the receiver contract when a `requestRedeem` CCCP message arrives.
	///
	/// @param pool_id       the pool ID
	/// @param chain_id      EVM chain ID of the chain where the vault is deployed
	/// @param vault_address ERC-7540 vault contract address on that chain
	/// @param investor      investor address on the external chain
	/// @param amount        tranche token amount to redeem
	#[precompile::public("submitRedeemOrder(uint64,uint64,address,address,uint256)")]
	#[precompile::public("submit_redeem_order(uint64,uint64,address,address,uint256)")]
	fn submit_redeem_order(
		handle: &mut impl PrecompileHandle,
		pool_id: u64,
		chain_id: u64,
		vault_address: Address,
		investor: Address,
		amount: U256,
	) -> EvmResult {
		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);

		let tranche_id = TrancheId { chain_id, vault_address: vault_address.0 };
		let investor: H160 = investor.0;

		let call = InvestmentsCall::<Runtime>::submit_redeem_order {
			pool_id,
			tranche_id,
			investor,
			amount,
		};

		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call, 0)?;
		Ok(())
	}
}
