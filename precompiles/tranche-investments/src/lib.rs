#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]

use frame_support::dispatch::{GetDispatchInfo, PostDispatchInfo};
use pallet_tranche_investments::{
	AdapterValuation, Allocation, AssetPosition, Call as InvestmentsCall, OrderType, TrancheSettle,
	MAX_ADAPTER_VALUATIONS, MAX_ALLOCATIONS, MAX_ASSET_POSITIONS,
};
use pallet_tranche_system::{AdapterKey, ProductId, VaultId};
use precompile_utils::prelude::*;
use sp_core::{ConstU32, H160, H256, U256};
use sp_runtime::{traits::Dispatchable, BoundedVec};
use sp_std::{marker::PhantomData, vec::Vec};

// ---------------------------------------------------------------------------
// Event log selectors
// ---------------------------------------------------------------------------

pub(crate) const SELECTOR_LOG_INVESTMENT_REQUESTED: [u8; 32] =
	keccak256!("InvestmentRequested(uint256,bytes32,uint256,uint64,address,address,uint256,uint8)");
pub(crate) const SELECTOR_LOG_INVESTMENT_APPROVED: [u8; 32] =
	keccak256!("InvestmentApproved(uint256,bytes32,uint256,(address,uint64,uint256)[],uint256)");
pub(crate) const SELECTOR_LOG_ADAPTER_VALUATIONS_RECORDED: [u8; 32] = keccak256!(
	"AdapterValuationsRecorded(uint256,uint256,(uint64,address,uint256,uint64,uint256,(address,uint256,uint256,uint256,bool)[])[])"
);
pub(crate) const SELECTOR_LOG_TRANCHE_SETTLEMENT_RECORDED: [u8; 32] =
	keccak256!("TrancheSettlementRecorded(uint256,uint256,uint256,uint256)");

// ---------------------------------------------------------------------------
// interface.sol struct <-> tuple mappings
// ---------------------------------------------------------------------------

