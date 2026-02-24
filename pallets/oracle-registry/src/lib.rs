//! # Oracle Registry Pallet
//!
//! This pallet manages oracle ID mappings for EVM-compatible assets and native currencies.
//!
//! ## Overview
//!
//! The pallet provides:
//! - Registry mapping EVM asset contract addresses to their oracle IDs
//! - Registry mapping chain IDs to their native currency oracle IDs
//! - Root-gated set/remove operations for both registries
//!
//! Oracle IDs are used by other pallets (e.g., fee payment) to fetch prices from
//! off-chain price feeds.

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]

pub mod types;
pub mod weights;

pub use pallet::*;
pub use types::*;
pub use weights::WeightInfo;

use frame_support::traits::StorageVersion;

/// The current storage version.
const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	#[pallet::without_storage_info]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Weight information for extrinsics.
		type WeightInfo: WeightInfo;
	}

	#[pallet::storage]
	#[pallet::unbounded]
	/// Mapping from asset addresses to their oracle IDs.
	///
	/// This storage maps EVM-compatible asset contract addresses to their corresponding
	/// oracle IDs. Oracle IDs are used to fetch the price of the asset from the
	/// price oracle.
	///
	/// - **Key**: `AssetId` (H160) - The EVM-compatible asset contract address
	/// - **Value**: `AssetOracleId` (H256) - The oracle ID
	pub type AssetOracles<T: Config> = StorageMap<_, Twox64Concat, AssetId, AssetOracleId>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// Mapping from chain IDs to their native currency oracle IDs.
	///
	/// This storage maps chain IDs to their corresponding native currency oracle IDs.
	/// Native currency oracle IDs are used to fetch the price of the native currency from the
	/// price oracle.
	///
	/// - **Key**: `ChainId` (u32) - The chain ID
	/// - **Value**: `AssetOracleId` (H256) - The oracle ID
	pub type NativeCurrencyOracles<T: Config> = StorageMap<_, Twox64Concat, ChainId, AssetOracleId>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An asset oracle ID has been set or updated.
		AssetOracleSet { asset: AssetId, oracle_id: AssetOracleId },
		/// An asset oracle ID has been removed.
		AssetOracleRemoved { asset: AssetId },
		/// A native currency oracle ID has been set or updated.
		NativeCurrencyOracleSet { chain_id: ChainId, oracle_id: AssetOracleId },
		/// A native currency oracle ID has been removed.
		NativeCurrencyOracleRemoved { chain_id: ChainId },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The asset oracle does not exist. Use `set_asset_oracle` to add it.
		AssetDNE,
		/// The native currency oracle for this chain does not exist.
		NativeCurrencyChainDNE,
		/// Cannot write the same value that is already stored.
		NoWritingSameValue,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::set_asset_oracle())]
		/// Set or update the oracle ID for an asset.
		///
		/// Inserts the oracle ID if the asset is not yet registered, or updates it if already
		/// present. Rejects the call if the new oracle ID is identical to the current one.
		///
		/// # Parameters
		/// * `origin` - Must be `Root` (sudo access required)
		/// * `asset` - The asset contract address (H160)
		/// * `asset_oracle_id` - The oracle ID (H256)
		pub fn set_asset_oracle(
			origin: OriginFor<T>,
			asset: AssetId,
			asset_oracle_id: AssetOracleId,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;

			if let Some(current) = AssetOracles::<T>::get(asset) {
				ensure!(current != asset_oracle_id, Error::<T>::NoWritingSameValue);
			}
			AssetOracles::<T>::insert(asset, asset_oracle_id);

			Self::deposit_event(Event::AssetOracleSet { asset, oracle_id: asset_oracle_id });

			Ok(().into())
		}

		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::remove_asset_oracle())]
		/// Remove the oracle ID for an asset.
		///
		/// # Parameters
		/// * `origin` - Must be `Root` (sudo access required)
		/// * `asset` - The asset contract address (H160)
		pub fn remove_asset_oracle(
			origin: OriginFor<T>,
			asset: AssetId,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;

			ensure!(AssetOracles::<T>::contains_key(asset), Error::<T>::AssetDNE);
			AssetOracles::<T>::remove(asset);

			Self::deposit_event(Event::AssetOracleRemoved { asset });

			Ok(().into())
		}

		#[pallet::call_index(3)]
		#[pallet::weight(<T as Config>::WeightInfo::set_native_currency_oracle())]
		/// Set or update the native currency oracle ID for a chain.
		///
		/// Inserts the oracle ID if the chain is not yet registered, or updates it if already
		/// present. Rejects the call if the new oracle ID is identical to the current one.
		///
		/// # Parameters
		/// * `origin` - Must be `Root` (sudo access required)
		/// * `chain_id` - The chain ID (u32)
		/// * `native_currency_oracle_id` - The native currency oracle ID (H256)
		pub fn set_native_currency_oracle(
			origin: OriginFor<T>,
			chain_id: ChainId,
			native_currency_oracle_id: AssetOracleId,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;

			if let Some(current) = NativeCurrencyOracles::<T>::get(chain_id) {
				ensure!(current != native_currency_oracle_id, Error::<T>::NoWritingSameValue);
			}
			NativeCurrencyOracles::<T>::insert(chain_id, native_currency_oracle_id);

			Self::deposit_event(Event::NativeCurrencyOracleSet {
				chain_id,
				oracle_id: native_currency_oracle_id,
			});

			Ok(().into())
		}

		#[pallet::call_index(5)]
		#[pallet::weight(<T as Config>::WeightInfo::remove_native_currency_oracle())]
		/// Remove the native currency oracle ID for a chain.
		///
		/// # Parameters
		/// * `origin` - Must be `Root` (sudo access required)
		/// * `chain_id` - The chain ID (u32)
		pub fn remove_native_currency_oracle(
			origin: OriginFor<T>,
			chain_id: ChainId,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;

			ensure!(
				NativeCurrencyOracles::<T>::contains_key(chain_id),
				Error::<T>::NativeCurrencyChainDNE
			);
			NativeCurrencyOracles::<T>::remove(chain_id);

			Self::deposit_event(Event::NativeCurrencyOracleRemoved { chain_id });

			Ok(().into())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Get the oracle ID for an asset, if registered.
		pub fn get_asset_oracle(asset: &AssetId) -> Option<AssetOracleId> {
			AssetOracles::<T>::get(asset)
		}

		/// Get the native currency oracle ID for a chain, if registered.
		pub fn get_native_currency_oracle(chain_id: ChainId) -> Option<AssetOracleId> {
			NativeCurrencyOracles::<T>::get(chain_id)
		}
	}
}
