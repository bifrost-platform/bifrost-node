#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]

use frame_support::dispatch::{GetDispatchInfo, PostDispatchInfo};
use pallet_investments::Call as InvestmentsCall;
use pallet_investments::MAX_INVESTORS_PER_APPROVAL;
use pallet_pools::{PoolInspect, TrancheId};
use precompile_utils::prelude::*;
use sp_core::{ConstU32, H160, U256};
use sp_runtime::{traits::Dispatchable, BoundedVec};
use sp_std::marker::PhantomData;

/// A precompile that dispatches invest/redeem order requests to pallet-investments.
///
/// Only callable by the Gateway contract whose address is stored in
/// `pallet_pools::GatewayAddress` storage. Calls are dispatched with the
/// `pallet_investments::Origin::Gateway` origin so the pallet rejects any
/// direct extrinsic submissions.
pub struct InvestmentsPrecompile<Runtime>(PhantomData<Runtime>);

#[precompile_utils::precompile]
impl<Runtime> InvestmentsPrecompile<Runtime>
where
	Runtime: pallet_investments::Config + pallet_evm::Config + frame_system::Config,
	Runtime::RuntimeCall: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	<Runtime::RuntimeCall as Dispatchable>::RuntimeOrigin: From<pallet_investments::Origin>,
	Runtime::RuntimeCall: From<InvestmentsCall<Runtime>>,
	<Runtime as pallet_investments::Config>::Pools: PoolInspect<Runtime::AccountId>,
{
	fn gateway_address() -> H160 {
		<Runtime as pallet_investments::Config>::Pools::gateway_address()
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
	#[precompile::public("submitDepositOrder(uint64,uint64,address,address,uint256)")]
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

		let call = InvestmentsCall::<Runtime>::submit_deposit_order {
			pool_id,
			tranche_id,
			investor_id,
			amount,
		};

		RuntimeHelper::<Runtime>::try_dispatch(
			handle,
			pallet_investments::Origin::Gateway.into(),
			call,
			0,
		)?;
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
	#[precompile::public("submitRedeemOrder(uint64,uint64,address,address,uint256)")]
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

		let call = InvestmentsCall::<Runtime>::submit_redeem_order {
			pool_id,
			tranche_id,
			investor_id,
			amount,
		};

		RuntimeHelper::<Runtime>::try_dispatch(
			handle,
			pallet_investments::Origin::Gateway.into(),
			call,
			0,
		)?;
		Ok(())
	}

	/// Approve a selected set of investors' pending deposit orders (Approval mode).
	///
	/// Only the Gateway contract may call this function.
	///
	/// @param pool_id       the pool ID
	/// @param chain_id      EVM chain ID of the chain where the vault is deployed
	/// @param vault_address ERC-7540 vault contract address on that chain
	/// @param investor_ids  list of investor addresses to approve (max 100)
	#[precompile::public("approveDepositOrders(uint64,uint64,address,address[])")]
	fn approve_deposit_orders(
		handle: &mut impl PrecompileHandle,
		pool_id: u64,
		chain_id: u64,
		vault_address: Address,
		investor_ids: Vec<Address>,
	) -> EvmResult {
		if handle.context().caller != Self::gateway_address() {
			return Err(revert("caller is not the gateway"));
		}

		let tranche_id = TrancheId { chain_id, vault_address: vault_address.0 };
		let ids: sp_std::vec::Vec<H160> = investor_ids.into_iter().map(|a| a.0).collect();
		let investor_ids: BoundedVec<H160, ConstU32<MAX_INVESTORS_PER_APPROVAL>> =
			ids.try_into().map_err(|_| revert("too many investor IDs"))?;

		let call = InvestmentsCall::<Runtime>::approve_deposit_orders {
			pool_id,
			tranche_id,
			investor_ids,
		};

		RuntimeHelper::<Runtime>::try_dispatch(
			handle,
			pallet_investments::Origin::Gateway.into(),
			call,
			0,
		)?;
		Ok(())
	}

	/// Approve a selected set of investors' pending redeem orders (Approval mode).
	///
	/// Only the Gateway contract may call this function.
	///
	/// @param pool_id       the pool ID
	/// @param chain_id      EVM chain ID of the chain where the vault is deployed
	/// @param vault_address ERC-7540 vault contract address on that chain
	/// @param investor_ids  list of investor addresses to approve (max 100)
	#[precompile::public("approveRedeemOrders(uint64,uint64,address,address[])")]
	fn approve_redeem_orders(
		handle: &mut impl PrecompileHandle,
		pool_id: u64,
		chain_id: u64,
		vault_address: Address,
		investor_ids: Vec<Address>,
	) -> EvmResult {
		if handle.context().caller != Self::gateway_address() {
			return Err(revert("caller is not the gateway"));
		}

		let tranche_id = TrancheId { chain_id, vault_address: vault_address.0 };
		let ids: sp_std::vec::Vec<H160> = investor_ids.into_iter().map(|a| a.0).collect();
		let investor_ids: BoundedVec<H160, ConstU32<MAX_INVESTORS_PER_APPROVAL>> =
			ids.try_into().map_err(|_| revert("too many investor IDs"))?;

		let call =
			InvestmentsCall::<Runtime>::approve_redeem_orders { pool_id, tranche_id, investor_ids };

		RuntimeHelper::<Runtime>::try_dispatch(
			handle,
			pallet_investments::Origin::Gateway.into(),
			call,
			0,
		)?;
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
	#[precompile::public("claimShares(uint64,uint64,address,address)")]
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

		let call = InvestmentsCall::<Runtime>::claim_shares { pool_id, tranche_id, investor_id };

		RuntimeHelper::<Runtime>::try_dispatch(
			handle,
			pallet_investments::Origin::Gateway.into(),
			call,
			0,
		)?;
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
	#[precompile::public("claimAssets(uint64,uint64,address,address)")]
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

		let call = InvestmentsCall::<Runtime>::claim_assets { pool_id, tranche_id, investor_id };

		RuntimeHelper::<Runtime>::try_dispatch(
			handle,
			pallet_investments::Origin::Gateway.into(),
			call,
			0,
		)?;
		Ok(())
	}
}