/// `Allocation` — (adapter_address, adapter_chain_id, amount)
type EvmAllocation = (Address, u64, U256);
/// `AssetPosition` — (asset, amount, priceUsd, usdValue, counted)
type EvmAssetPosition = (Address, U256, U256, U256, bool);
/// `AdapterValuation` — (chainId, adapter, epochId, valuationCutoff, principal, positions)
type EvmAdapterValuation = (u64, Address, U256, u64, U256, Vec<EvmAssetPosition>);
/// `TrancheSettle` — (vault_chain_id, vault_address, tranche_nav, share_price,
/// units_outstanding, principal)
type EvmTrancheSettle = (u64, Address, U256, U256, U256, U256);
/// `VaultInput` — (chain_id, vault_address)
type EvmVaultInput = (u64, Address);

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
		"record_investment_request(uint256,bytes32,uint256,uint64,address,address,uint256,uint8)"
	)]
	fn record_investment_request(
		handle: &mut impl PrecompileHandle,
		product_id: U256,
		request_id: H256,
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
		"record_investment_approval(uint256,bytes32,uint256,(address,uint64,uint256)[],uint256)"
	)]
	fn record_investment_approval(
		handle: &mut impl PrecompileHandle,
		product_id: U256,
		request_id: H256,
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

	/// Record the post-waterfall per-tranche settlement result, plus the
	/// product's finalized aggregate NAV. See
	/// `pallet_tranche_investments::record_tranche_settlement`'s doc comment.
	#[precompile::public(
		"record_tranche_settlement(uint256,uint256,(uint64,address,uint256,uint256,uint256,uint256)[],uint256,uint256)"
	)]
	fn record_tranche_settlement(
		handle: &mut impl PrecompileHandle,
		product_id: U256,
		settlement_id: U256,
		tranches: Vec<EvmTrancheSettle>,
		pending_deposit_assets: U256,
		product_nav: U256,
	) -> EvmResult {
		let caller = handle.context().caller;
		let product_id = to_product_id(product_id)?;
		ensure_caller_is_valuation::<Runtime>(product_id, caller)?;
		let bounded_tranches = decode_tranche_settles(&tranches)?;

		let call = InvestmentsCall::<Runtime>::record_tranche_settlement {
			product_id,
			settlement_id,
			tranches: bounded_tranches,
			pending_deposit_assets,
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
			SELECTOR_LOG_TRANCHE_SETTLEMENT_RECORDED,
			solidity::encode_event_data((
				U256::from(product_id),
				settlement_id,
				pending_deposit_assets,
				product_nav,
			)),
		);
		handle.record_log_costs(&[&event])?;
		event.record(handle)?;

		Ok(())
	}

	/// Read a product's current settlement_id — the value most recently passed to
	/// `record_tranche_settlement`. Zero if the product has never settled yet.
	///
	/// @param product_id The product to look up
	#[precompile::public("get_settlement_id(uint256)")]
	#[precompile::view]
	fn get_settlement_id(handle: &mut impl PrecompileHandle, product_id: U256) -> EvmResult<U256> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let product_id = to_product_id(product_id)?;
		Ok(pallet_tranche_investments::LastSettlementId::<Runtime>::get(product_id)
			.unwrap_or_default())
	}

	/// Enumerate pending (unapproved) request IDs for a product, scoped to one
	/// settlement batch, with offset/limit pagination.
	///
	/// @param product_id    The product to look up
	/// @param settlement_id Only requests recorded against this settlement cycle are returned
	/// @param offset        Number of matching entries to skip
	/// @param limit         Maximum number of entries to return
	#[precompile::public("get_pending_requests(uint256,uint256,uint256,uint256)")]
	#[precompile::view]
	fn get_pending_requests(
		handle: &mut impl PrecompileHandle,
		product_id: U256,
		settlement_id: U256,
		offset: U256,
		limit: U256,
	) -> EvmResult<Vec<H256>> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let product_id = to_product_id(product_id)?;
		let offset = to_u64(offset)?;
		let limit = to_u64(limit)?;

		let mut ids = Vec::new();
		let mut skipped = 0u64;
		let mut collected = 0u64;
		for (request_id, requested) in
			pallet_tranche_investments::RequestedInvestments::<Runtime>::iter_prefix(product_id)
		{
			if requested.settlement_id != settlement_id {
				continue;
			}
			if skipped < offset {
				skipped += 1;
				continue;
			}
			if collected >= limit {
				break;
			}
			ids.push(request_id);
			collected += 1;
		}
		Ok(ids)
	}

	/// Read a single request's current state — pending or approved.
	///
	/// @param product_id The product the request belongs to
	/// @param request_id The request to look up
	/// @return investor, vault_chain_id, vault, amount, settlement_id, order_type, status
	/// (status: 0 = pending, 1 = approved)
	#[precompile::public("get_request(uint256,bytes32)")]
	#[precompile::view]
	fn get_request(
		handle: &mut impl PrecompileHandle,
		product_id: U256,
		request_id: H256,
	) -> EvmResult<(Address, u64, Address, U256, U256, u8, u8)> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let product_id = to_product_id(product_id)?;

		if let Some(requested) =
			pallet_tranche_investments::RequestedInvestments::<Runtime>::get(product_id, request_id)
		{
			return Ok((
				Address(requested.investor_address),
				requested.vault.chain_id,
				Address(requested.vault.vault_address),
				requested.amount,
				requested.settlement_id,
				encode_order_type(requested.order_type),
				0u8,
			));
		}
		if let Some(approved) =
			pallet_tranche_investments::ApprovedInvestments::<Runtime>::get(product_id, request_id)
		{
			let requested = &approved.requested;
			return Ok((
				Address(requested.investor_address),
				requested.vault.chain_id,
				Address(requested.vault.vault_address),
				requested.amount,
				approved.settlement_id,
				encode_order_type(requested.order_type),
				1u8,
			));
		}
		Err(revert("request not found"))
	}

	/// Read a tranche's outstanding units and Senior principal claim, as of the
	/// product's most recently recorded settlement.
	///
	/// @param product_id The product the tranche belongs to
	/// @param tranche    The tranche's identifying vault (chain_id, vault_address)
	/// @return units_outstanding, principal
	#[precompile::public("get_tranche_state(uint256,(uint64,address))")]
	#[precompile::view]
	fn get_tranche_state(
		handle: &mut impl PrecompileHandle,
		product_id: U256,
		tranche: EvmVaultInput,
	) -> EvmResult<(U256, U256)> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let product_id = to_product_id(product_id)?;
		let (chain_id, vault_address) = tranche;
		let vault = VaultId { chain_id, vault_address: vault_address.0 };

		let last_id = pallet_tranche_investments::LastSettlementId::<Runtime>::get(product_id)
			.ok_or_else(|| revert("product has no recorded settlement"))?;
		let settlement =
			pallet_tranche_investments::TrancheSettlements::<Runtime>::get(product_id, last_id)
				.ok_or_else(|| revert("product has no recorded settlement"))?;
		let settle = settlement
			.tranches
			.iter()
			.find(|s| s.vault == vault)
			.ok_or_else(|| revert("tranche not found in latest settlement"))?;
		Ok((settle.units_outstanding, settle.principal))
	}

	/// Read a product's pending (unconfirmed) deposit total, as of the product's
	/// most recently recorded settlement. Zero if the product has never settled.
	///
	/// @param product_id The product to look up
	#[precompile::public("get_pending_deposit_assets(uint256)")]
	#[precompile::view]
	fn get_pending_deposit_assets(
		handle: &mut impl PrecompileHandle,
		product_id: U256,
	) -> EvmResult<U256> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let product_id = to_product_id(product_id)?;
		let Some(last_id) =
			pallet_tranche_investments::LastSettlementId::<Runtime>::get(product_id)
		else {
			return Ok(U256::zero());
		};
		Ok(pallet_tranche_investments::TrancheSettlements::<Runtime>::get(product_id, last_id)
			.map(|s| s.pending_deposit_assets)
			.unwrap_or_default())
	}

	/// Read a product's most recently recorded settlement: its settlement_id, each
	/// tranche's share price/NAV (ordered by tranche priority — see TrancheSystem's
	/// `get_tranches`, NOT the order Valuation happened to submit them in), and the
	/// product's finalized aggregate NAV.
	///
	/// @param product_id The product to look up
	#[precompile::public("get_last_settlement(uint256)")]
	#[precompile::view]
	fn get_last_settlement(
		handle: &mut impl PrecompileHandle,
		product_id: U256,
	) -> EvmResult<(U256, Vec<U256>, Vec<U256>, U256)> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let product_id = to_product_id(product_id)?;

		let last_id = pallet_tranche_investments::LastSettlementId::<Runtime>::get(product_id)
			.ok_or_else(|| revert("product has no recorded settlement"))?;
		let settlement =
			pallet_tranche_investments::TrancheSettlements::<Runtime>::get(product_id, last_id)
				.ok_or_else(|| revert("product has no recorded settlement"))?;
		let product_nav =
			pallet_tranche_investments::ProductNavs::<Runtime>::get(product_id, last_id)
				.ok_or_else(|| revert("product has no recorded settlement"))?;

		let product = pallet_tranche_system::Products::<Runtime>::get(product_id)
			.ok_or_else(|| revert("product not found"))?;

		let mut share_prices = Vec::with_capacity(product.tranches.len());
		let mut tranche_navs = Vec::with_capacity(product.tranches.len());
		for tranche in product.tranches.iter() {
			let settle = settlement
				.tranches
				.iter()
				.find(|s| s.vault == tranche.vault)
				.ok_or_else(|| revert("tranche missing from latest settlement"))?;
			share_prices.push(settle.share_price);
			tranche_navs.push(settle.tranche_nav);
		}

		Ok((last_id, share_prices, tranche_navs, product_nav))
	}

	/// Read a request's approval details.
	///
	/// @param product_id The product the request belongs to
	/// @param request_id The request to look up
	/// @return settlement_id, claimable_assets, status (always 1 = approved; reverts if
	/// no approval is recorded for `request_id`)
	#[precompile::public("get_approval(uint256,bytes32)")]
	#[precompile::view]
	fn get_approval(
		handle: &mut impl PrecompileHandle,
		product_id: U256,
		request_id: H256,
	) -> EvmResult<(U256, U256, u8)> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let product_id = to_product_id(product_id)?;
		let approved =
			pallet_tranche_investments::ApprovedInvestments::<Runtime>::get(product_id, request_id)
				.ok_or_else(|| revert("approval not found"))?;
		Ok((approved.settlement_id, approved.claimable_assets, 1u8))
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

