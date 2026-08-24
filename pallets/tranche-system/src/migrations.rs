use crate::{
	AdapterKey, ChainTranches, Config, MultichainAdapterInfo, MultichainProductDetails, Pallet,
	ProductDetails, ProductId, SingleChainProductDetails, Tranche, ValuationInfo,
	MAX_MULTICHAIN_ADAPTERS, MAX_TRANCHES_PER_CHAIN, MAX_TRANCHE_CHAINS, MAX_TRANCHE_MANAGERS,
};

use frame_support::{
	migrations::VersionedMigration, pallet_prelude::*, storage_alias,
	traits::UncheckedOnRuntimeUpgrade, weights::Weight, Blake2_128Concat,
};
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::{ConstU32, H160};
use sp_runtime::{BoundedBTreeMap, BoundedVec, RuntimeDebug};
use sp_std::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	marker::PhantomData,
	vec::Vec,
};

pub(crate) const LOG_TARGET: &str = "runtime::tranche-system";

// syntactic sugar for logging.
macro_rules! log {
	($level:tt, $patter:expr $(, $values:expr)* $(,)?) => {
		log::$level!(
			target: LOG_TARGET,
			$patter $(, $values)*
		)
	};
}

/// Groups a flat, product-wide-ordered tranche list (the shape every
/// pre-v3 `MultichainProductDetails::tranches` used) into chain-keyed groups,
/// preserving each chain's own relative order — used by `v1`/`v2`/`v3`'s
/// migrations alike to produce the current, chain-scoped `tranches` shape
/// from whatever flat shape they're each starting from. Needs no
/// re-validation of the Senior-before-Junior invariant per chain: a stable
/// partition of an already-valid flat order can only ever preserve each
/// chain's own relative order, never invert it, so if the flat order
/// satisfied "every Senior precedes every Junior" as a whole, each chain's
/// own subsequence trivially still does too.
fn group_tranches_by_chain(
	flat: BoundedVec<Tranche, ConstU32<MAX_TRANCHES_PER_CHAIN>>,
) -> BoundedBTreeMap<u64, ChainTranches, ConstU32<MAX_TRANCHE_CHAINS>> {
	let mut by_chain: BTreeMap<u64, Vec<Tranche>> = BTreeMap::new();
	for tranche in flat.into_inner() {
		by_chain.entry(tranche.vault.chain_id).or_default().push(tranche);
	}
	let mut grouped = BoundedBTreeMap::new();
	for (chain_id, tranches) in by_chain {
		// Bounded at `MAX_TRANCHES_PER_CHAIN` (10) per chain and `MAX_TRANCHE_CHAINS`
		// (10) distinct chains — a flat list already bounded at `MAX_TRANCHES_PER_CHAIN`
		// (10) total can never produce a per-chain group exceeding 10, nor
		// more than 10 distinct chains, so neither fallback below can ever
		// actually trigger; kept only so this stays a total function rather
		// than one that could panic a migration.
		let chain_tranches: ChainTranches = BoundedVec::try_from(tranches).unwrap_or_default();
		let _ = grouped.try_insert(chain_id, chain_tranches);
	}
	grouped
}

/// v0 -> v1: two shape changes land together here. **Correction (see `v2`'s doc comment):**
/// this was written on the assumption that neither change had shipped to a live chain yet —
/// wrong for the `multichain_tranche_managers` backfill half, which had already run on live
/// testbed under an earlier build of this same migration, before the `ProductDetails` enum
/// existed. Editing this migration in place to also add the enum-wrapping half therefore
/// silently never ran on that chain (its `StorageVersion` was already 1, past this gate) —
/// `v2` is the real fix for the enum-wrapping half. This doc comment is left describing both
/// halves together anyway, since that's still an accurate description of what a chain
/// starting genuinely fresh at v0 gets from this migration alone.
///
/// 1. What was flat `ProductDetails` gained a `multichain_tranche_managers: BoundedBTreeMap<u64,
///    H160, ConstU32<MAX_TRANCHE_MANAGERS>>` field — one TrancheManager contract address per
///    chain a product's vaults span, Hub included (see `MultichainProductDetails::
///    multichain_tranche_managers`'s doc comment).
///
///    For every product that already existed before this upgrade, the real TrancheManager
///    addresses aren't recoverable from on-chain state, so this migration backfills one entry
///    per distinct chain the product already has a tranche vault on — derived from `tranches`,
///    which IS already known — each pointed at the zero address as an explicit placeholder,
///    not a real binding. `set_multichain_tranche_managers` must be called afterward, per
///    product, to fill in the genuine addresses; until then, any flow reading this table sees
///    zero addresses rather than an empty (and therefore ambiguous — "not backfilled yet" vs.
///    "genuinely has no vaults") table.
///
/// 2. `ProductDetails` itself became an enum (`Multichain`/`SingleChain`, see its doc comment)
///    to make room for single-chain products — every existing product is, by construction, a
///    `Multichain` one, so this migration just wraps each entry rather than deriving anything.
pub mod v1 {
	use super::*;

