mod impls;

use crate::{
	migrations, AdapterInfo, AdapterKey, ChainTranches, CrudAction, MultichainAdapterInfo,
	MultichainProductDetails, ProductDetails, ProductId, SettlementMode, SingleChainProductDetails,
	SingleChainValuationInfo, Tranche, TrancheInput, TrancheType, ValuationInfo, VaultId,
	WeightInfo, MAX_ADAPTERS_PER_MULTICHAIN_ADAPTER, MAX_ADAPTERS_PER_SINGLE_CHAIN_PRODUCT,
	MAX_MULTICHAIN_ADAPTERS, MAX_TRANCHES, MAX_TRANCHE_CHAINS, MAX_TRANCHE_INPUTS,
	MAX_TRANCHE_MANAGERS,
};

use frame_support::{
	pallet_prelude::*,
	traits::{OnRuntimeUpgrade, StorageVersion},
};
use frame_system::pallet_prelude::*;
use sp_core::H160;
use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

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
	pub enum Origin<T: Config> {
		/// Dispatched by the tranche-system precompile after verifying the
		/// caller holds ProductAdmin for the product being acted on (via
		/// pallet-tranche-permissions). Carries that verified account so
		/// `create_product` can populate `ProductCreated`'s `product_admin`
		/// field without re-deriving it — mirrors
		/// `frame_system::RawOrigin::Signed`. The only accepted origin for
		/// every extrinsic in this pallet, ensuring none of them can be
		/// called except through the precompile.
		ProductAdmin(T::AccountId),
	}

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_timestamp::Config<Moment = u64> {
		/// Only accepted origin for every extrinsic in this pallet
		/// (`create_product`, `set_tranche`, `set_adapters`,
		/// `set_multichain_adapters`) — none of them can be called via a plain
		/// signed extrinsic. The tranche-system precompile constructs this
		/// origin itself, after verifying the caller holds ProductAdmin for
		/// the product being acted on (via pallet-tranche-permissions), so
		/// this pallet never needs to re-check that itself.
		/// Wire as `pallet_tranche_system::EnsureProductAdmin<Runtime>` in the
		/// runtime so that only the tranche-system precompile can invoke
		/// these extrinsics.
		type ProductAdminOrigin: frame_support::traits::EnsureOrigin<
			Self::RuntimeOrigin,
			Success = Self::AccountId,
		>;
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
		/// A single chain within a product cannot hold more than `MAX_TRANCHES`
		/// tranches, and a product cannot span more than `MAX_TRANCHE_CHAINS`
		/// distinct chains with tranches on them.
		TooManyTranches,
		/// The vault (chain_id, vault_address) is already registered — either
		/// to this product or a different one.
		VaultAlreadyRegistered,
		/// No tranche with the given vault exists for this product.
		VaultNotFound,
		/// `priority` is beyond the current number of tranches on that
		/// tranche's own chain — can only insert at an existing slot or
		/// immediately after the last one within that chain's own ordering
		/// (priority is scoped per chain, not product-wide — see
		/// `TrancheInput`'s doc comment).
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
		/// Two entries of `create_product`'s `tranches` input, on the *same*
		/// chain (`vault.chain_id`), shared the same `priority` — sort order
		/// would be ambiguous within that chain's own ordering. Entries on
		/// different chains sharing a `priority` is fine — see `TrancheInput`'s
		/// doc comment.
		DuplicatePriority,
		/// Within one chain's own priority order (0 = highest), every `Senior`
		/// tranche must precede every `Junior` tranche on that same chain —
		/// this invariant is per-chain, not product-wide.
		SeniorMustPrecedeJunior,
		/// A chain's own tranche list can hold at most one `Junior` tranche —
		/// the residual/variable-yield slot is singular per chain (multiple
		/// `Senior` tranches on the same chain are fine; there's only ever one
		/// residual claimant). Per-chain, not product-wide — a `Multichain`
		/// product can still have one Junior per chain across several chains.
		TooManyJuniorTranches,
		/// A chain's own tranche list, if non-empty, must contain at least one
		/// `Senior` tranche — a chain with only a `Junior` (nothing for it to
		/// receive the *residual* of) isn't a valid waterfall. Vacuously
		/// satisfied for a chain with no tranches at all (that chain simply
		/// has no entry — see `MultichainProductDetails::tranches`' doc
		/// comment on empty chain groups never being left around).
		AtLeastOneSeniorTrancheRequired,
		/// `set_tranche`'s `Remove` cannot take a product's very last tranche
		/// (summed across every chain for a `Multichain` product, or the
		/// product's one flat list for a `SingleChain` one) — a product must
		/// always retain at least one tranche somewhere. Emptying one
		/// specific chain entirely is fine (see `AtLeastOneSeniorTrancheRequired`'s
		/// doc comment) as long as at least one *other* chain still has a
		/// tranche; only removing the product's absolute last one reverts.
		ProductMustHaveAtLeastOneTranche,
		/// `set_tranche`'s `Update` cannot change a tranche's Junior/Senior
		/// discriminant (only `apr` and `priority` are mutable) — remove and
		/// re-add to change it.
		TrancheTypeImmutable,
		/// `settlement_offset_secs` must be strictly less than
		/// `settlement_length_secs` — otherwise the settlement window would
		/// swallow the whole cycle (or more), leaving no room for order
		/// submission.
		SettlementOffsetMustBeShorterThanLength,
		/// `settlement_start_timestamp` must be strictly after the current
		/// block time.
		SettlementStartMustBeInFuture,
		/// This extrinsic only applies to one `ProductDetails` variant
		/// (`Multichain` or `SingleChain`) — `product_id` refers to a product
		/// of the other kind.
		WrongProductType,
		/// `create_single_chain_product`'s `tranches` each carry their own
		/// `vault.chain_id`, but a single-chain product has exactly one chain
		/// — every tranche's `vault.chain_id` must equal the product's
		/// declared `chain_id`.
		SingleChainTranchesMustShareChain,
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
			base_asset: H160,
			valuation_address: H160,
			settlement_start_timestamp: u64,
			settlement_length_secs: u64,
			settlement_offset_secs: u64,
		},
		/// A new single-chain product was created (see
		/// `SingleChainProductDetails`). `is_sync` discriminates
		/// `settlement_mode`: when `true`, `settlement_start_timestamp`/
		/// `settlement_length_secs`/`settlement_offset_secs` are all `0` and
		/// not meaningful (there is no settlement cycle) — when `false`, they
		/// carry the same semantics as `ProductCreated`'s fields of the same
		/// name.
		SingleChainProductCreated {
			product_id: ProductId,
			product_admin: T::AccountId,
			chain_id: u64,
			base_asset: H160,
			valuation_address: H160,
			tranche_manager: H160,
			ledger: H160,
			is_sync: bool,
			settlement_start_timestamp: u64,
			settlement_length_secs: u64,
			settlement_offset_secs: u64,
		},
		/// A tranche was added, removed, or updated.
		TrancheSet {
			product_id: ProductId,
			action: CrudAction,
			vault: VaultId,
			tranche_type: TrancheType,
			asset: H160,
			shares: H160,
			/// Position within `vault.chain_id`'s own ordering, NOT
			/// product-wide — see `TrancheInput`'s doc comment.
			priority: u8,
		},
		/// A MultichainAdapter's nested adapters were replaced wholesale.
		AdaptersSet { product_id: ProductId, parent_adapter_address: H160, parent_chain_id: u64 },
		/// A product's entire MultichainAdapter table was replaced wholesale.
		MultichainAdaptersSet { product_id: ProductId },
		/// A product's entire per-chain TrancheManager table was replaced
		/// wholesale.
		MultichainTrancheManagersSet { product_id: ProductId },
		/// The global Orchestrator contract address was set.
		OrchestratorAddressSet { address: H160 },
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
	pub type AdapterIndex<T: Config> = StorageMap<_, Blake2_128Concat, AdapterKey, ProductId>;

	#[pallet::storage]
	/// Reverse index: which product a MultichainAdapter (adapter_address, chain_id)
	/// belongs to. A separate namespace from `AdapterIndex` even though the key
	/// shape is identical — globally unique across all products, same rationale.
	pub type MultichainAdapterIndex<T: Config> =
		StorageMap<_, Blake2_128Concat, AdapterKey, ProductId>;

	#[pallet::storage]
	/// The single, global Hub-chain Orchestrator contract address — one per
	/// chain, not per product. The tranche-permissions precompile calls
	/// `Orchestrator.sendWhitelist(...)` here to propagate a `TrancheInvestor`
	/// grant/revoke to the Spoke chain a vault lives on. Defaults to the zero
	/// address (propagation reverts until sudo sets it). Only writable by root.
	pub type OrchestratorAddress<T: Config> = StorageValue<_, H160, ValueQuery>;

	// -----------------------------------------------------------------------
	// Hooks
	// -----------------------------------------------------------------------

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_runtime_upgrade() -> Weight {
			// Chained rather than just `MigrateToV3` alone: each `VersionedMigration`
			// self-gates on its own exact on-chain version, so this is safe regardless of
			// whether a given chain is still at v0 (runs all three, back to back, in the
			// same upgrade), already at v1 (skips straight to v2 then v3 — the live
			// testbed case, see `migrations::v2`'s doc comment for why v1 alone didn't
			// get every chain to v2 on its own), or already at v2 (skips straight to v3).
			migrations::v1::MigrateToV1::<T>::on_runtime_upgrade()
				.saturating_add(migrations::v2::MigrateToV2::<T>::on_runtime_upgrade())
				.saturating_add(migrations::v3::MigrateToV3::<T>::on_runtime_upgrade())
		}
	}

	// -----------------------------------------------------------------------
	// Extrinsics
	// -----------------------------------------------------------------------

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Create a new tranche-system product: its Valuation contract binding,
		/// its tranches, its MultichainAdapter routing table (each entry
		/// carrying its own nested individual-Adapter registrations), and its
		/// per-chain TrancheManager bindings.
		///
		/// Origin must be `ProductAdminOrigin` — the tranche-system precompile
		/// constructs it after verifying the caller holds ProductAdmin for
		/// `product_id` (granted up front via pallet-tranche-permissions,
		/// before this is ever called).
		///
		/// `tranches` carries an explicit `priority` per entry (see
		/// `TrancheInput`'s doc comment) — grouped by `vault.chain_id` here,
		/// then each chain's own group sorted independently to establish
		/// `MultichainProductDetails::tranches[chain_id]`'s final order.
		/// Reverts if two entries on the *same* chain share a `priority`, if
		/// sorting a chain's own group by `priority` doesn't put every
		/// `Senior` tranche before every `Junior` one on that chain, if any
		/// chain's own group holds more than one `Junior` tranche, or if any
		/// chain's own group holds zero `Senior` tranches — all four checks
		/// are per-chain, not product-wide (see `Error::TooManyJuniorTranches`/
		/// `Error::AtLeastOneSeniorTrancheRequired`).
		///
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::create_product())]
		pub fn create_product(
			origin: OriginFor<T>,
			product_id: ProductId,
			valuation: ValuationInfo,
			tranches: BoundedVec<TrancheInput, ConstU32<MAX_TRANCHE_INPUTS>>,
			multichain_adapters: BoundedBTreeMap<
				AdapterKey,
				MultichainAdapterInfo<T::AccountId>,
				ConstU32<MAX_MULTICHAIN_ADAPTERS>,
			>,
			multichain_tranche_managers: BoundedBTreeMap<u64, H160, ConstU32<MAX_TRANCHE_MANAGERS>>,
		) -> DispatchResult {
			let product_admin = T::ProductAdminOrigin::ensure_origin(origin)?;

			ensure!(!Products::<T>::contains_key(product_id), Error::<T>::ProductAlreadyExists);
			ensure!(!tranches.is_empty(), Error::<T>::EmptyTranches);
			ensure!(
				valuation.settlement_offset_secs < valuation.settlement_length_secs,
				Error::<T>::SettlementOffsetMustBeShorterThanLength
			);
			let now_secs = <pallet_timestamp::Pallet<T>>::get() / 1000;
			ensure!(
				valuation.settlement_start_timestamp > now_secs,
				Error::<T>::SettlementStartMustBeInFuture
			);

			Self::ensure_weights_sum_to_10000(
				multichain_adapters.values().map(|info| info.weight_bps),
			)?;
			for info in multichain_adapters.values() {
				Self::ensure_weights_sum_to_10000(info.adapters.values().map(|a| a.weight_bps))?;
			}

			// Group by chain first — priority/duplicate/Senior-before-Junior are all
			// checked per chain, not across the whole product (see `TrancheInput`'s
			// doc comment for why cross-chain tranche ordering isn't meaningful).
			let mut by_chain: BTreeMap<u64, Vec<TrancheInput>> = BTreeMap::new();
			for input in tranches.into_inner() {
				by_chain.entry(input.vault.chain_id).or_default().push(input);
			}
			let mut tranches: BoundedBTreeMap<u64, ChainTranches, ConstU32<MAX_TRANCHE_CHAINS>> =
				BoundedBTreeMap::new();
			for (chain_id, mut chain_inputs) in by_chain {
				chain_inputs.sort_by_key(|input| input.priority);
				for pair in chain_inputs.windows(2) {
					ensure!(pair[0].priority != pair[1].priority, Error::<T>::DuplicatePriority);
				}
				let ordered: Vec<Tranche> = chain_inputs
					.into_iter()
					.map(|input| Tranche {
						tranche_type: input.tranche_type,
						vault: input.vault,
						asset: input.asset,
						shares: input.shares,
					})
					.collect();
				Self::ensure_senior_precedes_junior(&ordered)?;
				Self::ensure_valid_tranche_composition(&ordered)?;
				let chain_tranches: ChainTranches =
					BoundedVec::try_from(ordered).map_err(|_| Error::<T>::TooManyTranches)?;
				tranches
					.try_insert(chain_id, chain_tranches)
					.map_err(|_| Error::<T>::TooManyTranches)?;
			}

			Self::ensure_tranches_are_unregistered(
				tranches.values().flat_map(|chain| chain.iter()),
			)?;
			Self::ensure_multichain_adapters_are_unregistered(multichain_adapters.iter())?;

			for tranche in tranches.values().flat_map(|chain| chain.iter()) {
				Vaults::<T>::insert(&tranche.vault, product_id);
			}
			Self::insert_multichain_adapter_index(product_id, multichain_adapters.iter());

			Self::deposit_event(Event::ProductCreated {
				product_id,
				product_admin,
				base_asset: valuation.base_asset,
				valuation_address: valuation.valuation_address,
				settlement_start_timestamp: valuation.settlement_start_timestamp,
				settlement_length_secs: valuation.settlement_length_secs,
				settlement_offset_secs: valuation.settlement_offset_secs,
			});

			Products::<T>::insert(
				product_id,
				ProductDetails::Multichain(MultichainProductDetails {
					valuation,
					tranches,
					multichain_adapters,
					multichain_tranche_managers,
				}),
			);

			Ok(())
		}

		/// Create a new single-chain product: every contract (Vault(s),
		/// TrancheManager, Valuation, Adapters, Ledger) lives on one EVM
		/// chain (`chain_id`, not necessarily the Hub) — see
		/// `SingleChainProductDetails`'s doc comment for the model this
		/// differs from `create_product`'s hub-spoke one in.
		///
		/// Origin must be `ProductAdminOrigin` — same precompile-only gating
		/// as `create_product`.
		///
		/// `tranches` uses the same `priority`-sort-and-validate rules
		/// `create_product` applies within one chain's own group (sort by
		/// `priority`, reject duplicates, require every `Senior` before every
		/// `Junior`, at most one `Junior`, at least one `Senior`) — applied
		/// directly to the whole flat input here, without `create_product`'s
		/// chain-grouping step, since every entry's `vault.chain_id` must
		/// equal `chain_id` anyway (reverts with
		/// `SingleChainTranchesMustShareChain` otherwise) — there's only ever
		/// the one chain to group by. `adapters`' `weight_bps` must sum to
		/// exactly 10_000, same invariant as one `MultichainAdapterInfo`'s
		/// nested `adapters`.
		#[pallet::call_index(6)]
		#[pallet::weight(<T as Config>::WeightInfo::create_single_chain_product())]
		pub fn create_single_chain_product(
			origin: OriginFor<T>,
			product_id: ProductId,
			chain_id: u64,
			valuation: SingleChainValuationInfo,
			tranches: BoundedVec<TrancheInput, ConstU32<MAX_TRANCHES>>,
			tranche_manager: H160,
			adapters: BoundedBTreeMap<
				H160,
				AdapterInfo<T::AccountId>,
				ConstU32<MAX_ADAPTERS_PER_SINGLE_CHAIN_PRODUCT>,
			>,
			ledger: H160,
		) -> DispatchResult {
			let product_admin = T::ProductAdminOrigin::ensure_origin(origin)?;

			ensure!(!Products::<T>::contains_key(product_id), Error::<T>::ProductAlreadyExists);
			ensure!(!tranches.is_empty(), Error::<T>::EmptyTranches);
			if let SettlementMode::Async {
				settlement_start_timestamp,
				settlement_length_secs,
				settlement_offset_secs,
			} = &valuation.settlement_mode
			{
				ensure!(
					*settlement_offset_secs < *settlement_length_secs,
					Error::<T>::SettlementOffsetMustBeShorterThanLength
				);
				let now_secs = <pallet_timestamp::Pallet<T>>::get() / 1000;
				ensure!(
					*settlement_start_timestamp > now_secs,
					Error::<T>::SettlementStartMustBeInFuture
				);
			}

			Self::ensure_weights_sum_to_10000(adapters.values().map(|a| a.weight_bps))?;

			let mut sorted: Vec<TrancheInput> = tranches.into_inner();
			sorted.sort_by_key(|input| input.priority);
			for pair in sorted.windows(2) {
				ensure!(pair[0].priority != pair[1].priority, Error::<T>::DuplicatePriority);
			}
			for input in sorted.iter() {
				ensure!(
					input.vault.chain_id == chain_id,
					Error::<T>::SingleChainTranchesMustShareChain
				);
			}
			let ordered: Vec<Tranche> = sorted
				.into_iter()
				.map(|input| Tranche {
					tranche_type: input.tranche_type,
					vault: input.vault,
					asset: input.asset,
					shares: input.shares,
				})
				.collect();
			Self::ensure_senior_precedes_junior(&ordered)?;
			Self::ensure_valid_tranche_composition(&ordered)?;
			let tranches: BoundedVec<Tranche, ConstU32<MAX_TRANCHES>> =
				BoundedVec::try_from(ordered).map_err(|_| Error::<T>::TooManyTranches)?;

			Self::ensure_tranches_are_unregistered(tranches.iter())?;
			Self::ensure_single_chain_adapters_are_unregistered(chain_id, adapters.keys())?;

			for tranche in tranches.iter() {
				Vaults::<T>::insert(&tranche.vault, product_id);
			}
			for address in adapters.keys() {
				AdapterIndex::<T>::insert(&AdapterKey { address: *address, chain_id }, product_id);
			}

			let (
				is_sync,
				settlement_start_timestamp,
				settlement_length_secs,
				settlement_offset_secs,
			) = match &valuation.settlement_mode {
				SettlementMode::Sync => (true, 0, 0, 0),
				SettlementMode::Async {
					settlement_start_timestamp,
					settlement_length_secs,
					settlement_offset_secs,
				} => (
					false,
					*settlement_start_timestamp,
					*settlement_length_secs,
					*settlement_offset_secs,
				),
			};

			Self::deposit_event(Event::SingleChainProductCreated {
				product_id,
				product_admin,
				chain_id,
				base_asset: valuation.base_asset,
				valuation_address: valuation.valuation_address,
				tranche_manager,
				ledger,
				is_sync,
				settlement_start_timestamp,
				settlement_length_secs,
				settlement_offset_secs,
			});

			Products::<T>::insert(
				product_id,
				ProductDetails::SingleChain(SingleChainProductDetails {
					valuation,
					chain_id,
					tranches,
					tranche_manager,
					adapters,
					ledger,
				}),
			);

			Ok(())
		}

		/// Add, remove, or update a tranche on an existing product — Multichain or
		/// single-chain alike, identified by its vault. Origin must be
		/// `ProductAdminOrigin` — same precompile-only gating as `create_product`.
		///
		/// Field usage differs by `action`, mirroring interface.sol's
		/// `set_tranche`. Every branch re-validates, on the resulting list for
		/// `vault.chain_id`'s own chain (Multichain — a `Remove`/`Update` on a
		/// chain with no existing tranches reverts with `VaultNotFound`
		/// before even reaching this; SingleChain — the product's one and
		/// only list): that all `Senior` tranches on that chain still precede
		/// all `Junior` ones on that same chain; that chain's own list still
		/// holds at most one `Junior`; and, if that chain's own list is still
		/// non-empty after the mutation, that it holds at least one `Senior`
		/// — reverts if the requested change would break any of these. All
		/// per-chain checks, not product-wide (see `TrancheInput`'s doc
		/// comment). Emptying a chain's list entirely (its last tranche
		/// removed) is fine — that chain's own entry is then dropped, not
		/// left around violating the "at least one Senior" rule vacuously
		/// (e.g. retiring a Hub-deployed vault while keeping Spoke ones, or
		/// vice versa). One additional check IS product-wide, checked once
		/// here rather than per-chain: `Remove` reverts with
		/// `Error::ProductMustHaveAtLeastOneTranche` if it would take the
		/// product's absolute last tranche, summed across every chain — a
		/// product must always retain at least one tranche *somewhere*, even
		/// though any single chain may be emptied out entirely. For a
		/// single-chain product, `Add`/`Update` additionally revert unless
		/// `vault.chain_id` equals the product's own `chain_id` (same
		/// constraint `create_single_chain_product` enforces at creation
		/// time).
		/// - `Add`: `vault` becomes the new tranche's identity (reverts if already registered to
		///   any product). `tranche_type`, `asset`, `shares`, and `priority` are used. If
		///   `priority` is already occupied within `vault.chain_id`'s own list, the existing
		///   tranche at that slot (and everything after it, on that same chain) shifts down by
		///   one. For a Multichain product, `vault.chain_id` need not already have any tranches
		///   — a fresh per-chain entry is created on first use.
		/// - `Remove`: only `vault` is used, to find which tranche to remove — searched within
		///   `vault.chain_id`'s own list. Every tranche after it, on that same chain, shifts up
		///   by one, closing the gap. For a Multichain product, removing a chain's last tranche
		///   drops that chain's entry entirely (never left around empty). NOT YET CHECKED
		///   (deferred): interface.sol also specifies this should revert if the tranche has
		///   outstanding investments — pallet-tranche-investments doesn't expose an inspection
		///   trait for this yet.
		/// - `Update`: `vault` identifies which tranche to update (reverts if not found, searched
		///   within `vault.chain_id`'s own list); `asset`, `shares`, and `priority` are applied
		///   as new values using the same insert-and-shift semantics as `Add` for `priority`
		///   (still scoped to that same chain — `Update` never moves a tranche to a different
		///   chain, only `Remove` then `Add` can). `tranche_type`'s Junior/Senior discriminant is
		///   immutable — reverts if it doesn't match the existing tranche's; `apr` (carried
		///   inside `tranche_type` for `Senior`) may still change, since only the discriminant is
		///   checked.
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::set_tranche())]
		pub fn set_tranche(
			origin: OriginFor<T>,
			product_id: ProductId,
			action: CrudAction,
			vault: VaultId,
			tranche_type: TrancheType,
			asset: H160,
			shares: H160,
			priority: u8,
		) -> DispatchResult {
			T::ProductAdminOrigin::ensure_origin(origin)?;

			Products::<T>::try_mutate(product_id, |maybe_product| -> DispatchResult {
				let product = maybe_product.as_mut().ok_or(Error::<T>::ProductNotFound)?;

				match product {
					ProductDetails::Multichain(product) => {
						let chain_id = vault.chain_id;
						if action == CrudAction::Add && !product.tranches.contains_key(&chain_id) {
							product
								.tranches
								.try_insert(chain_id, ChainTranches::default())
								.map_err(|_| Error::<T>::TooManyTranches)?;
						}
						let chain_tranches =
							product.tranches.get_mut(&chain_id).ok_or(Error::<T>::VaultNotFound)?;
						Self::apply_tranche_action(
							product_id,
							chain_tranches,
							action,
							&vault,
							&tranche_type,
							asset,
							shares,
							priority,
						)?;
						if chain_tranches.is_empty() {
							product.tranches.remove(&chain_id);
						}
					},
					ProductDetails::SingleChain(product) => {
						// A single-chain product has exactly one chain — every
						// tranche's vault must live on it, same constraint
						// `create_single_chain_product` enforces at creation time.
						if matches!(action, CrudAction::Add | CrudAction::Update) {
							ensure!(
								vault.chain_id == product.chain_id,
								Error::<T>::SingleChainTranchesMustShareChain
							);
						}
						Self::apply_tranche_action(
							product_id,
							&mut product.tranches,
							action,
							&vault,
							&tranche_type,
							asset,
							shares,
							priority,
						)?;
					},
				}

				// `apply_tranche_action`/`ensure_valid_tranche_composition` only ever
				// check one chain's own list — emptying a chain entirely (Remove) is
				// allowed there (e.g. retiring a Hub-deployed vault while keeping
				// Spoke ones). What's checked here, once, at the product level, is
				// the floor beneath that: a product must always retain at least one
				// tranche *somewhere*. Only reachable via `Remove` — `Add` only ever
				// grows the total, `Update` never changes it.
				if action == CrudAction::Remove {
					let total_tranches: usize = match &*product {
						ProductDetails::Multichain(product) => {
							product.tranches.values().map(|chain| chain.len()).sum()
						},
						ProductDetails::SingleChain(product) => product.tranches.len(),
					};
					ensure!(total_tranches > 0, Error::<T>::ProductMustHaveAtLeastOneTranche);
				}

				Ok(())
			})?;

			Self::deposit_event(Event::TrancheSet {
				product_id,
				action,
				vault,
				tranche_type,
				asset,
				shares,
				priority,
			});
			Ok(())
		}

		/// Replace, atomically, a product's entire flat individual-Adapter set —
		/// Multichain or single-chain alike. Origin must be `ProductAdminOrigin`
		/// — same precompile-only gating as `create_product`.
		///
		/// For a Multichain product, this replaces one MultichainAdapter's
		/// nested `adapters` (identified by `parent_adapter_address`,
		/// `parent_chain_id` — reverts with `MultichainAdapterNotFound` if no
		/// such parent exists). For a single-chain product, `parent_adapter_address`/
		/// `parent_chain_id` are ignored (there's no MultichainAdapter parent
		/// concept at all) — this replaces the product's whole flat `adapters`
		/// map instead.
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
			T::ProductAdminOrigin::ensure_origin(origin)?;

			Self::ensure_weights_sum_to_10000(adapters.values().map(|a| a.weight_bps))?;

			Products::<T>::try_mutate(product_id, |maybe_product| -> DispatchResult {
				let product = maybe_product.as_mut().ok_or(Error::<T>::ProductNotFound)?;
				match product {
					ProductDetails::Multichain(product) => {
						let parent_key = AdapterKey {
							address: parent_adapter_address,
							chain_id: parent_chain_id,
						};
						let parent = product
							.multichain_adapters
							.get_mut(&parent_key)
							.ok_or(Error::<T>::MultichainAdapterNotFound)?;
						Self::replace_adapter_index(
							product_id,
							parent_chain_id,
							parent.adapters.keys(),
							adapters.keys(),
						)?;
						parent.adapters = adapters;
					},
					ProductDetails::SingleChain(product) => {
						// `parent_adapter_address`/`parent_chain_id` are ignored here —
						// a single-chain product has no MultichainAdapter parent
						// concept at all, so this replaces the product's whole flat
						// `adapters` map instead of one parent's nested set.
						Self::replace_adapter_index(
							product_id,
							product.chain_id,
							product.adapters.keys(),
							adapters.keys(),
						)?;
						product.adapters = adapters;
					},
				}
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
		/// `adapters`. Origin must be `ProductAdminOrigin` — same
		/// precompile-only gating as `create_product`.
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
			T::ProductAdminOrigin::ensure_origin(origin)?;

			Self::ensure_weights_sum_to_10000(
				multichain_adapters.values().map(|info| info.weight_bps),
			)?;
			for info in multichain_adapters.values() {
				Self::ensure_weights_sum_to_10000(info.adapters.values().map(|a| a.weight_bps))?;
			}

			Products::<T>::try_mutate(product_id, |maybe_product| -> DispatchResult {
				let product = maybe_product.as_mut().ok_or(Error::<T>::ProductNotFound)?;
				let product = match product {
					ProductDetails::Multichain(product) => product,
					ProductDetails::SingleChain(_) => {
						return Err(Error::<T>::WrongProductType.into())
					},
				};

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

		/// Set the single, global Orchestrator contract address. Root-only —
		/// see `OrchestratorAddress`'s doc comment.
		#[pallet::call_index(4)]
		#[pallet::weight(<T as Config>::WeightInfo::set_orchestrator_address())]
		pub fn set_orchestrator_address(origin: OriginFor<T>, address: H160) -> DispatchResult {
			ensure_root(origin)?;
			OrchestratorAddress::<T>::put(address);
			Self::deposit_event(Event::OrchestratorAddressSet { address });
			Ok(())
		}

		/// Replace a product's entire per-chain TrancheManager table
		/// atomically (Hub included, if the product has a Hub vault — see
		/// `ProductDetails::multichain_tranche_managers`'s doc comment).
		/// Origin must be `ProductAdminOrigin` — same precompile-only gating
		/// as `create_product`.
		#[pallet::call_index(5)]
		#[pallet::weight(<T as Config>::WeightInfo::set_multichain_tranche_managers())]
		pub fn set_multichain_tranche_managers(
			origin: OriginFor<T>,
			product_id: ProductId,
			multichain_tranche_managers: BoundedBTreeMap<u64, H160, ConstU32<MAX_TRANCHE_MANAGERS>>,
		) -> DispatchResult {
			T::ProductAdminOrigin::ensure_origin(origin)?;

			Products::<T>::try_mutate(product_id, |maybe_product| -> DispatchResult {
				let product = maybe_product.as_mut().ok_or(Error::<T>::ProductNotFound)?;
				let product = match product {
					ProductDetails::Multichain(product) => product,
					ProductDetails::SingleChain(_) => {
						return Err(Error::<T>::WrongProductType.into())
					},
				};
				product.multichain_tranche_managers = multichain_tranche_managers;
				Ok(())
			})?;

			Self::deposit_event(Event::MultichainTrancheManagersSet { product_id });
			Ok(())
		}
	}
}