fn encode_order_type(order_type: OrderType) -> u8 {
	match order_type {
		OrderType::Redeem => 0,
		OrderType::Deposit => 1,
	}
}

/// Reverts rather than silently truncating if `value` doesn't fit `u64` —
/// shared by `get_pending_requests`'s `offset`/`limit` parameters.
fn to_u64(value: U256) -> EvmResult<u64> {
	if value > U256::from(u64::MAX) {
		return Err(revert("value exceeds u64::MAX"));
	}
	Ok(value.as_u64())
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

fn decode_tranche_settles(
	tranches: &[EvmTrancheSettle],
) -> EvmResult<BoundedVec<TrancheSettle, ConstU32<{ pallet_tranche_system::MAX_TRANCHES }>>> {
	let mut bounded =
		BoundedVec::<TrancheSettle, ConstU32<{ pallet_tranche_system::MAX_TRANCHES }>>::default();
	for (vault_chain_id, vault_address, tranche_nav, share_price, units_outstanding, principal) in
		tranches.iter().cloned()
	{
		let vault = VaultId { chain_id: vault_chain_id, vault_address: vault_address.0 };
		bounded
			.try_push(TrancheSettle {
				vault,
				tranche_nav,
				share_price,
				units_outstanding,
				principal,
			})
			.map_err(|_| revert("too many tranches"))?;
	}
	Ok(bounded)
}
