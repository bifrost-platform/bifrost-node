#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]

use frame_support::dispatch::{GetDispatchInfo, PostDispatchInfo};
use pallet_tranche_investments::{
	AdapterValuation, Allocation, AssetPosition, Call as InvestmentsCall, OrderType,
	MAX_ADAPTER_VALUATIONS, MAX_ALLOCATIONS, MAX_ASSET_POSITIONS,
};
use pallet_tranche_system::{AdapterKey, ProductId};
use precompile_utils::prelude::*;
use sp_core::{ConstU32, H160, U256};
use sp_runtime::{traits::Dispatchable, BoundedVec};
use sp_std::{marker::PhantomData, vec::Vec};

// ---------------------------------------------------------------------------
// Event log selectors
// ---------------------------------------------------------------------------

pub(crate) const SELECTOR_LOG_INVESTMENT_REQUESTED: [u8; 32] =
	keccak256!("InvestmentRequested(uint256,uint256,uint256,uint64,address,address,uint256,uint8)");
pub(crate) const SELECTOR_LOG_INVESTMENT_APPROVED: [u8; 32] =
	keccak256!("InvestmentApproved(uint256,uint256,uint256,(address,uint64,uint256)[],uint256)");
pub(crate) const SELECTOR_LOG_ADAPTER_VALUATIONS_RECORDED: [u8; 32] = keccak256!(
	"AdapterValuationsRecorded(uint256,uint256,(uint64,address,uint256,uint64,uint256,(address,uint256,uint256,uint256,bool)[])[])"
);
pub(crate) const SELECTOR_LOG_PRODUCT_NAV_RECORDED: [u8; 32] =
	keccak256!("ProductNavRecorded(uint256,uint256,uint256)");

// ---------------------------------------------------------------------------
// interface.sol struct <-> tuple mappings
// ---------------------------------------------------------------------------

/// `Allocation` — (adapter_address, adapter_chain_id, amount)
type EvmAllocation = (Address, u64, U256);
/// `AssetPosition` — (asset, amount, priceUsd, usdValue, counted)
type EvmAssetPosition = (Address, U256, U256, U256, bool);
/// `AdapterValuation` — (chainId, adapter, epochId, valuationCutoff, principal, positions)
type EvmAdapterValuation = (u64, Address, U256, u64, U256, Vec<EvmAssetPosition>);

// ---------------------------------------------------------------------------
// Precompile
// ---------------------------------------------------------------------------

/// A precompile that wraps `pallet_tranche_investments`'s `record_*` extrinsics.
///
/// Called exclusively by the calling product's registered Valuation contract
/// — not by a Gateway. Every function reads `pallet-tranche-system`'s
/// `Products` storage directly to confirm `handle.context().caller` equals
/// that product's registered `valuation.valuation_address`, before
/// constructing `pallet_tranche_investments::Origin::Valuation` — the pallet
/// itself trusts that check happened here and doesn't re-verify it (see
/// `Origin::Valuation`'s doc comment in `pallet_tranche_investments`).
pub struct TrancheInvestmentsPrecompile<Runtime>(PhantomData<Runtime>);

