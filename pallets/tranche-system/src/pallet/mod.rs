mod impls;

use crate::{
	AdapterInfo, AdapterKey, CrudAction, MultichainAdapterInfo, PermissionInspect, ProductDetails,
	ProductId, Tranche, TrancheType, ValuationInfo, VaultId, VaultInspect, WeightInfo,
	MAX_ADAPTERS_PER_MULTICHAIN_ADAPTER, MAX_MULTICHAIN_ADAPTERS, MAX_TRANCHES,
};

use frame_support::{pallet_prelude::*, traits::StorageVersion};
use frame_system::pallet_prelude::*;
use sp_core::H160;

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
	pub enum Origin<T: Config> {
		/// Dispatched by the tranche-system precompile after verifying the
		/// caller holds ProductAdmin for the product being acted on (via
		/// pallet-tranche-permissions). Carries that verified account so
		/// `create_product` can populate `ProductCreated`'s `product_admin`
		/// field without re-deriving it — mirrors
		/// `frame_system::RawOrigin::Signed`. Used to ensure `create_product`
		/// can only be called through the precompile — mirrors pallet-pools'
		/// `Origin::PoolAdmin`.
		ProductAdmin(T::AccountId),
	}

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Only accepted origin for `create_product`. `set_tranche`,
		/// `set_adapters`, and `set_multichain_adapters` are gated differently
		/// — via a plain signed origin checked against `Permissions` inline —
		/// since, unlike `create_product`, they don't need to surface the
		/// admin's identity in an event.
		/// Wire as `pallet_tranche_system::EnsureProductAdmin<Runtime>` in the
		/// runtime so that only the tranche-system precompile can invoke
		/// `create_product`.
		type ProductAdminOrigin: frame_support::traits::EnsureOrigin<
			Self::RuntimeOrigin,
			Success = Self::AccountId,
		>;
		/// Permission inspector — implemented by pallet-tranche-permissions.
		/// Used to gate `set_tranche`, `set_adapters`, and
		/// `set_multichain_adapters` (checked inline via `ensure_signed` +
		/// `is_product_admin`); `create_product` is gated via
		/// `ProductAdminOrigin` instead, but the precompile that constructs
		/// that origin ultimately consults the same source of truth. Borrower
		/// identity is not gated here at all — it lives directly on each
		/// OffchainSource adapter (see `SourceType`), not as a
		/// permissions-pallet role.
		type Permissions: PermissionInspect<Self::AccountId>;
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	// -----------------------------------------------------------------------
	// Errors
	// -----------------------------------------------------------------------

	#[pallet::error]
	pub enum Error<T> {
		/// `product_id` is already taken by an existing product.
		ProductAlreadyExists,
		/// No product exists for `product_id`.
		ProductNotFound,
		/// `create_product` requires at least one tranche.
		EmptyTranches,
		/// A product cannot hold more than `MAX_TRANCHES` tranches.
		TooManyTranches,
		/// Caller does not hold the ProductAdmin role for this product.
		NotProductAdmin,
		/// The vault (chain_id, vault_address) is already registered — either
		/// to this product or a different one.
		VaultAlreadyRegistered,
		/// No tranche with the given vault exists for this product.
		VaultNotFound,
		/// `priority` is beyond the current number of tranches — can only
		/// insert at an existing slot or immediately after the last one.
		InvalidPriority,
		/// The MultichainAdapter (address, chain_id) is already registered —
		/// either to this product or a different one.
		MultichainAdapterAlreadyRegistered,
		/// No MultichainAdapter with the given (address, chain_id) exists for
		/// this product.
		MultichainAdapterNotFound,
		/// The Adapter address is already registered on this chain — either
		/// under a different parent MultichainAdapter, a different product, or
		/// (for `create_product`) duplicated within the same call.
		AdapterAlreadyRegistered,
		/// A `weightBps` set (top-level `multichain_adapters`, or one parent's
		/// nested `adapters`) must sum to exactly 10_000 (100%).
		WeightsMustSumTo10000,
	}

	// -----------------------------------------------------------------------
	// Events
	// -----------------------------------------------------------------------

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new product was created.
		ProductCreated {
			product_id: ProductId,
			product_admin: T::AccountId,
			valuation_address: H160,
			settlement_length_secs: u64,
			settlement_offset_secs: u64,
		},
		/// A tranche was added, removed, or updated.
		TrancheSet {
			product_id: ProductId,
			action: CrudAction,
			vault: VaultId,
			tranche_type: TrancheType,
			priority: u8,
		},
		/// A MultichainAdapter's nested adapters were replaced wholesale.
		AdaptersSet { product_id: ProductId, parent_adapter_address: H160, parent_chain_id: u64 },
		/// A product's entire MultichainAdapter table was replaced wholesale.
		MultichainAdaptersSet { product_id: ProductId },
	}

	// -----------------------------------------------------------------------
	// Storage
	// -----------------------------------------------------------------------

	#[pallet::storage]
	#[pallet::unbounded]
	/// All active products, keyed by product ID.
	pub type Products<T: Config> =
		StorageMap<_, Blake2_128Concat, ProductId, ProductDetails<T::AccountId>>;

	#[pallet::storage]
	/// Reverse index: which product a tranche's vault (chain_id, vault_address)
	/// belongs to. Globally unique across all products — enforces that the same
	/// vault can't be registered to two different products, and lets other
	/// pallets (tranche-investments, tranche-permissions) resolve a vault to its
	/// product without the caller supplying `product_id` up front.
	/// Mirrors pallet-pools' `Tranches: TrancheId -> PoolId`.
	pub type Vaults<T: Config> = StorageMap<_, Blake2_128Concat, VaultId, ProductId>;

	#[pallet::storage]
	/// Reverse index: which product an individual Adapter (source_address, chain_id)
	/// belongs to. Globally unique across all products, same rationale as `Vaults`.
	/// The adapter itself now lives nested inside its parent MultichainAdapter's
	/// `adapters` map (see `MultichainAdapterInfo`), keyed there by plain `H160`
	/// (no `chain_id` — it carries none of its own, see `AdapterInfo`) — this
	/// index still keys by the full `AdapterKey{address, chain_id}` shape, with
	/// `chain_id` derived from the parent MultichainAdapter at write time, so
	/// global uniqueness stays chain-aware (some on-chain protocols share the
	/// same contract address across different chains via CREATE2) without the
	/// adapter itself having to carry a redundant `chain_id` field.
	/// Mirrors pallet-pools' `Collaterals: CollateralAsset -> PoolId`.
	pub type AdapterIndex<T: Config> = StorageMap<_, Blake2_128Concat, AdapterKey, ProductId>;

	#[pallet::storage]
	/// Reverse index: which product a MultichainAdapter (adapter_address, chain_id)
	/// belongs to. A separate namespace from `AdapterIndex` even though the key
	/// shape is identical — globally unique across all products, same rationale.
	pub type MultichainAdapterIndex<T: Config> =
		StorageMap<_, Blake2_128Concat, AdapterKey, ProductId>;

	// -----------------------------------------------------------------------
	// Extrinsics
	// -----------------------------------------------------------------------

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Create a new tranche-system product: its Valuation contract binding,
		/// its tranches, and its MultichainAdapter routing table (each entry
		/// carrying its own nested individual-Adapter registrations).
		///
		/// Origin must be `ProductAdminOrigin` — the tranche-system precompile
		/// constructs it after verifying the caller holds ProductAdmin for
		/// `product_id` (granted up front via pallet-tranche-permissions,
		/// before this is ever called).
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::create_product())]
		pub fn create_product(
			origin: OriginFor<T>,
			product_id: ProductId,
			valuation: ValuationInfo,
			tranches: BoundedVec<Tranche, ConstU32<MAX_TRANCHES>>,
			multichain_adapters: BoundedBTreeMap<
				AdapterKey,
				MultichainAdapterInfo<T::AccountId>,
				ConstU32<MAX_MULTICHAIN_ADAPTERS>,
			>,
		) -> DispatchResult {
			let product_admin = T::ProductAdminOrigin::ensure_origin(origin)?;

			ensure!(!Products::<T>::contains_key(product_id), Error::<T>::ProductAlreadyExists);
			ensure!(!tranches.is_empty(), Error::<T>::EmptyTranches);

			Self::ensure_weights_sum_to_10000(
				multichain_adapters.values().map(|info| info.weight_bps),
			)?;
			for info in multichain_adapters.values() {
				Self::ensure_weights_sum_to_10000(info.adapters.values().map(|a| a.weight_bps))?;
			}

			Self::ensure_tranches_are_unregistered(tranches.iter())?;
			Self::ensure_multichain_adapters_are_unregistered(multichain_adapters.iter())?;

			for tranche in tranches.iter() {
				Vaults::<T>::insert(&tranche.vault, product_id);
			}
			Self::insert_multichain_adapter_index(product_id, multichain_adapters.iter());

			Self::deposit_event(Event::ProductCreated {
				product_id,
				product_admin,
				valuation_address: valuation.valuation_address,
				settlement_length_secs: valuation.settlement_length_secs,
				settlement_offset_secs: valuation.settlement_offset_secs,
			});

			Products::<T>::insert(
				product_id,
				ProductDetails { valuation, tranches, multichain_adapters },
			);

			Ok(())
		}

		/// Add, remove, or update a tranche on an existing product, identified
		/// by its vault. Caller must hold the ProductAdmin role for
		/// `product_id`, checked inline against `T::Permissions`.
		///
		/// Field usage differs by `action`, mirroring interface.sol's
		/// `set_tranche`:
		/// - `Add`: `vault` becomes the new tranche's identity (reverts if already registered to
		///   any product). `tranche_type` and `priority` are used. If `priority` is already
		///   occupied, the existing tranche at that slot (and everything after it) shifts down by
		///   one.
		/// - `Remove`: only `vault` is used, to find which tranche to remove. Every tranche after
		///   it shifts up by one, closing the gap. NOT YET CHECKED (deferred): interface.sol also
		///   specifies this should revert if the tranche has outstanding investments —
		///   pallet-tranche-investments doesn't expose an inspection trait for this yet.
		/// - `Update`: `vault` identifies which tranche to update (reverts if not found);
		///   `priority` is applied as a new value using the same insert-and-shift semantics as
		///   `Add`. `tranche_type` is ignored — fixed at `Add`, cannot be changed.
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::set_tranche())]
		pub fn set_tranche(
			origin: OriginFor<T>,
			product_id: ProductId,
			action: CrudAction,
			vault: VaultId,
			tranche_type: TrancheType,
			priority: u8,
		) -> DispatchResult {
			let caller = ensure_signed(origin)?;
			ensure!(
				T::Permissions::is_product_admin(product_id, &caller),
				Error::<T>::NotProductAdmin
			);

			Products::<T>::try_mutate(product_id, |maybe_product| -> DispatchResult {
				let product = maybe_product.as_mut().ok_or(Error::<T>::ProductNotFound)?;

				match action {
					CrudAction::Add => {
						ensure!(
							!Vaults::<T>::contains_key(&vault),
							Error::<T>::VaultAlreadyRegistered
						);
						let idx = priority as usize;
						ensure!(idx <= product.tranches.len(), Error::<T>::InvalidPriority);
						product
							.tranches
							.try_insert(
								idx,
								Tranche {
									tranche_type: tranche_type.clone(),
									vault: vault.clone(),
								},
							)
							.map_err(|_| Error::<T>::TooManyTranches)?;
						Vaults::<T>::insert(&vault, product_id);
					},
					CrudAction::Remove => {
						let idx = product
							.tranches
							.iter()
							.position(|t| t.vault == vault)
							.ok_or(Error::<T>::VaultNotFound)?;
						product.tranches.remove(idx);
						Vaults::<T>::remove(&vault);
					},
					CrudAction::Update => {
						let idx = product
							.tranches
							.iter()
							.position(|t| t.vault == vault)
							.ok_or(Error::<T>::VaultNotFound)?;
						let existing = product.tranches.remove(idx);
						let new_idx = priority as usize;
						ensure!(new_idx <= product.tranches.len(), Error::<T>::InvalidPriority);
						product
							.tranches
							.try_insert(
								new_idx,
								Tranche {
									tranche_type: existing.tranche_type,
									vault: vault.clone(),
								},
							)
							.map_err(|_| Error::<T>::TooManyTranches)?;
					},
				}
				Ok(())
			})?;

			Self::deposit_event(Event::TrancheSet {
				product_id,
				action,
				vault,
				tranche_type,
				priority,
			});
			Ok(())
		}

		/// Replace, atomically, the entire set of individual Adapters nested
		/// under one MultichainAdapter (identified by `parent_adapter_address`,
		/// `parent_chain_id`). Caller must hold the ProductAdmin role for
		/// `product_id`, checked inline against `T::Permissions`.
		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::set_adapters())]
		pub fn set_adapters(
			origin: OriginFor<T>,
			product_id: ProductId,
			parent_adapter_address: H160,
			parent_chain_id: u64,
			adapters: BoundedBTreeMap<
				H160,
				AdapterInfo<T::AccountId>,
				ConstU32<MAX_ADAPTERS_PER_MULTICHAIN_ADAPTER>,
			>,
		) -> DispatchResult {
			let caller = ensure_signed(origin)?;
			ensure!(
				T::Permissions::is_product_admin(product_id, &caller),
				Error::<T>::NotProductAdmin
			);

			Self::ensure_weights_sum_to_10000(adapters.values().map(|a| a.weight_bps))?;

			Products::<T>::try_mutate(product_id, |maybe_product| -> DispatchResult {
				let product = maybe_product.as_mut().ok_or(Error::<T>::ProductNotFound)?;
				let parent_key =
					AdapterKey { address: parent_adapter_address, chain_id: parent_chain_id };
				let parent = product
					.multichain_adapters
					.get_mut(&parent_key)
					.ok_or(Error::<T>::MultichainAdapterNotFound)?;

				// Deep replace: drop this parent's current reverse-index entries
				// first, so re-registering the same address under the same
				// parent (e.g. just to change its weightBps) isn't mistaken for
				// a collision below.
				for old_address in parent.adapters.keys() {
					AdapterIndex::<T>::remove(&AdapterKey {
						address: *old_address,
						chain_id: parent_chain_id,
					});
				}
				for address in adapters.keys() {
					let new_key = AdapterKey { address: *address, chain_id: parent_chain_id };
					ensure!(
						!AdapterIndex::<T>::contains_key(&new_key),
						Error::<T>::AdapterAlreadyRegistered
					);
				}
				for address in adapters.keys() {
					let new_key = AdapterKey { address: *address, chain_id: parent_chain_id };
					AdapterIndex::<T>::insert(&new_key, product_id);
				}

				parent.adapters = adapters;
				Ok(())
			})?;

			Self::deposit_event(Event::AdaptersSet {
				product_id,
				parent_adapter_address,
				parent_chain_id,
			});
			Ok(())
		}

		/// Replace a product's entire MultichainAdapter routing table
		/// atomically — deep replace, including every entry's nested
		/// `adapters`. Caller must hold the ProductAdmin role for
		/// `product_id`, checked inline against `T::Permissions`.
		#[pallet::call_index(3)]
		#[pallet::weight(<T as Config>::WeightInfo::set_multichain_adapters())]
		pub fn set_multichain_adapters(
			origin: OriginFor<T>,
			product_id: ProductId,
			multichain_adapters: BoundedBTreeMap<
				AdapterKey,
				MultichainAdapterInfo<T::AccountId>,
				ConstU32<MAX_MULTICHAIN_ADAPTERS>,
			>,
		) -> DispatchResult {
			let caller = ensure_signed(origin)?;
			ensure!(
				T::Permissions::is_product_admin(product_id, &caller),
				Error::<T>::NotProductAdmin
			);

			Self::ensure_weights_sum_to_10000(
				multichain_adapters.values().map(|info| info.weight_bps),
			)?;
			for info in multichain_adapters.values() {
				Self::ensure_weights_sum_to_10000(info.adapters.values().map(|a| a.weight_bps))?;
			}

			Products::<T>::try_mutate(product_id, |maybe_product| -> DispatchResult {
				let product = maybe_product.as_mut().ok_or(Error::<T>::ProductNotFound)?;

				// Deep replace: drop every old entry's reverse-index rows first
				// (top-level and nested), same reasoning as `set_adapters`.
				for (old_key, old_info) in product.multichain_adapters.iter() {
					MultichainAdapterIndex::<T>::remove(old_key);
					for old_address in old_info.adapters.keys() {
						AdapterIndex::<T>::remove(&AdapterKey {
							address: *old_address,
							chain_id: old_key.chain_id,
						});
					}
				}

				Self::ensure_multichain_adapters_are_unregistered(multichain_adapters.iter())?;
				Self::insert_multichain_adapter_index(product_id, multichain_adapters.iter());

				product.multichain_adapters = multichain_adapters;
				Ok(())
			})?;

			Self::deposit_event(Event::MultichainAdaptersSet { product_id });
			Ok(())
		}
	}
}

impl<T: pallet::Config> VaultInspect for pallet::Pallet<T> {
	fn vault_belongs_to_product(product_id: ProductId, vault: &VaultId) -> bool {
		pallet::Vaults::<T>::get(vault) == Some(product_id)
	}
}
