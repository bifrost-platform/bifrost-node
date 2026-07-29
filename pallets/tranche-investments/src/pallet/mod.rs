use crate::{
	AdapterValuation, Allocation, ApprovedInvestment, OrderType, RequestId, RequestedInvestment,
	SettlementId, WeightInfo, MAX_ADAPTER_VALUATIONS, MAX_ALLOCATIONS,
};
use pallet_tranche_system::{AdapterInspect, AdapterKey, ProductId, VaultId, VaultInspect};

use frame_support::{pallet_prelude::*, traits::StorageVersion};
use frame_system::pallet_prelude::*;
use sp_core::{ConstU32, H160, U256};
use sp_runtime::BoundedVec;
use sp_std::collections::btree_set::BTreeSet;

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
		/// Only accepted origin for all four extrinsics in this pallet.
		/// Wire as `pallet_tranche_investments::EnsureValuation` in the runtime
		/// so that only the tranche-investments precompile can invoke them.
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
		/// A product NAV was already recorded for this
		/// (product_id, settlement_id).
		ProductNavAlreadyRecorded,
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
			claimable_assets: U256,
		},
		/// Per-Adapter NAV breakdown was recorded for a settlement.
		AdapterValuationsRecorded { product_id: ProductId, settlement_id: SettlementId },
		/// A product's aggregate NAV was recorded for a settlement.
		ProductNavRecorded { product_id: ProductId, settlement_id: SettlementId, product_nav: U256 },
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
		/// allocation breakdown, plus what the investor can claim as a result.
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
			claimable_assets: U256,
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

			RequestedInvestments::<T>::remove(product_id, request_id);
			ApprovedInvestments::<T>::insert(
				product_id,
				request_id,
				ApprovedInvestment { requested, settlement_id, allocations, claimable_assets },
			);

			Self::deposit_event(Event::InvestmentApproved {
				product_id,
				request_id,
				settlement_id,
				claimable_assets,
			});
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

		/// Record the settlement's finalized aggregate NAV across all of the
		/// product's sources. Origin must be `ValuationOrigin`. Callable at
		/// most once per (product_id, settlement_id).
		#[pallet::call_index(3)]
		#[pallet::weight(<T as Config>::WeightInfo::record_product_nav())]
		pub fn record_product_nav(
			origin: OriginFor<T>,
			product_id: ProductId,
			settlement_id: SettlementId,
			product_nav: U256,
		) -> DispatchResult {
			T::ValuationOrigin::ensure_origin(origin)?;

			ensure!(
				!ProductNavs::<T>::contains_key(product_id, settlement_id),
				Error::<T>::ProductNavAlreadyRecorded
			);

			ProductNavs::<T>::insert(product_id, settlement_id, product_nav);

			Self::deposit_event(Event::ProductNavRecorded {
				product_id,
				settlement_id,
				product_nav,
			});
			Ok(())
		}
	}
}