	/// `ProductDetails` as it existed under `STORAGE_VERSION::new(0)`, before
	/// `multichain_tranche_managers` existed.
	#[derive(
		Clone,
		Encode,
		Decode,
		DecodeWithMemTracking,
		PartialEq,
		RuntimeDebug,
		TypeInfo,
		MaxEncodedLen,
	)]
	pub struct ProductDetailsV0<AccountId> {
		pub valuation: ValuationInfo,
		pub tranches: BoundedVec<Tranche, ConstU32<MAX_TRANCHES_PER_CHAIN>>,
		pub multichain_adapters: BoundedBTreeMap<
			AdapterKey,
			MultichainAdapterInfo<AccountId>,
			ConstU32<MAX_MULTICHAIN_ADAPTERS>,
		>,
	}

	#[storage_alias]
	type Products<T: Config> = StorageMap<
		Pallet<T>,
		Blake2_128Concat,
		ProductId,
		ProductDetailsV0<<T as frame_system::Config>::AccountId>,
	>;

	pub struct MigrateV0ToV1<T>(PhantomData<T>);

	impl<T: Config> UncheckedOnRuntimeUpgrade for MigrateV0ToV1<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();

			let products = Products::<T>::drain().collect::<Vec<_>>();
			// `drain()` removes each entry as it's iterated (1 read + 1 write per
			// entry), separate from the reinsert-writes charged below.
			weight = weight.saturating_add(
				T::DbWeight::get().reads_writes(products.len() as u64, products.len() as u64),
			);
			let products_count = products.len();
			for (product_id, old) in products {
				let mut chain_ids = BTreeSet::new();
				for tranche in old.tranches.iter() {
					chain_ids.insert(tranche.vault.chain_id);
				}
				let multichain_tranche_managers: BoundedBTreeMap<
					u64,
					H160,
					ConstU32<MAX_TRANCHE_MANAGERS>,
				> = BoundedBTreeMap::try_from(
					chain_ids
						.into_iter()
						.map(|chain_id| (chain_id, H160::zero()))
						.collect::<sp_std::collections::btree_map::BTreeMap<_, _>>(),
				)
				// `MAX_TRANCHES_PER_CHAIN` (10) <= `MAX_TRANCHE_MANAGERS` (20), so the set of
				// distinct tranche chain_ids can never overflow this bound — falls
				// back to an empty table in the unreachable case it somehow did,
				// rather than panicking a migration.
				.unwrap_or_default();

				crate::Products::<T>::insert(
					product_id,
					ProductDetails::Multichain(MultichainProductDetails {
						valuation: old.valuation,
						tranches: group_tranches_by_chain(old.tranches),
						multichain_adapters: old.multichain_adapters,
						multichain_tranche_managers,
					}),
				);
			}
			weight = weight.saturating_add(T::DbWeight::get().writes(products_count as u64));

			log!(
				info,
				"tranche-system v0->v1: backfilled multichain_tranche_managers (zero address per distinct tranche chain, must be replaced via set_multichain_tranche_managers) and wrapped in ProductDetails::Multichain for {} products ✅",
				products_count,
			);

			weight
		}
	}

	/// Gated `on_chain == 0 && in_code == 1`, and bumps the on-chain version itself —
	/// wire this (not `MigrateV0ToV1` directly) into the pallet's own hook.
	pub type MigrateToV1<T> = VersionedMigration<
		0,
		1,
		MigrateV0ToV1<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;
}

