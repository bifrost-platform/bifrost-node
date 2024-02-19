mod impls;

use crate::{BoundedBitcoinAddress, BoundedSignature, PoolMember, WeightInfo};

use ethers_core::types::Signature;
use frame_support::{pallet_prelude::*, traits::StorageVersion};
use frame_system::pallet_prelude::*;
use sp_core::H160;
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
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T> {
		UserBfcAddressAlreadyRegistered,
		RefundAddressAlreadyRegistered,
		VaultAddressAlreadyRegistered,
		InvalidSignature,
		NoWritingSameValue,
		IssuerDNE,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		Registered {
			user_bfc_address: T::AccountId,
			vault_address: BoundedBitcoinAddress,
			refund_address: BoundedBitcoinAddress,
		},
	}

	#[pallet::storage]
	#[pallet::getter(fn address_issuer)]
	pub type AddressIssuer<T: Config> = StorageValue<_, T::AccountId, OptionQuery>;

	#[pallet::storage]
	#[pallet::unbounded]
	#[pallet::getter(fn registration_pool)]
	pub type RegistrationPool<T: Config> =
		StorageValue<_, BTreeMap<T::AccountId, PoolMember>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn bonded_vault)]
	pub type BondedVault<T: Config> =
		StorageMap<_, Twox64Concat, BoundedBitcoinAddress, T::AccountId>;

	#[pallet::storage]
	#[pallet::getter(fn bonded_user)]
	pub type BondedUser<T: Config> =
		StorageMap<_, Twox64Concat, BoundedBitcoinAddress, T::AccountId>;

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

	#[pallet::call]
	impl<T: Config> Pallet<T>
	where
		T::AccountId: Into<H160>,
	{
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::set_issuer())]
		pub fn set_issuer(origin: OriginFor<T>, new: T::AccountId) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			if let Some(old) = <AddressIssuer<T>>::get() {
				ensure!(old != new, Error::<T>::NoWritingSameValue);
			}
			<AddressIssuer<T>>::put(new);
			Ok(().into())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::register())]
		pub fn register(
			origin: OriginFor<T>,
			vault_address: BoundedBitcoinAddress,
			refund_address: BoundedBitcoinAddress,
			signature: BoundedSignature,
		) -> DispatchResultWithPostInfo {
			let user_bfc_address = ensure_signed(origin)?;
			let issuer = <AddressIssuer<T>>::get().ok_or(<Error<T>>::IssuerDNE)?;
			ensure!(
				!<BondedVault<T>>::contains_key(&vault_address),
				Error::<T>::VaultAddressAlreadyRegistered
			);
			ensure!(
				!<BondedUser<T>>::contains_key(&refund_address),
				Error::<T>::RefundAddressAlreadyRegistered
			);

			let mut registration_pool = <RegistrationPool<T>>::get();
			ensure!(
				!registration_pool.contains_key(&user_bfc_address),
				Error::<T>::UserBfcAddressAlreadyRegistered
			);

			let signature = Signature::try_from(signature.into_inner().as_slice())
				.map_err(|_| <Error<T>>::InvalidSignature)?;

			let message =
				format!("{}:{:?}:{:?}", user_bfc_address.clone(), refund_address, vault_address);
			signature
				.verify(message, issuer.into())
				.map_err(|_| <Error<T>>::InvalidSignature)?;

			<BondedVault<T>>::insert(&vault_address, &user_bfc_address);
			<BondedUser<T>>::insert(&refund_address, &user_bfc_address);
			registration_pool.insert(
				user_bfc_address.clone(),
				PoolMember::new(vault_address.clone(), refund_address.clone()),
			);
			<RegistrationPool<T>>::put(registration_pool);

			Self::deposit_event(Event::Registered {
				user_bfc_address,
				vault_address,
				refund_address,
			});

			Ok(().into())
		}
	}
}
