//! Storage migrations for `pallet-tranche-permissions`.
//!
//! Only the latest migration is kept: every live chain running this pallet is
//! already at the version the previous one targeted (v1).

use crate::{Config, Pallet};
use pallet_tranche_system::{legacy::VaultIdV1, ChainAddress, ProductId, VaultId};

use frame_support::{
	migrations::VersionedMigration, pallet_prelude::*, storage_alias,
	traits::UncheckedOnRuntimeUpgrade, weights::Weight, Blake2_128Concat,
};
use sp_core::H160;
use sp_std::{marker::PhantomData, vec::Vec};

pub(crate) const LOG_TARGET: &str = "runtime::tranche-permissions";

macro_rules! log {
	($level:tt, $patter:expr $(, $values:expr)* $(,)?) => {
		log::$level!(
			target: LOG_TARGET,
			$patter $(, $values)*
		)
	};
}

/// v1 -> v2: non-EVM support. `TrancheInvestors`' investor key changes from the
/// Hub `AccountId` (20-byte `AccountId20` on every Bifrost runtime, hence decoded
/// here as `H160`) to a 32-byte `ChainAddress` on the vault's chain, and its
/// `VaultId` key widens (`vault_address: H160 -> ChainAddress`). Both are
/// left-padded; no entry is added or dropped.
pub mod v2 {
	use super::*;

	#[storage_alias]
	pub type TrancheInvestors<T: Config> = StorageNMap<
		Pallet<T>,
		(
			NMapKey<Blake2_128Concat, ProductId>,
			NMapKey<Blake2_128Concat, VaultIdV1>,
			NMapKey<Blake2_128Concat, H160>,
		),
		(),
	>;

	pub struct MigrateV1ToV2<T>(PhantomData<T>);

	impl<T: Config> UncheckedOnRuntimeUpgrade for MigrateV1ToV2<T> {
		fn on_runtime_upgrade() -> Weight {
			// Collect before re-inserting — old and new keys share the map prefix.
			let entries = TrancheInvestors::<T>::iter_keys().collect::<Vec<_>>();
			let _ = TrancheInvestors::<T>::clear(u32::MAX, None);
			let count = entries.len() as u64;

			for (product_id, vault, investor) in entries {
				let vault: VaultId = vault.into();
				let investor: ChainAddress = investor.into();
				crate::TrancheInvestors::<T>::insert((product_id, vault, investor), ());
			}

			log!(info, "tranche-permissions v2: re-keyed {} TrancheInvestors entries", count);

			// Each entry: 1 read + 1 write (clear) + 1 write (insert).
			T::DbWeight::get().reads_writes(count, count.saturating_mul(2))
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			Ok((TrancheInvestors::<T>::iter_keys().count() as u32).encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			let before: u32 =
				Decode::decode(&mut &state[..]).map_err(|_| "v2: bad pre_upgrade state")?;
			ensure!(
				crate::TrancheInvestors::<T>::iter_keys().count() as u32 == before,
				"v2: TrancheInvestors entries lost"
			);
			Ok(())
		}
	}

	pub type MigrateToV2<T> = VersionedMigration<
		1,
		2,
		MigrateV1ToV2<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;
}