/// v1 -> v2: fixes a missed migration. `v1`'s doc comment above claimed both of its shape
/// changes "haven't been released to a live chain yet" — that was wrong for the
/// `multichain_tranche_managers` backfill half: an *earlier* build of this same v0->v1
/// migration (before the `ProductDetails` enum existed) had already shipped and run on
/// live testbed, bumping on-chain `StorageVersion` to 1. So when the enum-wrapping half was
/// added by editing that same migration in place, `VersionedMigration<0, 1, ...>`'s gate
/// (`on_chain == 0`) no longer matched on that already-migrated chain — the edit silently
/// never ran there, leaving `Products` storage in the pre-enum shape (flat struct, with
/// `multichain_tranche_managers`) while every reader now expects `ProductDetails::Multichain`,
/// which cannot decode it. This migration is the real fix: it picks up exactly where the
/// live chain actually is (`StorageVersion` 1, pre-enum shape) and does only the enum-wrapping
/// step `v1` was supposed to also cover.
///
/// Safe to apply regardless of which of the two `v1` behaviors a given chain actually saw
/// (this pallet's hook wires `MigrateToV1` before `MigrateToV2`, and each `VersionedMigration`
/// only fires on its own exact on-chain version) — a chain still at 0 runs `v1` (now already
/// producing the enum-wrapped shape directly) and then `v2` becomes a no-op (on-chain is
/// already 2); a chain at 1 (the live testbed case) skips `v1` and runs only `v2`.
pub mod v2 {
	use super::*;

	/// `MultichainProductDetails` as it existed under `STORAGE_VERSION::new(1)`
	/// — this pallet's transitional shape (`ProductDetails` not yet an enum,
	/// `tranches` still a single flat, product-wide `BoundedVec`). Originally
	/// this migration aliased the live `MultichainProductDetails` type
	/// directly here, reasoning that its fields were "byte for byte the same"
	/// as this v1 shape — true at the time, no longer true once `tranches`
	/// itself changed shape (see `v3`), so this is now its own named
	/// snapshot, same rationale as `v1`'s own `ProductDetailsV0`.
	#[derive(
		Clone,
		Encode,
		Decode,
		DecodeWithMemTracking,
		PartialEq,
		RuntimeDebug,
		TypeInfo,
		MaxEncodedLen,
	)]
	pub struct MultichainProductDetailsV1<AccountId> {
		pub valuation: ValuationInfo,
		pub tranches: BoundedVec<Tranche, ConstU32<MAX_TRANCHES_PER_CHAIN>>,
		pub multichain_adapters: BoundedBTreeMap<
			AdapterKey,
			MultichainAdapterInfo<AccountId>,
			ConstU32<MAX_MULTICHAIN_ADAPTERS>,
		>,
		pub multichain_tranche_managers: BoundedBTreeMap<u64, H160, ConstU32<MAX_TRANCHE_MANAGERS>>,
	}

	#[storage_alias]
	type Products<T: Config> = StorageMap<
		Pallet<T>,
		Blake2_128Concat,
		ProductId,
		MultichainProductDetailsV1<<T as frame_system::Config>::AccountId>,
	>;

	pub struct MigrateV1ToV2<T>(PhantomData<T>);

	impl<T: Config> UncheckedOnRuntimeUpgrade for MigrateV1ToV2<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();

			let products = Products::<T>::drain().collect::<Vec<_>>();
			// `drain()` removes each entry as it's iterated (1 read + 1 write per
			// entry), separate from the reinsert-writes charged below.
			weight = weight.saturating_add(
				T::DbWeight::get().reads_writes(products.len() as u64, products.len() as u64),
			);
			let products_count = products.len();
			for (product_id, old) in products {
				crate::Products::<T>::insert(
					product_id,
					ProductDetails::Multichain(MultichainProductDetails {
						valuation: old.valuation,
						tranches: group_tranches_by_chain(old.tranches),
						multichain_adapters: old.multichain_adapters,
						multichain_tranche_managers: old.multichain_tranche_managers,
					}),
				);
			}
			weight = weight.saturating_add(T::DbWeight::get().writes(products_count as u64));

			log!(
				info,
				"tranche-system v1->v2: wrapped {} existing products in ProductDetails::Multichain (recovering a migration missed when ProductDetails became an enum) ✅",
				products_count,
			);

			weight
		}
	}

	/// Gated `on_chain == 1 && in_code == 2`, and bumps the on-chain version itself —
	/// wire this (not `MigrateV1ToV2` directly) into the pallet's own hook.
	pub type MigrateToV2<T> = VersionedMigration<
		1,
		2,
		MigrateV1ToV2<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;
}

