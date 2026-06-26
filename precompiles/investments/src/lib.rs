#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]

use frame_support::dispatch::{GetDispatchInfo, PostDispatchInfo};
use pallet_evm::AddressMapping;
use pallet_rwa_investments::Call as InvestmentsCall;
use pallet_rwa_investments::MAX_INVESTORS_PER_APPROVAL;
use pallet_rwa_pools::{PoolInspect, TrancheId};
use precompile_utils::prelude::*;
use sp_core::{ConstU32, H160, U256};
use sp_runtime::{traits::Dispatchable, BoundedVec};
use sp_std::{marker::PhantomData, vec::Vec};

pub(crate) const SELECTOR_LOG_DEPOSIT_ORDER_SUBMITTED: [u8; 32] =
	keccak256!("DepositOrderSubmitted(uint64,uint64,address,address,uint256)");
pub(crate) const SELECTOR_LOG_REDEEM_ORDER_SUBMITTED: [u8; 32] =
	keccak256!("RedeemOrderSubmitted(uint64,uint64,address,address,uint256)");
pub(crate) const SELECTOR_LOG_DEPOSIT_ORDER_APPROVED: [u8; 32] =
	keccak256!("DepositOrderApproved(uint64,uint64,address,address,address)");
pub(crate) const SELECTOR_LOG_REDEEM_ORDER_APPROVED: [u8; 32] =
	keccak256!("RedeemOrderApproved(uint64,uint64,address,address,address)");
pub(crate) const SELECTOR_LOG_SHARES_CLAIMED: [u8; 32] =
	keccak256!("SharesClaimed(uint64,uint64,address,address,uint256)");
pub(crate) const SELECTOR_LOG_ASSETS_CLAIMED: [u8; 32] =
	keccak256!("AssetsClaimed(uint64,uint64,address,address,uint256)");

/// A precompile that dispatches invest/redeem order requests to pallet-investments.
///
/// Only callable by the Gateway contract whose address is stored in
/// `pallet_rwa_pools::GatewayAddress` storage. Calls are dispatched with the
/// `pallet_rwa_investments::Origin::Gateway` origin so the pallet rejects any
/// direct extrinsic submissions.
pub struct InvestmentsPrecompile<Runtime>(PhantomData<Runtime>);

