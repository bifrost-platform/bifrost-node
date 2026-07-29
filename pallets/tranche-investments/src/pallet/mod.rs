use crate::{
	AdapterValuation, ApprovedInvestment, RequestId, RequestedInvestment, SettlementId,
	MAX_ADAPTER_VALUATIONS,
};
use pallet_tranche_system::ProductId;

use frame_support::{pallet_prelude::*, traits::StorageVersion};
use sp_core::{ConstU32, U256};
use sp_runtime::BoundedVec;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::origin]
	#[derive(
		Clone,
		PartialEq,
		Eq,
		RuntimeDebug,
		Encode,
		Decode,
		DecodeWithMemTracking,
		TypeInfo,
		MaxEncodedLen,
	)]
	pub enum Origin {
		/// Dispatched by the tranche-investments precompile, after it has itself
		/// verified `handle.context().caller` against the calling product's
		/// registered `valuation_address` (read from pallet-tranche-system) —
		/// mirrors the old investments pallet trusting a precompile-checked
		/// `Origin::Gateway`. This pallet does not re-verify the caller itself;
		/// it trusts that check already happened at the precompile boundary.
		Valuation,
	}

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Only accepted origin for all five extrinsics in this pallet.
		/// Wire as `pallet_tranche_investments::EnsureValuation` in the runtime
		/// so that only the tranche-investments precompile can invoke them.
		type ValuationOrigin: frame_support::traits::EnsureOrigin<Self::RuntimeOrigin>;
	}

	// -----------------------------------------------------------------------
	// Errors / Events / Extrinsics — next pass: record_investment_request,
	// record_investment_approval, record_investment_cancellation,
	// record_adapter_valuations, record_product_nav. This pass is storage
	// only, per the current design step. Cancellation's request_id
	// replay-guard is explicitly deferred (not yet designed).
	// -----------------------------------------------------------------------

	// -----------------------------------------------------------------------
	// Storage
	// -----------------------------------------------------------------------

	#[pallet::storage]
	/// Requests recorded the moment they reach the Valuation Contract, before
	/// any Adapter allocation. Removed on approval (moved into
	/// `ApprovedInvestments`) or cancellation.
	///
	/// Keyed by `(product_id, request_id)`, NOT `request_id` alone: each
	/// product has its own independent Valuation Contract generating
	/// `request_id`s, so the same `request_id` value can plausibly recur
	/// across different products — every `record_*` function in interface.sol
	/// takes `product_id` alongside `request_id`/`settlement_id` precisely
	/// because they're only unique within a product's own namespace.
	pub type RequestedInvestments<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		ProductId,
		Blake2_128Concat,
		RequestId,
		RequestedInvestment,
	>;

	#[pallet::storage]
	/// Fully allocated and priced requests. See `ApprovedInvestment`'s doc
	/// comment for why this embeds the full original request rather than a
	/// differently-shaped record. Keyed by `(product_id, request_id)` — see
	/// `RequestedInvestments`' doc comment for why.
	pub type ApprovedInvestments<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		ProductId,
		Blake2_128Concat,
		RequestId,
		ApprovedInvestment,
	>;

	#[pallet::storage]
	/// Per-Adapter NAV breakdown for a completed settlement, as recorded by
	/// `record_adapter_valuations`. Keyed by `(product_id, settlement_id)`.
	pub type AdapterValuations<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		ProductId,
		Blake2_128Concat,
		SettlementId,
		BoundedVec<AdapterValuation, ConstU32<MAX_ADAPTER_VALUATIONS>>,
	>;

	#[pallet::storage]
	/// Settlement's finalized aggregate NAV across all of the product's
	/// sources, as recorded by `record_product_nav` -- a separate call/entry
	/// from `AdapterValuations`, not derived from it (Valuation is trusted for
	/// the aggregation, not independently re-checked against the per-Adapter
	/// breakdown). Keyed by `(product_id, settlement_id)`.
	pub type ProductNavs<T: Config> =
		StorageDoubleMap<_, Blake2_128Concat, ProductId, Blake2_128Concat, SettlementId, U256>;

	// -----------------------------------------------------------------------
	// Extrinsics — next pass: record_investment_request,
	// record_investment_approval, record_investment_cancellation,
	// record_adapter_valuations, record_product_nav
	// -----------------------------------------------------------------------
}