/// v2 -> v3: `MultichainProductDetails::tranches` changes shape from a single
/// flat, product-wide-ordered `BoundedVec<Tranche, ConstU32<MAX_TRANCHES_PER_CHAIN>>` to
/// a per-chain-keyed `BoundedBTreeMap<u64, ChainTranches, ConstU32<MAX_TRANCHE_CHAINS>>`
/// — waterfall priority ordering (and the Senior-before-Junior invariant) is
/// now enforced independently per chain rather than product-wide, since
/// cross-chain tranche comparison was never a meaningful ordering to begin
/// with (a Junior tranche on one chain and a Senior tranche on another don't
/// compete in the same waterfall) — see `TrancheInput`'s doc comment.
/// `SingleChainProductDetails::tranches` is untouched by this migration —
/// already exactly one chain, nothing to group.
///
/// Every pre-existing flat list is partitioned by `vault.chain_id`, preserving
/// each chain's own relative order — see `group_tranches_by_chain`'s doc
/// comment for why this needs no re-validation of the Senior-before-Junior
/// invariant per chain.
pub mod v3 {
	use super::*;

	/// Snapshot of `ProductDetails` as it existed under `STORAGE_VERSION::new(2)`
	/// — `SingleChain`'s shape is untouched between v2 and v3 (reuses the live
	/// `SingleChainProductDetails` directly), `Multichain`'s inner shape still
	/// has flat `tranches` — reuses `v2::MultichainProductDetailsV1` directly,
	/// since nothing about that shape changed between v1-final and v2-final;
	/// `tranches`' own shape is exactly what changes here, at v3.
	#[derive(
		Clone,
		Encode,
		Decode,
		DecodeWithMemTracking,
		PartialEq,
		RuntimeDebug,
		TypeInfo,
		MaxEncodedLen,
	)]
	pub enum ProductDetailsV2<AccountId> {
		Multichain(v2::MultichainProductDetailsV1<AccountId>),
		SingleChain(SingleChainProductDetails<AccountId>),
	}

	#[storage_alias]
	type Products<T: Config> = StorageMap<
		Pallet<T>,
		Blake2_128Concat,
		ProductId,
		ProductDetailsV2<<T as frame_system::Config>::AccountId>,
	>;

	pub struct MigrateV2ToV3<T>(PhantomData<T>);

	impl<T: Config> UncheckedOnRuntimeUpgrade for MigrateV2ToV3<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();

			let products = Products::<T>::drain().collect::<Vec<_>>();
			weight = weight.saturating_add(
				T::DbWeight::get().reads_writes(products.len() as u64, products.len() as u64),
			);
			let products_count = products.len();
			for (product_id, old) in products {
				let new = match old {
					ProductDetailsV2::Multichain(product) => {
						ProductDetails::Multichain(MultichainProductDetails {
							valuation: product.valuation,
							tranches: group_tranches_by_chain(product.tranches),
							multichain_adapters: product.multichain_adapters,
							multichain_tranche_managers: product.multichain_tranche_managers,
						})
					},
					ProductDetailsV2::SingleChain(product) => ProductDetails::SingleChain(product),
				};
				crate::Products::<T>::insert(product_id, new);
			}
			weight = weight.saturating_add(T::DbWeight::get().writes(products_count as u64));

			log!(
				info,
				"tranche-system v2->v3: converted {} Multichain products' tranches from flat, product-wide order to per-chain-keyed order (SingleChain products untouched) ✅",
				products_count,
			);

			weight
		}
	}

	/// Gated `on_chain == 2 && in_code == 3`, and bumps the on-chain version itself —
	/// wire this (not `MigrateV2ToV3` directly) into the pallet's own hook.
	pub type MigrateToV3<T> = VersionedMigration<
		2,
		3,
		MigrateV2ToV3<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;
}