#[precompile_utils::precompile]
impl<Runtime> TrancheInvestmentsPrecompile<Runtime>
where
	Runtime: pallet_tranche_investments::Config
		+ pallet_tranche_system::Config
		+ pallet_evm::Config
		+ frame_system::Config,
	Runtime::RuntimeCall: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	Runtime::RuntimeCall: From<InvestmentsCall<Runtime>>,
	Runtime::RuntimeOrigin: From<pallet_tranche_investments::Origin>,
{
	/// Record a pending deposit or redeem request. See
	/// `pallet_tranche_investments::record_investment_request`'s doc comment.
	///
	/// @param order_type 0 = redeem, 1 = deposit
	#[precompile::public(
		"record_investment_request(uint256,uint256,uint256,uint64,address,address,uint256,uint8)"
	)]
	fn record_investment_request(
		handle: &mut impl PrecompileHandle,
		product_id: U256,
		request_id: U256,
		settlement_id: U256,
		vault_chain_id: u64,
		vault_address: Address,
		investor_address: Address,
		amount: U256,
		order_type: u8,
	) -> EvmResult {
		let caller = handle.context().caller;
		let product_id = to_product_id(product_id)?;
		ensure_caller_is_valuation::<Runtime>(product_id, caller)?;
		let decoded_order_type = decode_order_type(order_type)?;

		let call = InvestmentsCall::<Runtime>::record_investment_request {
			product_id,
			request_id,
			settlement_id,
			vault_chain_id,
			vault_address: vault_address.0,
			investor_address: investor_address.0,
			amount,
			order_type: decoded_order_type,
		};
		RuntimeHelper::<Runtime>::try_dispatch(
			handle,
			pallet_tranche_investments::Origin::Valuation.into(),
			call,
			0,
		)?;

		let event = log1(
			handle.context().address,
			SELECTOR_LOG_INVESTMENT_REQUESTED,
			solidity::encode_event_data((
				U256::from(product_id),
				request_id,
				settlement_id,
				vault_chain_id,
				vault_address,
				investor_address,
				amount,
				order_type,
			)),
		);
		handle.record_log_costs(&[&event])?;
		event.record(handle)?;

		Ok(())
	}

	/// Record a pending request's full approval. See
	/// `pallet_tranche_investments::record_investment_approval`'s doc comment.
	#[precompile::public(
		"record_investment_approval(uint256,uint256,uint256,(address,uint64,uint256)[],uint256)"
	)]
	fn record_investment_approval(
		handle: &mut impl PrecompileHandle,
		product_id: U256,
		request_id: U256,
		settlement_id: U256,
		allocations: Vec<EvmAllocation>,
		claimable_assets: U256,
	) -> EvmResult {
		let caller = handle.context().caller;
		let product_id = to_product_id(product_id)?;
		ensure_caller_is_valuation::<Runtime>(product_id, caller)?;
		let bounded_allocations = decode_allocations(&allocations)?;

		let call = InvestmentsCall::<Runtime>::record_investment_approval {
			product_id,
			request_id,
			settlement_id,
			allocations: bounded_allocations,
			claimable_assets,
		};
		RuntimeHelper::<Runtime>::try_dispatch(
			handle,
			pallet_tranche_investments::Origin::Valuation.into(),
			call,
			0,
		)?;

		let event = log1(
			handle.context().address,
			SELECTOR_LOG_INVESTMENT_APPROVED,
			solidity::encode_event_data((
				U256::from(product_id),
				request_id,
				settlement_id,
				allocations,
				claimable_assets,
			)),
		);
		handle.record_log_costs(&[&event])?;
		event.record(handle)?;

		Ok(())
	}

	/// Record the finalized per-Adapter NAV breakdown for a settlement. See
	/// `pallet_tranche_investments::record_adapter_valuations`'s doc comment.
	#[precompile::public(
		"record_adapter_valuations(uint256,uint256,(uint64,address,uint256,uint64,uint256,(address,uint256,uint256,uint256,bool)[])[])"
	)]
	fn record_adapter_valuations(
		handle: &mut impl PrecompileHandle,
		product_id: U256,
		settlement_id: U256,
		valuations: Vec<EvmAdapterValuation>,
	) -> EvmResult {
		let caller = handle.context().caller;
		let product_id = to_product_id(product_id)?;
		ensure_caller_is_valuation::<Runtime>(product_id, caller)?;
		let bounded_valuations = decode_adapter_valuations(&valuations)?;

		let call = InvestmentsCall::<Runtime>::record_adapter_valuations {
			product_id,
			settlement_id,
			valuations: bounded_valuations,
		};
		RuntimeHelper::<Runtime>::try_dispatch(
			handle,
			pallet_tranche_investments::Origin::Valuation.into(),
			call,
			0,
		)?;

		let event = log1(
			handle.context().address,
			SELECTOR_LOG_ADAPTER_VALUATIONS_RECORDED,
			solidity::encode_event_data((U256::from(product_id), settlement_id, valuations)),
		);
		handle.record_log_costs(&[&event])?;
		event.record(handle)?;

		Ok(())
	}

	/// Record the settlement's finalized aggregate NAV. See
	/// `pallet_tranche_investments::record_product_nav`'s doc comment.
	#[precompile::public("record_product_nav(uint256,uint256,uint256)")]
	fn record_product_nav(
		handle: &mut impl PrecompileHandle,
		product_id: U256,
		settlement_id: U256,
		product_nav: U256,
	) -> EvmResult {
		let caller = handle.context().caller;
		let product_id = to_product_id(product_id)?;
		ensure_caller_is_valuation::<Runtime>(product_id, caller)?;

		let call = InvestmentsCall::<Runtime>::record_product_nav {
			product_id,
			settlement_id,
			product_nav,
		};
		RuntimeHelper::<Runtime>::try_dispatch(
			handle,
			pallet_tranche_investments::Origin::Valuation.into(),
			call,
			0,
		)?;

		let event = log1(
			handle.context().address,
			SELECTOR_LOG_PRODUCT_NAV_RECORDED,
			solidity::encode_event_data((U256::from(product_id), settlement_id, product_nav)),
		);
		handle.record_log_costs(&[&event])?;
		event.record(handle)?;

		Ok(())
	}
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// `pallet_tranche_system::ProductId` is `u64`; `interface.sol` carries it as
/// `uint256`. Reverts rather than silently truncating if the caller passes a
/// value that doesn't fit.
fn to_product_id(product_id: U256) -> EvmResult<ProductId> {
	if product_id > U256::from(u64::MAX) {
		return Err(revert("product_id exceeds u64::MAX"));
	}
	Ok(product_id.as_u64())
}

