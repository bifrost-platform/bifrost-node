//! Storage migrations for the EVM Fee Token pallet.

use crate::{types::FeeTokenConfigV0, Config, Pallet};
use frame_support::{
	pallet_prelude::*,
	storage_alias,
	traits::{Get, OnRuntimeUpgrade},
	weights::Weight,
};
use sp_core::H160;

#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

/// V1 migration: Add `min_balance` field to `FeeTokenConfig`.
///
/// This migration:
/// 1. Reads all existing `AcceptedFeeTokens` entries (stored as `FeeTokenConfigV0`)
/// 2. Converts them to `FeeTokenConfig` with `min_balance = 0`
/// 3. Writes them back to storage
/// 4. Updates the storage version from 0 to 1
pub mod v1 {
	use super::*;

	/// Old storage type for `AcceptedFeeTokens` using `FeeTokenConfigV0`.
	/// This is only used for pre_upgrade count check.
	#[storage_alias]
	pub type AcceptedFeeTokensV0<T: Config> =
		StorageMap<Pallet<T>, Blake2_128Concat, H160, FeeTokenConfigV0, OptionQuery>;

	pub struct MigrateToV1<T>(sp_std::marker::PhantomData<T>);

	impl<T: Config> OnRuntimeUpgrade for MigrateToV1<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();

			let current = Pallet::<T>::in_code_storage_version();
			let onchain = Pallet::<T>::on_chain_storage_version();

			log::info!(
				target: "bifrost-tx-payment",
				"Running migration with in-code version {:?} and on-chain version {:?}",
				current, onchain
			);

			// Only migrate if on-chain version is 0 and in-code version is 1
			if current == 1 && onchain == 0 {
				log::info!(
					target: "bifrost-tx-payment",
					"Starting migration from V0 to V1 (adding min_balance to FeeTokenConfig)"
				);

				let mut count: u32 = 0;

				// Use translate to safely convert old storage format to new format in-place
				crate::pallet::AcceptedFeeTokens::<T>::translate::<FeeTokenConfigV0, _>(
					|_token, old_config| {
						count += 1;
						// Convert V0 to V1 (adds min_balance = 0)
						Some(old_config.into())
					},
				);

				weight = weight
					.saturating_add(T::DbWeight::get().reads_writes(count as u64, count as u64));

				// Update storage version
				current.put::<Pallet<T>>();
				weight = weight.saturating_add(T::DbWeight::get().writes(1));

				log::info!(
					target: "bifrost-tx-payment",
					"Migration V0 -> V1 completed: migrated {} fee token configs",
					count
				);
			} else {
				log::info!(
					target: "bifrost-tx-payment",
					"Skipping migration V0 -> V1 (on-chain={:?}, in-code={:?})",
					onchain, current
				);
				weight = weight.saturating_add(T::DbWeight::get().reads(1));
			}

			weight
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
			let onchain = Pallet::<T>::on_chain_storage_version();

			// Count existing entries using old storage alias
			let count = AcceptedFeeTokensV0::<T>::iter().count() as u32;

			log::info!(
				target: "bifrost-tx-payment",
				"pre_upgrade: on-chain version {:?}, {} fee token configs to migrate",
				onchain, count
			);

			// Encode the count for post_upgrade verification
			Ok(count.encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
			let expected_count: u32 =
				Decode::decode(&mut &state[..]).expect("Failed to decode pre_upgrade state");

			// Verify storage version updated
			ensure!(
				Pallet::<T>::on_chain_storage_version() == 1,
				"Storage version should be 1 after migration"
			);

			// Verify all entries migrated correctly
			let actual_count = crate::pallet::AcceptedFeeTokens::<T>::iter().count() as u32;
			ensure!(
				actual_count == expected_count,
				"Fee token config count mismatch after migration"
			);

			// Verify all entries have valid FeeTokenConfig (with min_balance field)
			for (token, config) in crate::pallet::AcceptedFeeTokens::<T>::iter() {
				log::info!(
					target: "bifrost-tx-payment",
					"post_upgrade: token {:?} has min_balance {:?}",
					token, config.min_balance
				);
			}

			log::info!(
				target: "bifrost-tx-payment",
				"post_upgrade: migration V0 -> V1 verified successfully"
			);

			Ok(())
		}
	}
}