/// v3 -> v4: introduces this pallet's `FlowVersion` axis (see `lib.rs`'s
/// `FlowVersion` doc comment) — `RequestFlowVersion`/`SettlementFlowVersion`
/// are backfilled with `FlowVersion::V1` for every product already registered
/// as of this upgrade. Genuinely correct, not a placeholder: no product could
/// have been registered under any other `FlowVersion` before this upgrade,
/// since no other version existed yet. `create_product`/
/// `create_single_chain_product` write both unconditionally from now on, so
/// this migration only ever needs to run once, over whatever already existed
/// at upgrade time — any product registered *after* this migration runs
/// already gets both from those extrinsics directly.
pub mod v4 {
	use super::*;
	use crate::FlowVersion;

	pub struct MigrateV3ToV4<T>(PhantomData<T>);

	impl<T: Config> UncheckedOnRuntimeUpgrade for MigrateV3ToV4<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();

			let product_ids: Vec<ProductId> = crate::Products::<T>::iter_keys().collect();
			let product_ids_count = product_ids.len();
			weight = weight.saturating_add(T::DbWeight::get().reads(product_ids_count as u64));
			for product_id in product_ids {
				crate::RequestFlowVersion::<T>::insert(product_id, FlowVersion::V1);
				crate::SettlementFlowVersion::<T>::insert(product_id, FlowVersion::V1);
			}
			weight = weight.saturating_add(T::DbWeight::get().writes(product_ids_count as u64 * 2));

			log!(
				info,
				"tranche-system v3->v4: backfilled request/settlement flow_version=V1 for {} products ✅",
				product_ids_count,
			);

			weight
		}
	}

	/// Gated `on_chain == 3 && in_code == 4`, and bumps the on-chain version itself —
	/// wire this (not `MigrateV3ToV4` directly) into the pallet's own hook.
	pub type MigrateToV4<T> = VersionedMigration<
		3,
		4,
		MigrateV3ToV4<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;
}

/// v4 -> v5: `CollateralAsset` (nested under `SourceType::OffchainSource` →
/// `AdapterInfo` → `MultichainAdapterInfo`/flat `adapters` →
/// `MultichainProductDetails`/`SingleChainProductDetails` → `ProductDetails`)
/// gains `chain_id: u64` — see `lib.rs`'s `CollateralAsset` doc comment for
/// why (an NFT collateral need not live on Bifrost itself). Every
/// pre-existing `CollateralAsset` backfills `chain_id` with this chain's own
/// EVM chain id (`<T as pallet_evm::Config>::ChainId::get()`) — genuinely
/// correct, not an approximation: every `CollateralAsset` recorded before
/// this upgrade could only ever have meant a Bifrost-deployed NFT contract
/// (the field's own doc comment said so explicitly, "on Bifrost EVM"), so
/// this is the exact value that field always implicitly meant.
pub mod v5 {
	use super::*;
	use crate::{
		AdapterInfo, CollateralAsset, MultichainAdapterInfo, SourceType,
		MAX_ADAPTERS_PER_MULTICHAIN_ADAPTER, MAX_ADAPTERS_PER_SINGLE_CHAIN_PRODUCT,
		MAX_COLLATERALS,
	};
	use sp_core::U256;

