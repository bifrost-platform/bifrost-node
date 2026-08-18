use crate::{
	AdapterKey, Config, MultichainAdapterInfo, MultichainProductDetails, Pallet, ProductDetails,
	ProductId, Tranche, ValuationInfo, MAX_MULTICHAIN_ADAPTERS, MAX_TRANCHES, MAX_TRANCHE_MANAGERS,
};

use frame_support::{
	migrations::VersionedMigration, pallet_prelude::*, storage_alias,
	traits::UncheckedOnRuntimeUpgrade, weights::Weight, Blake2_128Concat,
};
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::{ConstU32, H160};
use sp_runtime::{BoundedBTreeMap, BoundedVec, RuntimeDebug};
use sp_std::{collections::btree_set::BTreeSet, marker::PhantomData, vec::Vec};

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
		pub tranches: BoundedVec<Tranche, ConstU32<MAX_TRANCHES>>,
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
				// `MAX_TRANCHES` (10) <= `MAX_TRANCHE_MANAGERS` (20), so the set of
				// distinct tranche chain_ids can never overflow this bound — falls
				// back to an empty table in the unreachable case it somehow did,
				// rather than panicking a migration.
				.unwrap_or_default();

				crate::Products::<T>::insert(
					product_id,
					ProductDetails::Multichain(MultichainProductDetails {
						valuation: old.valuation,
						tranches: old.tranches,
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

	/// Exactly `MultichainProductDetails<AccountId>` — this is deliberately not a
	/// separately-named "V1" struct: the on-chain v1 shape's fields are, byte for byte, the
	/// same fields `MultichainProductDetails` already declares, so re-declaring them here
	/// under a new name would just be duplication with no safety benefit (unlike `v1`'s own
	/// `ProductDetailsV0`, which really is a distinct, no-longer-existing shape).
	#[storage_alias]
	type Products<T: Config> = StorageMap<
		Pallet<T>,
		Blake2_128Concat,
		ProductId,
		MultichainProductDetails<<T as frame_system::Config>::AccountId>,
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
				crate::Products::<T>::insert(product_id, ProductDetails::Multichain(old));
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
