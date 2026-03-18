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

/// V2 migration: Remove oracle_address and oracle_decimals from FeeTokenConfig,
/// switch to oracle-registry with chain-ID-based native oracle lookup.
///
/// This migration:
/// 1. Translates `AcceptedFeeTokens` from `FeeTokenConfigV0` to `FeeTokenConfig`
///    - Drops `oracle_address` and `oracle_decimals` fields
/// 2. Removes old `NativeTokenOracle` storage (H160)
/// 3. Removes old `NativeOracleDecimals` storage (u8)
/// 4. Updates the storage version from 1 to 2
///
/// Note: After migration, the native currency oracle is resolved via
/// `OracleRegistry::get_native_currency_oracle(chain_id)`. Ensure the
/// oracle-registry has the BFC/USD oracle registered for the chain ID.
pub mod v1 {
	use super::*;

	/// Old storage type for `AcceptedFeeTokens` using `FeeTokenConfigV0`.
	#[storage_alias]
	pub type AcceptedFeeTokensV0<T: Config> =
		StorageMap<Pallet<T>, Blake2_128Concat, H160, FeeTokenConfigV0, OptionQuery>;

	/// Old storage: NativeTokenOracle (H160 address).
	#[storage_alias]
	pub type NativeTokenOracle<T: Config> = StorageValue<Pallet<T>, H160, OptionQuery>;

	/// Old storage: NativeOracleDecimals (u8).
	#[storage_alias]
	pub type NativeOracleDecimals<T: Config> = StorageValue<Pallet<T>, u8, ValueQuery>;

	pub struct MigrateToV2<T>(sp_std::marker::PhantomData<T>);

	impl<T: Config> OnRuntimeUpgrade for MigrateToV2<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();

			let current = Pallet::<T>::in_code_storage_version();
			let onchain = Pallet::<T>::on_chain_storage_version();

			log::info!(
				target: "bifrost-tx-payment",
				"V2 migration: in-code version {:?}, on-chain version {:?}",
				current, onchain
			);

			if current == 2 && onchain == 1 {
				log::info!(
					target: "bifrost-tx-payment",
					"Starting migration from V1 to V2 (removing oracle_address/oracle_decimals, switching to oracle-registry)"
				);

				let mut count: u32 = 0;

				// Translate FeeTokenConfigV0 → FeeTokenConfig
				crate::pallet::AcceptedFeeTokens::<T>::translate::<FeeTokenConfigV0, _>(
					|_token, old_config| {
						count += 1;
						Some(old_config.into())
					},
				);

				weight = weight
					.saturating_add(T::DbWeight::get().reads_writes(count as u64, count as u64));

				// Remove old NativeTokenOracle (H160) storage
				NativeTokenOracle::<T>::kill();
				weight = weight.saturating_add(T::DbWeight::get().writes(1));

				// Remove old NativeOracleDecimals storage
				NativeOracleDecimals::<T>::kill();
				weight = weight.saturating_add(T::DbWeight::get().writes(1));

				// Update storage version
				current.put::<Pallet<T>>();
				weight = weight.saturating_add(T::DbWeight::get().writes(1));

				log::info!(
					target: "bifrost-tx-payment",
					"Migration V1 -> V2 completed: migrated {} fee token configs, removed NativeTokenOracle and NativeOracleDecimals",
					count
				);
			} else {
				log::info!(
					target: "bifrost-tx-payment",
					"Skipping migration V1 -> V2 (on-chain={:?}, in-code={:?})",
					onchain, current
				);
				weight = weight.saturating_add(T::DbWeight::get().reads(1));
			}

			weight
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
			let onchain = Pallet::<T>::on_chain_storage_version();

			let count = if onchain == 1 {
				AcceptedFeeTokensV0::<T>::iter().count() as u32
			} else {
				0u32
			};

			log::info!(
				target: "bifrost-tx-payment",
				"pre_upgrade V1 -> V2: on-chain version {:?}, {} fee token configs to migrate",
				onchain, count
			);

			Ok(count.encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
			let expected_count: u32 =
				Decode::decode(&mut &state[..]).expect("Failed to decode pre_upgrade state");

			ensure!(
				Pallet::<T>::on_chain_storage_version() == 2,
				"Storage version should be 2 after migration"
			);

			let actual_count = crate::pallet::AcceptedFeeTokens::<T>::iter().count() as u32;
			ensure!(
				actual_count == expected_count,
				"Fee token config count mismatch after migration"
			);

			// Verify old storage items were removed
			ensure!(
				NativeTokenOracle::<T>::get().is_none(),
				"NativeTokenOracle should be removed after migration"
			);

			log::info!(
				target: "bifrost-tx-payment",
				"post_upgrade: migration V1 -> V2 verified successfully ({} configs)",
				actual_count
			);

			Ok(())
		}
	}
}
