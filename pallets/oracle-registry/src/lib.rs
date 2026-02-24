//! # Oracle Registry Pallet
//!
//! This pallet manages oracle ID mappings for EVM-compatible assets and native currencies.
//!
//! ## Overview
//!
//! The pallet provides:
//! - Registry mapping EVM asset contract addresses to their oracle IDs
//! - Registry mapping chain IDs to their native currency oracle IDs
//! - A configurable oracle manager contract address for EVM-level authorization
//! - Root-gated set/remove operations for all registries
//!
//! Oracle IDs are used by other pallets (e.g., fee payment) to fetch prices from
//! off-chain price feeds. Other pallets access this pallet through the
//! [`bp_oracle::traits::OracleRegistryManager`] trait.

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]

pub mod weights;

pub use bp_oracle::{traits::OracleRegistryManager, AssetId, AssetOracleId, ChainId};
pub use pallet::*;
pub use weights::WeightInfo;

use frame_support::traits::StorageVersion;

/// The current storage version.
const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;
	use sp_core::H160;

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
	/// - **Key**: `AssetId` (H160) - The EVM-compatible asset contract address
	/// - **Value**: `AssetOracleId` (H256) - The oracle ID
	pub type AssetOracles<T: Config> = StorageMap<_, Twox64Concat, AssetId, AssetOracleId>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// Mapping from chain IDs to their native currency oracle IDs.
	///
	/// - **Key**: `ChainId` (u32) - The chain ID
	/// - **Value**: `AssetOracleId` (H256) - The oracle ID
	pub type NativeCurrencyOracles<T: Config> = StorageMap<_, Twox64Concat, ChainId, AssetOracleId>;

	#[pallet::storage]
	/// The EVM contract address authorised to manage the oracle registry.
	///
	/// Precompiles and other pallets can call [`OracleRegistryManager::get_oracle_manager_contract`]
	/// to check whether a given caller is the designated manager.
	pub type OracleManagerContract<T: Config> = StorageValue<_, H160, OptionQuery>;

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
		/// The oracle manager contract address has been set or updated.
		OracleManagerContractSet { contract: H160 },
		/// The oracle manager contract address has been removed.
		OracleManagerContractRemoved,
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The asset oracle does not exist. Use `set_asset_oracle` to add it.
		AssetDNE,
		/// The native currency oracle for this chain does not exist.
		NativeCurrencyChainDNE,
		/// The oracle manager contract is not set.
		OracleManagerContractDNE,
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

		#[pallet::call_index(6)]
		#[pallet::weight(<T as Config>::WeightInfo::set_oracle_manager_contract())]
		/// Set or update the oracle manager contract address.
		///
		/// The oracle manager contract is an EVM contract authorised to perform
		/// oracle registry management. Precompiles and other pallets check this
		/// address via [`OracleRegistryManager::get_oracle_manager_contract`].
		///
		/// # Parameters
		/// * `origin` - Must be `Root` (sudo access required)
		/// * `contract` - The EVM contract address (H160)
		pub fn set_oracle_manager_contract(
			origin: OriginFor<T>,
			contract: H160,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;

			if let Some(current) = OracleManagerContract::<T>::get() {
				ensure!(current != contract, Error::<T>::NoWritingSameValue);
			}
			OracleManagerContract::<T>::put(contract);

			Self::deposit_event(Event::OracleManagerContractSet { contract });

			Ok(().into())
		}

		#[pallet::call_index(7)]
		#[pallet::weight(<T as Config>::WeightInfo::remove_oracle_manager_contract())]
		/// Remove the oracle manager contract address.
		///
		/// # Parameters
		/// * `origin` - Must be `Root` (sudo access required)
		pub fn remove_oracle_manager_contract(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;

			ensure!(
				OracleManagerContract::<T>::get().is_some(),
				Error::<T>::OracleManagerContractDNE
			);
			OracleManagerContract::<T>::kill();

			Self::deposit_event(Event::OracleManagerContractRemoved);

			Ok(().into())
		}
	}

	impl<T: Config> OracleRegistryManager for Pallet<T> {
		fn get_asset_oracle(asset: &AssetId) -> Option<AssetOracleId> {
			AssetOracles::<T>::get(asset)
		}

		fn get_native_currency_oracle(chain_id: ChainId) -> Option<AssetOracleId> {
			NativeCurrencyOracles::<T>::get(chain_id)
		}

		fn get_oracle_manager_contract() -> Option<H160> {
			OracleManagerContract::<T>::get()
		}
	}
}
