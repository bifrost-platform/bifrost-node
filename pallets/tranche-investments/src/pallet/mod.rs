use crate::{
	migrations, AdapterValuation, Allocation, ApprovedInvestment, InvestmentApprovalInput,
	OrderType, RequestId, RequestedInvestment, Settlement, SettlementId, TrancheSettle, WeightInfo,
	MAX_ADAPTER_VALUATIONS, MAX_ALLOCATIONS, MAX_SETTLEMENT_REQUESTS,
};
use pallet_tranche_system::{AdapterInspect, AdapterKey, ProductId, VaultId, VaultInspect};

use frame_support::{
	pallet_prelude::*,
	traits::{OnRuntimeUpgrade, StorageVersion},
};
use frame_system::pallet_prelude::*;
use sp_core::{ConstU32, H160, U256};
use sp_runtime::BoundedVec;
use sp_std::collections::btree_set::BTreeSet;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(3);

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
	pub trait Config: frame_system::Config + pallet_timestamp::Config<Moment = u64> {
		/// Only accepted origin for every extrinsic in this pallet
		/// (`record_investment_request`/`record_investment_approval`/
		/// `record_investment_approvals`/`record_adapter_valuations`/
		/// `record_settlement`). Wire as `pallet_tranche_investments::EnsureValuation`
		/// in the runtime so that only the tranche-investments precompile can
		/// invoke them.
		type ValuationOrigin: frame_support::traits::EnsureOrigin<Self::RuntimeOrigin>;
		/// Vault inspector — implemented by pallet-tranche-system. Used to
		/// verify a vault actually belongs to `product_id` before recording a
		/// request against it.
		type Vaults: VaultInspect;
		/// Adapter inspector — implemented by pallet-tranche-system. Used to
		/// verify an `Allocation`'s MultichainAdapter, or an `AdapterValuation`'s
		/// individual Adapter, actually belongs to `product_id` before recording
		/// against it.
		type Adapters: AdapterInspect;
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	// -----------------------------------------------------------------------
	// Errors
	// -----------------------------------------------------------------------

	#[pallet::error]
	pub enum Error<T> {
		/// The vault does not belong to `product_id`.
		VaultNotRegistered,
		/// `record_investment_request` was called with `amount == 0`.
		ZeroAmount,
		/// `record_investment_request` was called with a zero `investor_address`.
		ZeroInvestorAddress,
		/// `request_id` is already in use (pending or approved) for this product.
		DuplicateRequestId,
		/// No pending request exists for `request_id` on this product.
		RequestNotFound,
		/// The same MultichainAdapter appears twice in one `allocations` array.
		DuplicateAllocationAdapter,
		/// An `allocations` entry's MultichainAdapter doesn't belong to
		/// `product_id`.
		AllocationAdapterNotRegistered,
		/// Summing `allocations[i].amount` overflowed `U256`.
		AllocationSumOverflow,
		/// The sum of `allocations[i].amount` doesn't equal the original
		/// request's `amount`.
		AllocationSumMismatch,
		/// The same (chain_id, adapter) appears twice in one `valuations` array.
		DuplicateAdapterValuationEntry,
		/// A `valuations` entry's Adapter doesn't belong to `product_id`.
		AdapterValuationAdapterNotRegistered,
		/// Adapter valuations were already recorded for this
		/// (product_id, settlement_id).
		AdapterValuationsAlreadyRecorded,
		/// The same vault appears twice in one `tranches` array.
		DuplicateTrancheSettleEntry,
		/// A tranche settlement was already recorded for this
		/// (product_id, settlement_id).
		TrancheSettlementAlreadyRecorded,
		/// `(product_id, settlement_id)` already has `MAX_SETTLEMENT_REQUESTS`
		/// requests approved into it.
		TooManySettlementRequests,
	}

	// -----------------------------------------------------------------------
	// Events
	// -----------------------------------------------------------------------

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A deposit/redeem request was recorded.
		InvestmentRequested {
			product_id: ProductId,
			vault: VaultId,
			investor_address: H160,
			amount: U256,
			request_id: RequestId,
			settlement_id: SettlementId,
			order_type: OrderType,
		},
		/// A pending request was fully allocated and approved.
		InvestmentApproved {
			product_id: ProductId,
			request_id: RequestId,
			settlement_id: SettlementId,
			receivable_amount: U256,
		},
		/// Per-Adapter NAV breakdown was recorded for a settlement.
		AdapterValuationsRecorded { product_id: ProductId, settlement_id: SettlementId },
		/// Post-waterfall per-tranche settlement results (and the product's
		/// aggregate NAV) were recorded for a settlement.
		TrancheSettlementRecorded {
			product_id: ProductId,
			settlement_id: SettlementId,
			pending_deposit_assets: U256,
			product_nav: U256,
		},
	}

	// -----------------------------------------------------------------------
	// Storage
	// -----------------------------------------------------------------------

	#[pallet::storage]
	/// Requests recorded the moment they reach the Valuation Contract, before
	/// any Adapter allocation. Removed on approval (moved into
	/// `ApprovedInvestments`).
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
		RequestedInvestment<BlockNumberFor<T>>,
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
		ApprovedInvestment<BlockNumberFor<T>>,
	>;

	#[pallet::storage]
	/// Reverse index: every request_id approved into a given (product_id,
	/// settlement_id), written alongside `ApprovedInvestments` by
	/// `record_investment_approval`. Originally read cross-pallet by
	/// pallet-tranche-tx-registry (via the now-removed `RequestSettlementInspect`
	/// trait) to answer "which requests does this settlement cover" when closing
	/// out its own `InvestorActiveRequests` entries — that pallet now keeps an
	/// independent, event-sourced copy of the same linkage instead (see
	/// `pallet_tranche_tx_registry::SettlementRequests`'s doc comment), so this
	/// storage is this pallet's own bookkeeping only from here on.
	pub type SettlementRequests<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		ProductId,
		Blake2_128Concat,
		SettlementId,
		BoundedVec<RequestId, ConstU32<MAX_SETTLEMENT_REQUESTS>>,
		ValueQuery,
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
	/// sources, as recorded by `record_settlement` alongside the
	/// per-tranche breakdown -- a separate entry from `AdapterValuations`,
	/// not derived from it (Valuation is trusted for the aggregation, not
	/// independently re-checked against the per-Adapter breakdown). Keyed by
	/// `(product_id, settlement_id)`.
	pub type ProductNavs<T: Config> =
		StorageDoubleMap<_, Blake2_128Concat, ProductId, Blake2_128Concat, SettlementId, U256>;

	#[pallet::storage]
	/// Post-waterfall per-tranche settlement result for a completed
	/// settlement, as recorded by `record_settlement`. Keyed by
	/// `(product_id, settlement_id)`, same rationale as `AdapterValuations`/
	/// `ProductNavs`. `units_outstanding`/`principal` are overwritten
	/// wholesale by each new settlement — nothing else in this pallet
	/// separately accumulates them.
	pub type Settlements<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		ProductId,
		Blake2_128Concat,
		SettlementId,
		Settlement<BlockNumberFor<T>>,
	>;

	#[pallet::storage]
	/// The most recently recorded `settlement_id` per product, written
	/// alongside `Settlements`/`ProductNavs` by `record_settlement`.
	/// The Valuation Contract remains the source of truth for settlement_id
	/// assignment/incrementing (this pallet never generates or advances it on
	/// its own) — this pointer exists purely to serve read-side queries that
	/// need "the latest settlement" without a `settlement_id` parameter
	/// (`get_settlement_id`/`get_tranche_state`/`get_pending_deposit_assets`/
	/// `get_last_settlement` on the precompile). It plays no role in
	/// `record_settlement`'s own duplicate-write check, which still
	/// keys off `Settlements::contains_key` directly.
	///
	/// `record_settlement` only ever advances this — it writes `settlement_id`
	/// iff it's strictly greater than what's already stored (or nothing is).
	/// The Valuation Contract assigns settlement_ids monotonically, but a cycle
	/// recorded out of order (a stuck/retried settlement tx landing after a
	/// later cycle's, a reorg) must not drag "the latest settlement" backwards
	/// — same guard and rationale as
	/// `pallet_tranche_tx_registry::LatestWhitelistNonce`.
	pub type LastSettlementId<T: Config> = StorageMap<_, Blake2_128Concat, ProductId, SettlementId>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_runtime_upgrade() -> Weight {
			migrations::v3::MigrateToV3::<T>::on_runtime_upgrade()
		}
	}

	// -----------------------------------------------------------------------
	// Extrinsics
	// -----------------------------------------------------------------------

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Record a pending deposit or redeem request as it arrives at the
		/// Valuation Contract, before any Adapter allocation has happened.
		/// Origin must be `ValuationOrigin`.
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::record_investment_request())]
		pub fn record_investment_request(
			origin: OriginFor<T>,
			product_id: ProductId,
			request_id: RequestId,
			settlement_id: SettlementId,
			vault_chain_id: u64,
			vault_address: H160,
			investor_address: H160,
			amount: U256,
			order_type: OrderType,
		) -> DispatchResult {
			T::ValuationOrigin::ensure_origin(origin)?;

			let vault = VaultId { chain_id: vault_chain_id, vault_address };
			ensure!(
				T::Vaults::vault_belongs_to_product(product_id, &vault),
				Error::<T>::VaultNotRegistered
			);
			ensure!(!amount.is_zero(), Error::<T>::ZeroAmount);
			ensure!(!investor_address.is_zero(), Error::<T>::ZeroInvestorAddress);
			ensure!(
				!RequestedInvestments::<T>::contains_key(product_id, request_id)
					&& !ApprovedInvestments::<T>::contains_key(product_id, request_id),
				Error::<T>::DuplicateRequestId
			);

			RequestedInvestments::<T>::insert(
				product_id,
				request_id,
				RequestedInvestment {
					product_id,
					settlement_id,
					vault: vault.clone(),
					investor_address,
					amount,
					order_type,
					recorded_at: frame_system::Pallet::<T>::block_number(),
					timestamp: pallet_timestamp::Pallet::<T>::get(),
				},
			);

			Self::deposit_event(Event::InvestmentRequested {
				product_id,
				vault,
				investor_address,
				amount,
				request_id,
				settlement_id,
				order_type,
			});
			Ok(())
		}

		/// Record a pending request's full approval: the complete Adapter
		/// allocation breakdown, plus what the investor can receive as a result.
		/// Origin must be `ValuationOrigin`. Reverts unless
		/// `sum(allocations[i].amount) == requested.amount`.
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::record_investment_approval())]
		pub fn record_investment_approval(
			origin: OriginFor<T>,
			product_id: ProductId,
			request_id: RequestId,
			settlement_id: SettlementId,
			allocations: BoundedVec<Allocation, ConstU32<MAX_ALLOCATIONS>>,
			receivable_amount: U256,
		) -> DispatchResult {
			T::ValuationOrigin::ensure_origin(origin)?;

			let requested = RequestedInvestments::<T>::get(product_id, request_id)
				.ok_or(Error::<T>::RequestNotFound)?;

			let mut seen = BTreeSet::new();
			let mut sum = U256::zero();
			for allocation in allocations.iter() {
				ensure!(
					seen.insert(allocation.adapter.clone()),
					Error::<T>::DuplicateAllocationAdapter
				);
				ensure!(
					T::Adapters::multichain_adapter_belongs_to_product(
						product_id,
						&allocation.adapter
					),
					Error::<T>::AllocationAdapterNotRegistered
				);
				sum =
					sum.checked_add(allocation.amount).ok_or(Error::<T>::AllocationSumOverflow)?;
			}
			ensure!(sum == requested.amount, Error::<T>::AllocationSumMismatch);

			let mut settlement_requests = SettlementRequests::<T>::get(product_id, settlement_id);
			settlement_requests
				.try_push(request_id)
				.map_err(|_| Error::<T>::TooManySettlementRequests)?;

			RequestedInvestments::<T>::remove(product_id, request_id);
			ApprovedInvestments::<T>::insert(
				product_id,
				request_id,
				ApprovedInvestment {
					requested,
					settlement_id,
					allocations,
					receivable_amount,
					recorded_at: frame_system::Pallet::<T>::block_number(),
					timestamp: pallet_timestamp::Pallet::<T>::get(),
				},
			);
			SettlementRequests::<T>::insert(product_id, settlement_id, settlement_requests);

			Self::deposit_event(Event::InvestmentApproved {
				product_id,
				request_id,
				settlement_id,
				receivable_amount,
			});
			Ok(())
		}

		/// Batch form of `record_investment_approval` — records every entry in
		/// `approvals` against the same `(product_id, settlement_id)` in one
		/// extrinsic, for a Valuation Contract that resolves an entire
		/// settlement's approvals in one pass rather than one call per
		/// `request_id`. Does the exact same per-entry work
		/// `record_investment_approval` does (allocation validation,
		/// `RequestedInvestments` -> `ApprovedInvestments` move,
		/// `SettlementRequests` append, `InvestmentApproved` event), repeated
		/// once per entry — kept as a fully separate extrinsic (rather than
		/// having `record_investment_approval` delegate to a shared helper) so
		/// the existing single-entry call path is untouched.
		/// Origin must be `ValuationOrigin`. Atomic like any other extrinsic: if
		/// any entry fails, every entry in this call — including ones already
		/// applied earlier in the same batch — is rolled back. A duplicate
		/// `request_id` within the same batch fails on its second occurrence
		/// with `RequestNotFound`, since the first occurrence already moved it
		/// out of `RequestedInvestments` — same outcome as calling
		/// `record_investment_approval` twice for the same `request_id`.
		/// Emits one `InvestmentApproved` per entry, in the same shape a caller
		/// would see from `record_investment_approval`, so indexers don't need
		/// to special-case this batch entry point.
		#[pallet::call_index(4)]
		#[pallet::weight(<T as Config>::WeightInfo::record_investment_approvals(approvals.len() as u32))]
		pub fn record_investment_approvals(
			origin: OriginFor<T>,
			product_id: ProductId,
			settlement_id: SettlementId,
			approvals: BoundedVec<InvestmentApprovalInput, ConstU32<MAX_SETTLEMENT_REQUESTS>>,
		) -> DispatchResult {
			T::ValuationOrigin::ensure_origin(origin)?;

			for InvestmentApprovalInput { request_id, allocations, receivable_amount } in approvals
			{
				let requested = RequestedInvestments::<T>::get(product_id, request_id)
					.ok_or(Error::<T>::RequestNotFound)?;

				let mut seen = BTreeSet::new();
				let mut sum = U256::zero();
				for allocation in allocations.iter() {
					ensure!(
						seen.insert(allocation.adapter.clone()),
						Error::<T>::DuplicateAllocationAdapter
					);
					ensure!(
						T::Adapters::multichain_adapter_belongs_to_product(
							product_id,
							&allocation.adapter
						),
						Error::<T>::AllocationAdapterNotRegistered
					);
					sum = sum
						.checked_add(allocation.amount)
						.ok_or(Error::<T>::AllocationSumOverflow)?;
				}
				ensure!(sum == requested.amount, Error::<T>::AllocationSumMismatch);

				let mut settlement_requests =
					SettlementRequests::<T>::get(product_id, settlement_id);
				settlement_requests
					.try_push(request_id)
					.map_err(|_| Error::<T>::TooManySettlementRequests)?;

				RequestedInvestments::<T>::remove(product_id, request_id);
				ApprovedInvestments::<T>::insert(
					product_id,
					request_id,
					ApprovedInvestment {
						requested,
						settlement_id,
						allocations,
						receivable_amount,
						recorded_at: frame_system::Pallet::<T>::block_number(),
						timestamp: pallet_timestamp::Pallet::<T>::get(),
					},
				);
				SettlementRequests::<T>::insert(product_id, settlement_id, settlement_requests);

				Self::deposit_event(Event::InvestmentApproved {
					product_id,
					request_id,
					settlement_id,
					receivable_amount,
				});
			}

			Ok(())
		}

		/// Record the finalized per-Adapter NAV breakdown for a settlement.
		/// Origin must be `ValuationOrigin`. Callable at most once per
		/// (product_id, settlement_id).
		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::record_adapter_valuations())]
		pub fn record_adapter_valuations(
			origin: OriginFor<T>,
			product_id: ProductId,
			settlement_id: SettlementId,
			valuations: BoundedVec<AdapterValuation, ConstU32<MAX_ADAPTER_VALUATIONS>>,
		) -> DispatchResult {
			T::ValuationOrigin::ensure_origin(origin)?;

			ensure!(
				!AdapterValuations::<T>::contains_key(product_id, settlement_id),
				Error::<T>::AdapterValuationsAlreadyRecorded
			);

			let mut seen = BTreeSet::new();
			for valuation in valuations.iter() {
				let key = AdapterKey { address: valuation.adapter, chain_id: valuation.chain_id };
				ensure!(seen.insert(key.clone()), Error::<T>::DuplicateAdapterValuationEntry);
				ensure!(
					T::Adapters::adapter_belongs_to_product(product_id, &key),
					Error::<T>::AdapterValuationAdapterNotRegistered
				);
			}

			AdapterValuations::<T>::insert(product_id, settlement_id, valuations);

			Self::deposit_event(Event::AdapterValuationsRecorded { product_id, settlement_id });
			Ok(())
		}

		/// Record the post-waterfall per-tranche settlement result: each
		/// tranche's NAV/share price/units/principal, the product's pending
		/// deposit total, and the product's finalized aggregate NAV (formerly
		/// `record_product_nav`, folded in here so both are recorded
		/// atomically in one call). Origin must be `ValuationOrigin`. Callable
		/// at most once per (product_id, settlement_id).
		#[pallet::call_index(3)]
		#[pallet::weight(<T as Config>::WeightInfo::record_settlement())]
		pub fn record_settlement(
			origin: OriginFor<T>,
			product_id: ProductId,
			settlement_id: SettlementId,
			tranches: BoundedVec<
				TrancheSettle,
				ConstU32<{ pallet_tranche_system::MAX_TRANCHE_INPUTS }>,
			>,
			pending_deposit_assets: U256,
			product_nav: U256,
		) -> DispatchResult {
			T::ValuationOrigin::ensure_origin(origin)?;

			ensure!(
				!Settlements::<T>::contains_key(product_id, settlement_id),
				Error::<T>::TrancheSettlementAlreadyRecorded
			);

			let mut seen = BTreeSet::new();
			for settle in tranches.iter() {
				ensure!(seen.insert(settle.vault.clone()), Error::<T>::DuplicateTrancheSettleEntry);
				ensure!(
					T::Vaults::vault_belongs_to_product(product_id, &settle.vault),
					Error::<T>::VaultNotRegistered
				);
			}

			Settlements::<T>::insert(
				product_id,
				settlement_id,
				Settlement {
					tranches,
					pending_deposit_assets,
					recorded_at: frame_system::Pallet::<T>::block_number(),
					timestamp: pallet_timestamp::Pallet::<T>::get(),
				},
			);
			ProductNavs::<T>::insert(product_id, settlement_id, product_nav);
			// Advance only — never let an out-of-order (stuck/retried/reorged)
			// cycle drag "the latest settlement" backwards. See
			// `LastSettlementId`'s storage doc comment.
			LastSettlementId::<T>::mutate(product_id, |last| {
				if last.map_or(true, |current| settlement_id > current) {
					*last = Some(settlement_id);
				}
			});

			Self::deposit_event(Event::TrancheSettlementRecorded {
				product_id,
				settlement_id,
				pending_deposit_assets,
				product_nav,
			});
			Ok(())
		}
	}
}