#[precompile_utils::precompile]
impl<Runtime> InvestmentsPrecompile<Runtime>
where
	Runtime: pallet_rwa_investments::Config + pallet_evm::Config + frame_system::Config,
	Runtime::RuntimeCall: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	<Runtime::RuntimeCall as Dispatchable>::RuntimeOrigin: From<pallet_rwa_pools::Origin>,
	Runtime::RuntimeCall: From<InvestmentsCall<Runtime>>,
	<Runtime as pallet_rwa_investments::Config>::Pools: PoolInspect,
	<Runtime as pallet_evm::Config>::AddressMapping: AddressMapping<Runtime::AccountId>,
{
	fn gateway_address() -> H160 {
		<Runtime as pallet_rwa_investments::Config>::Pools::gateway_address()
	}

	/// Submit a pending deposit order for epoch settlement.
	///
	/// Only the Gateway contract may call this function.
	///
	/// @param pool_id       the pool ID
	/// @param chain_id      EVM chain ID of the chain where the vault is deployed
	/// @param vault_address ERC-7540 vault contract address on that chain
	/// @param investor_id   investor address on the external chain
	/// @param amount        USDC amount to deposit
	#[precompile::public("submit_deposit_order(uint64,uint64,address,address,uint256)")]
	fn submit_deposit_order(
		handle: &mut impl PrecompileHandle,
		pool_id: u64,
		chain_id: u64,
		vault_address: Address,
		investor_id: Address,
		amount: U256,
	) -> EvmResult {
		if handle.context().caller != Self::gateway_address() {
			return Err(revert("caller is not the gateway"));
		}

		let tranche_id = TrancheId { chain_id, vault_address: vault_address.0 };
		let investor_id: H160 = investor_id.0;
		let investor_account = Runtime::AddressMapping::into_account_id(investor_id);

		let call = InvestmentsCall::<Runtime>::submit_deposit_order {
			pool_id,
			tranche_id,
			investor_id: investor_account,
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
			SELECTOR_LOG_DEPOSIT_ORDER_SUBMITTED,
			solidity::encode_event_data((
				U256::from(pool_id),
				U256::from(chain_id),
				vault_address,
				Address(investor_id),
				amount,
			)),
		);
		handle.record_log_costs(&[&event])?;
		event.record(handle)?;

		Ok(())
	}

	/// Submit a pending redeem order for epoch settlement.
	///
	/// Only the Gateway contract may call this function.
	///
	/// @param pool_id       the pool ID
	/// @param chain_id      EVM chain ID of the chain where the vault is deployed
	/// @param vault_address ERC-7540 vault contract address on that chain
	/// @param investor_id   investor address on the external chain
	/// @param amount        tranche token amount to redeem
	#[precompile::public("submit_redeem_order(uint64,uint64,address,address,uint256)")]
	fn submit_redeem_order(
		handle: &mut impl PrecompileHandle,
		pool_id: u64,
		chain_id: u64,
		vault_address: Address,
		investor_id: Address,
		amount: U256,
	) -> EvmResult {
		if handle.context().caller != Self::gateway_address() {
			return Err(revert("caller is not the gateway"));
		}

		let tranche_id = TrancheId { chain_id, vault_address: vault_address.0 };
		let investor_id: H160 = investor_id.0;
		let investor_account = Runtime::AddressMapping::into_account_id(investor_id);

		let call = InvestmentsCall::<Runtime>::submit_redeem_order {
			pool_id,
			tranche_id,
			investor_id: investor_account,
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
			SELECTOR_LOG_REDEEM_ORDER_SUBMITTED,
			solidity::encode_event_data((
				U256::from(pool_id),
				U256::from(chain_id),
				vault_address,
				Address(investor_id),
				amount,
			)),
		);
		handle.record_log_costs(&[&event])?;
		event.record(handle)?;

		Ok(())
	}

	/// Approve a selected set of investors' pending deposit orders (Approval mode).
	///
	/// Only the Gateway contract may call this function.
	///
	/// @param pool_id       the pool ID
	/// @param chain_id      EVM chain ID of the chain where the vault is deployed
	/// @param vault_address ERC-7540 vault contract address on that chain
	/// @param borrower      EVM address of the institution approving the orders
	/// @param investor_ids  list of investor addresses to approve (max 100)
	#[precompile::public("approve_deposit_orders(uint64,uint64,address,address,address[])")]
	fn approve_deposit_orders(
		handle: &mut impl PrecompileHandle,
		pool_id: u64,
		chain_id: u64,
		vault_address: Address,
		borrower: Address,
		investor_ids: Vec<Address>,
	) -> EvmResult {
		if handle.context().caller != Self::gateway_address() {
			return Err(revert("caller is not the gateway"));
		}

		let tranche_id = TrancheId { chain_id, vault_address: vault_address.0 };
		let borrower: H160 = borrower.0;
		let borrower_account = Runtime::AddressMapping::into_account_id(borrower);
		let ids: sp_std::vec::Vec<H160> = investor_ids.into_iter().map(|a| a.0).collect();
		let investor_accounts: sp_std::vec::Vec<Runtime::AccountId> =
			ids.iter().map(|a| Runtime::AddressMapping::into_account_id(*a)).collect();
		let investor_ids: BoundedVec<Runtime::AccountId, ConstU32<MAX_INVESTORS_PER_APPROVAL>> =
			investor_accounts.try_into().map_err(|_| revert("too many investor IDs"))?;

		let call = InvestmentsCall::<Runtime>::approve_deposit_orders {
			pool_id,
			tranche_id,
			borrower: borrower_account,
			investor_ids,
		};

		RuntimeHelper::<Runtime>::try_dispatch(
			handle,
			pallet_rwa_pools::Origin::Gateway.into(),
			call,
			0,
		)?;

		for investor_id in &ids {
			let event = log1(
				handle.context().address,
				SELECTOR_LOG_DEPOSIT_ORDER_APPROVED,
				solidity::encode_event_data((
					U256::from(pool_id),
					U256::from(chain_id),
					vault_address,
					Address(borrower),
					Address(*investor_id),
				)),
			);
			handle.record_log_costs(&[&event])?;
			event.record(handle)?;
		}

		Ok(())
	}

	/// Approve a selected set of investors' pending redeem orders (Approval mode).
	///
	/// Only the Gateway contract may call this function.
	///
	/// @param pool_id       the pool ID
	/// @param chain_id      EVM chain ID of the chain where the vault is deployed
	/// @param vault_address ERC-7540 vault contract address on that chain
	/// @param borrower      EVM address of the institution approving the orders
	/// @param investor_ids  list of investor addresses to approve (max 100)
	#[precompile::public("approve_redeem_orders(uint64,uint64,address,address,address[])")]
	fn approve_redeem_orders(
		handle: &mut impl PrecompileHandle,
		pool_id: u64,
		chain_id: u64,
		vault_address: Address,
		borrower: Address,
		investor_ids: Vec<Address>,
	) -> EvmResult {
		if handle.context().caller != Self::gateway_address() {
			return Err(revert("caller is not the gateway"));
		}

		let tranche_id = TrancheId { chain_id, vault_address: vault_address.0 };
		let borrower: H160 = borrower.0;
		let borrower_account = Runtime::AddressMapping::into_account_id(borrower);
		let ids: sp_std::vec::Vec<H160> = investor_ids.into_iter().map(|a| a.0).collect();
		let investor_accounts: sp_std::vec::Vec<Runtime::AccountId> =
			ids.iter().map(|a| Runtime::AddressMapping::into_account_id(*a)).collect();
		let investor_ids: BoundedVec<Runtime::AccountId, ConstU32<MAX_INVESTORS_PER_APPROVAL>> =
			investor_accounts.try_into().map_err(|_| revert("too many investor IDs"))?;

		let call = InvestmentsCall::<Runtime>::approve_redeem_orders {
			pool_id,
			tranche_id,
			borrower: borrower_account,
			investor_ids,
		};

		RuntimeHelper::<Runtime>::try_dispatch(
			handle,
			pallet_rwa_pools::Origin::Gateway.into(),
			call,
			0,
		)?;

		for investor_id in &ids {
			let event = log1(
				handle.context().address,
				SELECTOR_LOG_REDEEM_ORDER_APPROVED,
				solidity::encode_event_data((
					U256::from(pool_id),
					U256::from(chain_id),
					vault_address,
					Address(borrower),
					Address(*investor_id),
				)),
			);
			handle.record_log_costs(&[&event])?;
			event.record(handle)?;
		}

		Ok(())
	}

	/// Automatic mode: claim settled deposit shares for an investor.
	///
	/// Called by the Gateway when a `requestTrancheClaim()` message for a deposit
	/// arrives from the spoke chain. Moves the entry from `ClaimableDepositOrders`
	/// to `ApprovedDepositOrders` so the Gateway can send the mint instruction.
	///
	/// Only the Gateway contract may call this function.
	///
	/// @param pool_id       the pool ID
	/// @param chain_id      EVM chain ID of the chain where the vault is deployed
	/// @param vault_address ERC-7540 vault contract address on that chain
	/// @param investor_id   investor address on the external chain
	#[precompile::public("claim_shares(uint64,uint64,address,address)")]
	fn claim_shares(
		handle: &mut impl PrecompileHandle,
		pool_id: u64,
		chain_id: u64,
		vault_address: Address,
		investor_id: Address,
	) -> EvmResult {
		if handle.context().caller != Self::gateway_address() {
			return Err(revert("caller is not the gateway"));
		}

		let tranche_id = TrancheId { chain_id, vault_address: vault_address.0 };
		let investor_id: H160 = investor_id.0;
		let investor_account = Runtime::AddressMapping::into_account_id(investor_id);

		// Read shares_to_mint before dispatch — the pallet removes the entry via take().
		let shares_to_mint = pallet_rwa_investments::ClaimableDepositOrders::<Runtime>::get(
			&tranche_id,
			&investor_account,
		)
		.map(|o| o.shares_to_mint)
		.unwrap_or_default();

		let call = InvestmentsCall::<Runtime>::claim_shares {
			pool_id,
			tranche_id,
			investor_id: investor_account,
		};

		RuntimeHelper::<Runtime>::try_dispatch(
			handle,
			pallet_rwa_pools::Origin::Gateway.into(),
			call,
			0,
		)?;

		let event = log1(
			handle.context().address,
			SELECTOR_LOG_SHARES_CLAIMED,
			solidity::encode_event_data((
				U256::from(pool_id),
				U256::from(chain_id),
				vault_address,
				Address(investor_id),
				shares_to_mint,
			)),
		);
		handle.record_log_costs(&[&event])?;
		event.record(handle)?;

		Ok(())
	}

	/// Automatic mode: claim settled redemption assets for an investor.
	///
	/// Called by the Gateway when a `requestTrancheClaim()` message for a redemption
	/// arrives from the spoke chain. Moves the entry from `ClaimableRedeemOrders`
	/// to `ApprovedRedeemOrders` so the Gateway can send the payout instruction.
	///
	/// Only the Gateway contract may call this function.
	///
	/// @param pool_id       the pool ID
	/// @param chain_id      EVM chain ID of the chain where the vault is deployed
	/// @param vault_address ERC-7540 vault contract address on that chain
	/// @param investor_id   investor address on the external chain
	#[precompile::public("claim_assets(uint64,uint64,address,address)")]
	fn claim_assets(
		handle: &mut impl PrecompileHandle,
		pool_id: u64,
		chain_id: u64,
		vault_address: Address,
		investor_id: Address,
	) -> EvmResult {
		if handle.context().caller != Self::gateway_address() {
			return Err(revert("caller is not the gateway"));
		}

		let tranche_id = TrancheId { chain_id, vault_address: vault_address.0 };
		let investor_id: H160 = investor_id.0;
		let investor_account = Runtime::AddressMapping::into_account_id(investor_id);

		// Read payout before dispatch — the pallet removes the entry via take().
		let payout = pallet_rwa_investments::ClaimableRedeemOrders::<Runtime>::get(
			&tranche_id,
			&investor_account,
		)
		.map(|o| o.payout)
		.unwrap_or_default();

		let call = InvestmentsCall::<Runtime>::claim_assets {
			pool_id,
			tranche_id,
			investor_id: investor_account,
		};

		RuntimeHelper::<Runtime>::try_dispatch(
			handle,
			pallet_rwa_pools::Origin::Gateway.into(),
			call,
			0,
		)?;

		let event = log1(
			handle.context().address,
			SELECTOR_LOG_ASSETS_CLAIMED,
			solidity::encode_event_data((
				U256::from(pool_id),
				U256::from(chain_id),
				vault_address,
				Address(investor_id),
				payout,
			)),
		);
		handle.record_log_costs(&[&event])?;
		event.record(handle)?;

		Ok(())
	}
}
