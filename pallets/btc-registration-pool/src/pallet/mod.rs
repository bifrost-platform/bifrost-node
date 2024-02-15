mod impls;

use crate::PoolMember;

use frame_support::{pallet_prelude::*, traits::StorageVersion};
use sp_std::collections::btree_map::BTreeMap;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Overarching event type
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
	}

	#[pallet::error]
	pub enum Error<T> {
		UserEthAddressAlreadyRegistered,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		Registered,
	}

	#[pallet::storage]
	#[pallet::getter(fn address_issuer)]
	pub type AddressIssuer<T: Config> = StorageValue<_, T::AccountId, OptionQuery>;

	#[pallet::storage]
	#[pallet::unbounded]
	#[pallet::getter(fn registration_pool)]
	pub type RegistrationPool<T: Config> =
		StorageValue<_, BTreeMap<T::AccountId, PoolMember>, OptionQuery>;

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub address_issuer: T::AccountId,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			<AddressIssuer<T>>::put(self.address_issuer.clone());
		}
	}
}