/// Reads `pallet-tranche-system`'s `Products` storage directly to confirm
/// `caller` equals `product_id`'s registered Valuation contract address.
fn ensure_caller_is_valuation<Runtime>(product_id: ProductId, caller: H160) -> EvmResult
where
	Runtime: pallet_tranche_system::Config,
{
	let product = pallet_tranche_system::Products::<Runtime>::get(product_id)
		.ok_or_else(|| revert("product not found"))?;
	if product.valuation.valuation_address != caller {
		return Err(revert("caller is not product_id's registered Valuation contract"));
	}
	Ok(())
}

fn decode_order_type(order_type: u8) -> EvmResult<OrderType> {
	match order_type {
		0 => Ok(OrderType::Redeem),
		1 => Ok(OrderType::Deposit),
		_ => Err(revert("invalid order_type")),
	}
}

fn decode_allocations(
	allocations: &[EvmAllocation],
) -> EvmResult<BoundedVec<Allocation, ConstU32<MAX_ALLOCATIONS>>> {
	let mut bounded = BoundedVec::<Allocation, ConstU32<MAX_ALLOCATIONS>>::default();
	for (adapter_address, adapter_chain_id, amount) in allocations.iter().cloned() {
		let adapter = AdapterKey { address: adapter_address.0, chain_id: adapter_chain_id };
		bounded
			.try_push(Allocation { adapter, amount })
			.map_err(|_| revert("too many allocations"))?;
	}
	Ok(bounded)
}

fn decode_asset_positions(
	positions: &[EvmAssetPosition],
) -> EvmResult<BoundedVec<AssetPosition, ConstU32<MAX_ASSET_POSITIONS>>> {
	let mut bounded = BoundedVec::<AssetPosition, ConstU32<MAX_ASSET_POSITIONS>>::default();
	for (asset, amount, price_usd, usd_value, counted) in positions.iter().cloned() {
		bounded
			.try_push(AssetPosition { asset: asset.0, amount, price_usd, usd_value, counted })
			.map_err(|_| revert("too many asset positions"))?;
	}
	Ok(bounded)
}

fn decode_adapter_valuations(
	valuations: &[EvmAdapterValuation],
) -> EvmResult<BoundedVec<AdapterValuation, ConstU32<MAX_ADAPTER_VALUATIONS>>> {
	let mut bounded = BoundedVec::<AdapterValuation, ConstU32<MAX_ADAPTER_VALUATIONS>>::default();
	for (chain_id, adapter, epoch_id, valuation_cutoff, principal, positions) in
		valuations.iter().cloned()
	{
		let positions = decode_asset_positions(&positions)?;
		bounded
			.try_push(AdapterValuation {
				chain_id,
				adapter: adapter.0,
				epoch_id,
				valuation_cutoff,
				principal,
				positions,
			})
			.map_err(|_| revert("too many adapter valuations"))?;
	}
	Ok(bounded)
}
