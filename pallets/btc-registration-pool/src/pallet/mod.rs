mod impls;

use crate::{BitcoinAddressPair, BoundedBitcoinAddress, WeightInfo};

use frame_support::{pallet_prelude::*, traits::StorageVersion};
use frame_system::pallet_prelude::*;

use scale_info::prelude::format;
use sp_runtime::traits::{IdentifyAccount, Verify};
use sp_std::vec::Vec;

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
		/// The signature signed by the issuer.
		type Signature: Verify<Signer = Self::Signer> + Encode + Decode + Parameter;
		/// The signer of the message.
		type Signer: IdentifyAccount<AccountId = Self::AccountId>
			+ Encode
			+ Decode
			+ Parameter
			+ MaxEncodedLen;
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The Bifrost address is already registered.
		UserBfcAddressAlreadyRegistered,
		/// The refund Bitcoin address is already registered.
		RefundAddressAlreadyRegistered,
		/// The vault Bitcoin address is already registered.
		VaultAddressAlreadyRegistered,
		/// The requested refund and vault address are identical. It must be different.
		RefundAndVaultAddressIdentical,
		/// The requested refund or vault address is invalid.
		InvalidBitcoinAddress,
		/// The requested signature is invalid.
		InvalidSignature,
		/// Cannot set the value as identical to the previous value.
		NoWritingSameValue,
		/// The issuer is not set yet.
		IssuerDNE,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new user registered its Bitcoin address pair.
		Registered {
			user_bfc_address: T::AccountId,
			refund_address: BoundedBitcoinAddress,
			vault_address: BoundedBitcoinAddress,
		},
	}

	#[pallet::storage]
	#[pallet::getter(fn address_issuer)]
	/// The issuer address. The signature is originated from this account.
	pub type AddressIssuer<T: Config> = StorageValue<_, T::Signer, OptionQuery>;

	#[pallet::storage]
	#[pallet::unbounded]
	#[pallet::getter(fn registration_pool)]
	/// Registered addresses that are permitted to relay Bitcoin.
	pub type RegistrationPool<T: Config> =
		StorageMap<_, Twox64Concat, T::AccountId, BitcoinAddressPair>;

	#[pallet::storage]
	#[pallet::getter(fn bonded_vault)]
	/// Mapped Bitcoin vault addresses. The key is the vault address and the value is the user's Bifrost address.
	pub type BondedVault<T: Config> =
		StorageMap<_, Twox64Concat, BoundedBitcoinAddress, T::AccountId>;

	#[pallet::storage]
	#[pallet::getter(fn bonded_refund)]
	/// Mapped Bitcoin refund addresses. The key is the refund address and the value is the user's Bifrost address.
	pub type BondedRefund<T: Config> =
		StorageMap<_, Twox64Concat, BoundedBitcoinAddress, T::AccountId>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::set_issuer())]
		/// (Re-)set the issuer address.
		pub fn set_issuer(origin: OriginFor<T>, new: T::Signer) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			if let Some(old) = <AddressIssuer<T>>::get() {
				ensure!(old != new, Error::<T>::NoWritingSameValue);
			}
			<AddressIssuer<T>>::put(new);
			Ok(().into())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::register())]
		/// Register the requested addresses to the Bitcoin registration pool.
		pub fn register(
			origin: OriginFor<T>,
			refund_address: Vec<u8>,
			vault_address: Vec<u8>,
			signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			let user_bfc_address = ensure_signed(origin)?;
			let issuer = <AddressIssuer<T>>::get().ok_or(<Error<T>>::IssuerDNE)?;

			let refund_address: BoundedBitcoinAddress =
				refund_address.try_into().map_err(|_| Error::<T>::InvalidBitcoinAddress)?;
			let vault_address: BoundedBitcoinAddress =
				vault_address.try_into().map_err(|_| Error::<T>::InvalidBitcoinAddress)?;

			ensure!(
				!<BondedVault<T>>::contains_key(&vault_address),
				Error::<T>::VaultAddressAlreadyRegistered
			);
			ensure!(
				!<BondedRefund<T>>::contains_key(&refund_address),
				Error::<T>::RefundAddressAlreadyRegistered
			);
			ensure!(
				!<RegistrationPool<T>>::contains_key(&user_bfc_address),
				Error::<T>::UserBfcAddressAlreadyRegistered
			);
			ensure!(refund_address != vault_address, Error::<T>::RefundAndVaultAddressIdentical);

			let message = format!(
				"{:?}:{}:{}",
				user_bfc_address.clone(),
				Self::convert_bitcoin_address_to_string(&refund_address)?,
				Self::convert_bitcoin_address_to_string(&vault_address)?,
			);
			ensure!(
				signature.verify(message.as_bytes(), &issuer.into_account()),
				Error::<T>::InvalidSignature
			);

			<BondedVault<T>>::insert(&vault_address, &user_bfc_address);
			<BondedRefund<T>>::insert(&refund_address, &user_bfc_address);
			<RegistrationPool<T>>::insert(
				user_bfc_address.clone(),
				BitcoinAddressPair::new(refund_address.clone(), vault_address.clone()),
			);

			Self::deposit_event(Event::Registered {
				user_bfc_address,
				refund_address,
				vault_address,
			});

			Ok(().into())
		}
	}
}
