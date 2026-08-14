use crate::{
	AdapterKey, Config, MultichainAdapterInfo, Pallet, ProductDetails, ProductId, Tranche,
	ValuationInfo, MAX_MULTICHAIN_ADAPTERS, MAX_TRANCHES, MAX_TRANCHE_MANAGERS,
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

/// v0 -> v1: `ProductDetails` gained a `multichain_tranche_managers: BoundedBTreeMap<u64,
/// H160, ConstU32<MAX_TRANCHE_MANAGERS>>` field — one TrancheManager contract address per
/// chain a product's vaults span, Hub included (see `ProductDetails::
/// multichain_tranche_managers`'s doc comment).
///
/// For every product that already existed before this upgrade, the real TrancheManager
/// addresses aren't recoverable from on-chain state, so this migration backfills one entry
/// per distinct chain the product already has a tranche vault on — derived from `tranches`,
/// which IS already known — each pointed at the zero address as an explicit placeholder,
/// not a real binding. `set_multichain_tranche_managers` must be called afterward, per
/// product, to fill in the genuine addresses; until then, any flow reading this table sees
/// zero addresses rather than an empty (and therefore ambiguous — "not backfilled yet" vs.
/// "genuinely has no vaults") table.
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
					ProductDetails {
						valuation: old.valuation,
						tranches: old.tranches,
						multichain_adapters: old.multichain_adapters,
						multichain_tranche_managers,
					},
				);
			}
			weight = weight.saturating_add(T::DbWeight::get().writes(products_count as u64));

			log!(
				info,
				"tranche-system v0->v1: backfilled multichain_tranche_managers (zero address per distinct tranche chain, must be replaced via set_multichain_tranche_managers) for {} products ✅",
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
