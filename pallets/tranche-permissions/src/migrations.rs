use crate::{Config, Pallet};
use pallet_tranche_system::{VaultId, VaultInspect};

use frame_support::{
	migrations::VersionedMigration, pallet_prelude::*, storage_alias,
	traits::UncheckedOnRuntimeUpgrade, weights::Weight, Blake2_128Concat,
};
use sp_std::{marker::PhantomData, vec::Vec};

pub(crate) const LOG_TARGET: &str = "runtime::tranche-permissions";

// syntactic sugar for logging.
macro_rules! log {
	($level:tt, $patter:expr $(, $values:expr)* $(,)?) => {
		log::$level!(
			target: LOG_TARGET,
			$patter $(, $values)*
		)
	};
}

/// v0 -> v1: re-keys `TrancheInvestors` from `VaultId` alone to
/// `(product_id, VaultId)` — see the current storage definition's own doc
/// comment for why keying on `VaultId` alone was unsafe: `VaultId` is only
/// unique while a tranche is *currently registered* —
/// `pallet_tranche_system::set_tranche(Remove)` frees it for a different
/// product to register later, which would otherwise resurrect the old
/// product's stale investor whitelist for the new one.
pub mod v1 {
	use super::*;

	#[storage_alias]
	type TrancheInvestors<T: Config> = StorageDoubleMap<
		Pallet<T>,
		Blake2_128Concat,
		VaultId,
		Blake2_128Concat,
		<T as frame_system::Config>::AccountId,
		(),
	>;

	pub struct MigrateV0ToV1<T>(PhantomData<T>);

	impl<T: Config> UncheckedOnRuntimeUpgrade for MigrateV0ToV1<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();

			let entries = TrancheInvestors::<T>::drain().collect::<Vec<_>>();
			// `drain()` removes each entry as it's iterated (1 read + 1 write per
			// entry), separate from the per-entry lookup/reinsert charged below.
			weight = weight.saturating_add(
				T::DbWeight::get().reads_writes(entries.len() as u64, entries.len() as u64),
			);
			let mut migrated = 0u64;
			let mut dropped = 0u64;
			for (vault, who, ()) in entries {
				weight = weight.saturating_add(T::DbWeight::get().reads(1));
				match T::Vaults::product_id_for_vault(&vault) {
					// Vault still registered — re-key under its *current*
					// owner. If a different product re-registered this exact
					// `VaultId` since this entry was originally written, this
					// correctly attributes it to whoever owns the vault now
					// rather than carrying the stale grant forward under the
					// old product.
					Some(product_id) => {
						crate::TrancheInvestors::<T>::insert((product_id, vault, who), ());
						migrated += 1;
					},
					// Vault no longer registered at all — no current owner to
					// attribute this entry to, so it's simply dropped rather
					// than kept under a synthetic product_id.
					None => {
						dropped += 1;
					},
				}
			}
			weight = weight.saturating_add(T::DbWeight::get().writes(migrated));

			log!(
				info,
				"tranche-permissions v0->v1: re-keyed {} TrancheInvestors entries under their current product_id, dropped {} whose vault is no longer registered ✅",
				migrated,
				dropped,
			);

			weight
		}
	}

	/// Gated `on_chain == 0 && in_code == 1`, and bumps the on-chain version
	/// itself — wire this (not `MigrateV0ToV1` directly) into the pallet's
	/// own hook.
	pub type MigrateToV1<T> = VersionedMigration<
		0,
		1,
		MigrateV0ToV1<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;
}