	/// `CollateralAsset` as it existed under `STORAGE_VERSION::new(4)`, before
	/// `chain_id` existed.
	#[derive(
		Clone,
		Encode,
		Decode,
		DecodeWithMemTracking,
		PartialEq,
		RuntimeDebug,
		TypeInfo,
		MaxEncodedLen,
	)]
	pub struct CollateralAssetV0 {
		pub nft_contract: H160,
		pub nft_token_id: U256,
	}

	/// `SourceType` as it existed under `STORAGE_VERSION::new(4)`, before
	/// `CollateralAsset` gained `chain_id`.
	#[derive(
		Clone,
		Encode,
		Decode,
		DecodeWithMemTracking,
		PartialEq,
		RuntimeDebug,
		TypeInfo,
		MaxEncodedLen,
	)]
	pub enum SourceTypeV0<AccountId> {
		OffchainSource {
			borrower: AccountId,
			collaterals: BoundedVec<CollateralAssetV0, ConstU32<MAX_COLLATERALS>>,
		},
		OnchainSource,
	}

	/// `AdapterInfo` as it existed under `STORAGE_VERSION::new(4)`.
	#[derive(
		Clone,
		Encode,
		Decode,
		DecodeWithMemTracking,
		PartialEq,
		RuntimeDebug,
		TypeInfo,
		MaxEncodedLen,
	)]
	pub struct AdapterInfoV0<AccountId> {
		pub source_type: SourceTypeV0<AccountId>,
		pub weight_bps: u16,
	}

	/// `MultichainAdapterInfo` as it existed under `STORAGE_VERSION::new(4)`.
	#[derive(
		Clone,
		Encode,
		Decode,
		DecodeWithMemTracking,
		PartialEq,
		RuntimeDebug,
		TypeInfo,
		MaxEncodedLen,
	)]
	pub struct MultichainAdapterInfoV0<AccountId> {
		pub weight_bps: u16,
		pub adapters: BoundedBTreeMap<
			H160,
			AdapterInfoV0<AccountId>,
			ConstU32<MAX_ADAPTERS_PER_MULTICHAIN_ADAPTER>,
		>,
	}

	/// `MultichainProductDetails` as it existed under `STORAGE_VERSION::new(4)`
	/// — only `multichain_adapters`' nested shape differs; every other field
	/// reuses the current (unchanged) type directly.
	#[derive(
		Clone,
		Encode,
		Decode,
		DecodeWithMemTracking,
		PartialEq,
		RuntimeDebug,
		TypeInfo,
		MaxEncodedLen,
	)]
	pub struct MultichainProductDetailsV0<AccountId> {
		pub valuation: ValuationInfo,
		pub tranches: BoundedBTreeMap<u64, ChainTranches, ConstU32<MAX_TRANCHE_CHAINS>>,
		pub multichain_adapters: BoundedBTreeMap<
			AdapterKey,
			MultichainAdapterInfoV0<AccountId>,
			ConstU32<MAX_MULTICHAIN_ADAPTERS>,
		>,
		pub multichain_tranche_managers: BoundedBTreeMap<u64, H160, ConstU32<MAX_TRANCHE_MANAGERS>>,
	}

	/// `SingleChainProductDetails` as it existed under `STORAGE_VERSION::new(4)`
	/// — only `adapters`' nested shape differs; every other field reuses the
	/// current (unchanged) type directly.
	#[derive(
		Clone,
		Encode,
		Decode,
		DecodeWithMemTracking,
		PartialEq,
		RuntimeDebug,
		TypeInfo,
		MaxEncodedLen,
	)]
	pub struct SingleChainProductDetailsV0<AccountId> {
		pub valuation: crate::SingleChainValuationInfo,
		pub chain_id: u64,
		pub tranches: ChainTranches,
		pub tranche_manager: H160,
		pub adapters: BoundedBTreeMap<
			H160,
			AdapterInfoV0<AccountId>,
			ConstU32<MAX_ADAPTERS_PER_SINGLE_CHAIN_PRODUCT>,
		>,
		pub ledger: H160,
	}

	/// `ProductDetails` as it existed under `STORAGE_VERSION::new(4)`.
	#[derive(
		Clone,
		Encode,
		Decode,
		DecodeWithMemTracking,
		PartialEq,
		RuntimeDebug,
		TypeInfo,
		MaxEncodedLen,
	)]
	pub enum ProductDetailsV0<AccountId> {
		Multichain(MultichainProductDetailsV0<AccountId>),
		SingleChain(SingleChainProductDetailsV0<AccountId>),
	}

	#[storage_alias]
	type Products<T: Config> = StorageMap<
		Pallet<T>,
		Blake2_128Concat,
		ProductId,
		ProductDetailsV0<<T as frame_system::Config>::AccountId>,
	>;

	/// Converts one `AdapterInfoV0` into the current `AdapterInfo`, backfilling
	/// `chain_id` on every nested `CollateralAsset` — only meaningful for
	/// `SourceType::OffchainSource` (`OnchainSource` carries no collaterals at
	/// all). Also counts how many `CollateralAsset` entries it touched, purely
	/// for this migration's own log line.
	fn migrate_adapter_info<AccountId>(
		old: AdapterInfoV0<AccountId>,
		local_chain_id: u64,
		collaterals_count: &mut u64,
	) -> AdapterInfo<AccountId> {
		let source_type = match old.source_type {
			SourceTypeV0::OffchainSource { borrower, collaterals } => {
				let mut new_collaterals = BoundedVec::default();
				for c in collaterals {
					// Bounded at `MAX_COLLATERALS` on both sides, converting 1:1 from
					// an already correctly-bounded source — can never fail.
					let _ = new_collaterals.try_push(CollateralAsset {
						chain_id: local_chain_id,
						nft_contract: c.nft_contract,
						nft_token_id: c.nft_token_id,
					});
					*collaterals_count += 1;
				}
				SourceType::OffchainSource { borrower, collaterals: new_collaterals }
			},
			SourceTypeV0::OnchainSource => SourceType::OnchainSource,
		};
		AdapterInfo { source_type, weight_bps: old.weight_bps }
	}

	pub struct MigrateV4ToV5<T>(PhantomData<T>);

	impl<T: Config> UncheckedOnRuntimeUpgrade for MigrateV4ToV5<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();
			let local_chain_id = <T as pallet_evm::Config>::ChainId::get();
			let mut collaterals_count = 0u64;

			let products = Products::<T>::drain().collect::<Vec<_>>();
			// `drain()` removes each entry as it's iterated (1 read + 1 write per
			// entry), separate from the reinsert-writes charged below.
			weight = weight.saturating_add(
				T::DbWeight::get().reads_writes(products.len() as u64, products.len() as u64),
			);
			let products_count = products.len();
			for (product_id, old) in products {
				let new = match old {
					ProductDetailsV0::Multichain(product) => {
						let mut multichain_adapters = BoundedBTreeMap::new();
						for (key, info) in product.multichain_adapters {
							let mut adapters = BoundedBTreeMap::new();
							for (address, adapter_info) in info.adapters {
								// Bounded at `MAX_ADAPTERS_PER_MULTICHAIN_ADAPTER` on both
								// sides, same key, converting 1:1 from an already
								// correctly-bounded source — can never fail.
								let _ = adapters.try_insert(
									address,
									migrate_adapter_info(
										adapter_info,
										local_chain_id,
										&mut collaterals_count,
									),
								);
							}
							// Bounded at `MAX_MULTICHAIN_ADAPTERS` on both sides, same
							// key — can never fail, same reasoning as above.
							let _ = multichain_adapters.try_insert(
								key,
								MultichainAdapterInfo { weight_bps: info.weight_bps, adapters },
							);
						}
						ProductDetails::Multichain(MultichainProductDetails {
							valuation: product.valuation,
							tranches: product.tranches,
							multichain_adapters,
							multichain_tranche_managers: product.multichain_tranche_managers,
						})
					},
					ProductDetailsV0::SingleChain(product) => {
						let mut adapters = BoundedBTreeMap::new();
						for (address, adapter_info) in product.adapters {
							// Bounded at `MAX_ADAPTERS_PER_SINGLE_CHAIN_PRODUCT` on both
							// sides, same key, converting 1:1 from an already
							// correctly-bounded source — can never fail.
							let _ = adapters.try_insert(
								address,
								migrate_adapter_info(
									adapter_info,
									local_chain_id,
									&mut collaterals_count,
								),
							);
						}
						ProductDetails::SingleChain(SingleChainProductDetails {
							valuation: product.valuation,
							chain_id: product.chain_id,
							tranches: product.tranches,
							tranche_manager: product.tranche_manager,
							adapters,
							ledger: product.ledger,
						})
					},
				};
				crate::Products::<T>::insert(product_id, new);
			}
			weight = weight.saturating_add(T::DbWeight::get().writes(products_count as u64));

			log!(
				info,
				"tranche-system v4->v5: backfilled chain_id={} on {} CollateralAsset entries across {} products ✅",
				local_chain_id,
				collaterals_count,
				products_count,
			);

			weight
		}
	}

	/// Gated `on_chain == 4 && in_code == 5`, and bumps the on-chain version itself —
	/// wire this (not `MigrateV4ToV5` directly) into the pallet's own hook.
	pub type MigrateToV5<T> = VersionedMigration<
		4,
		5,
		MigrateV4ToV5<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;
}
