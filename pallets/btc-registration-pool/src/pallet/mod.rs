mod impls;

use crate::{
	BitcoinAddressPair, BoundedBitcoinAddress, KeySubmission, MultiSigAddress, VaultAddress,
	WeightInfo,
};

use frame_support::{
	pallet_prelude::*,
	traits::{SortedMembers, StorageVersion},
};
use frame_system::pallet_prelude::*;

use scale_info::prelude::format;
use sp_runtime::traits::{IdentifyAccount, Verify};
use sp_std::{str, vec, vec::Vec};

use miniscript::bitcoin::PublicKey;

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
		/// Required origin for setting or resetting the configuration.
		type SetOrigin: EnsureOrigin<Self::RuntimeOrigin>;
		/// The signature signed by the issuer.
		type Signature: Verify<Signer = Self::Signer> + Encode + Decode + Parameter;
		/// The signer of the message.
		type Signer: IdentifyAccount<AccountId = Self::AccountId>
			+ Encode
			+ Decode
			+ Parameter
			+ MaxEncodedLen;
		/// The relay executive members.
		type Executives: SortedMembers<Self::AccountId>;
		/// The required number of public keys to generate a vault address.
		#[pallet::constant]
		type DefaultRequiredM: Get<u8>;
		/// The required number of signatures to send a transaction with the vault account.
		#[pallet::constant]
		type DefaultRequiredN: Get<u8>;
		/// The flag that represents whether the target Bitcoin network is the mainnet.
		#[pallet::constant]
		type IsBitcoinMainnet: Get<bool>;
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The Bifrost address is already registered.
		UserBfcAddressAlreadyRegistered,
		/// The refund Bitcoin address is already registered.
		RefundAddressAlreadyRegistered,
		/// The vault address has already been generated.
		VaultAddressAlreadyGenerated,
		/// The vault address already contains the given public key.
		VaultAddressAlreadyContainsPubKey,
		/// The authority has already submitted a public key for a vault.
		AuthorityAlreadySubmittedPubKey,
		/// The given bitcoin address is invalid.
		InvalidBitcoinAddress,
		/// The given public key is invalid.
		InvalidPublicKey,
		/// Cannot set the value as identical to the previous value.
		NoWritingSameValue,
		/// The user does not exist.
		UserDNE,
		/// The vault is out of range.
		OutOfRange,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new user registered its credentials and received a pending vault address.
		VaultPending { who: T::AccountId, refund_address: BoundedBitcoinAddress },
		/// A user's vault address has been successfully generated.
		VaultGenerated {
			who: T::AccountId,
			refund_address: BoundedBitcoinAddress,
			vault_address: BoundedBitcoinAddress,
		},
		/// A new vault configuration has been set.
		VaultConfigSet { m: u8, n: u8 },
	}

	#[pallet::storage]
	#[pallet::unbounded]
	#[pallet::getter(fn registration_pool)]
	/// Registered addresses that are permitted to relay Bitcoin.
	pub type RegistrationPool<T: Config> =
		StorageMap<_, Twox64Concat, T::AccountId, BitcoinAddressPair<T::AccountId>>;

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

	#[pallet::storage]
	#[pallet::getter(fn required_m)]
	/// The required number of public keys to generate a vault address.
	pub type RequiredM<T: Config> = StorageValue<_, u8, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn required_n)]
	/// The required number of signatures to send a transaction with the vault account.
	pub type RequiredN<T: Config> = StorageValue<_, u8, ValueQuery>;

	#[pallet::genesis_config]
	pub struct GenesisConfig<T> {
		pub required_m: u8,
		pub required_n: u8,
		#[serde(skip)]
		pub _config: PhantomData<T>,
	}

	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				required_m: T::DefaultRequiredM::get(),
				required_n: T::DefaultRequiredN::get(),
				_config: Default::default(),
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			RequiredM::<T>::put(self.required_m);
			RequiredN::<T>::put(self.required_n);
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::set_vault_config())]
		pub fn set_vault_config(origin: OriginFor<T>, m: u8, n: u8) -> DispatchResultWithPostInfo {
			T::SetOrigin::ensure_origin(origin)?;

			ensure!(n >= 1 && n <= 16, Error::<T>::OutOfRange);
			ensure!(m >= 1 && m <= n, Error::<T>::OutOfRange);

			Self::deposit_event(Event::VaultConfigSet { m, n });

			<RequiredM<T>>::put(m);
			<RequiredN<T>>::put(n);

			Ok(().into())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::request_vault())]
		/// Request a vault address. Initially, the vault address will be in pending state.
		pub fn request_vault(
			origin: OriginFor<T>,
			refund_address: Vec<u8>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let refund_address: BoundedBitcoinAddress =
				Self::get_checked_bitcoin_address(&refund_address)?;

			ensure!(
				!<BondedRefund<T>>::contains_key(&refund_address),
				Error::<T>::RefundAddressAlreadyRegistered
			);
			ensure!(
				!<RegistrationPool<T>>::contains_key(&who),
				Error::<T>::UserBfcAddressAlreadyRegistered
			);

			<BondedRefund<T>>::insert(&refund_address, &who);
			<RegistrationPool<T>>::insert(
				who.clone(),
				BitcoinAddressPair::new(refund_address.clone()),
			);

			Self::deposit_event(Event::VaultPending { who, refund_address });

			Ok(().into())
		}

		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::submit_key())]
		/// Submit a public key for the given target. If the quorum reach, the vault address will be generated.
		pub fn submit_key(
			origin: OriginFor<T>,
			key_submission: KeySubmission<T::AccountId>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			// make sure this cannot be executed by a signed transaction.
			ensure_none(origin)?;

			let KeySubmission { authority_id, who, pub_key } = key_submission;
			let mut registered = <RegistrationPool<T>>::get(&who).ok_or(Error::<T>::UserDNE)?;

			ensure!(registered.is_pending(), Error::<T>::VaultAddressAlreadyGenerated);
			ensure!(
				!registered.pub_keys.contains_key(&authority_id),
				Error::<T>::AuthorityAlreadySubmittedPubKey
			);
			ensure!(
				!registered.is_key_submitted(&pub_key),
				Error::<T>::VaultAddressAlreadyContainsPubKey
			);
			ensure!(PublicKey::from_slice(&pub_key).is_ok(), Error::<T>::InvalidPublicKey);

			registered.insert_pub_key(authority_id, pub_key);

			if registered.is_generation_ready::<T>() {
				// generate vault address
				let vault_address =
					Self::generate_vault_address(registered.pub_keys.values().cloned().collect())?;
				registered.vault_address =
					VaultAddress::Generated(MultiSigAddress::new::<T>(vault_address.clone()));

				<BondedVault<T>>::insert(&vault_address, who.clone());
				Self::deposit_event(Event::VaultGenerated {
					who: who.clone(),
					refund_address: registered.refund_address.clone(),
					vault_address,
				});
			}

			<RegistrationPool<T>>::insert(&who, registered);

			Ok(().into())
		}
	}

	#[pallet::validate_unsigned]
	impl<T: Config> ValidateUnsigned for Pallet<T> {
		type Call = Call<T>;

		fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
			if let Call::submit_key { key_submission, signature } = call {
				let KeySubmission { authority_id, who, pub_key } = key_submission;

				// verify if the authority is a relay executive member.
				if !T::Executives::contains(&authority_id) {
					return InvalidTransaction::BadSigner.into();
				}

				// verify if the signature was originated from the authority.
				let message = format!(
					"{:?}:{:?}:{}",
					authority_id,
					who,
					array_bytes::bytes2hex("0x", pub_key)
				);
				if !signature.verify(message.as_bytes(), authority_id) {
					return InvalidTransaction::BadProof.into();
				}

				ValidTransaction::with_tag_prefix("RegPoolKeySubmission")
					.priority(TransactionPriority::MAX)
					.and_provides((authority_id, who))
					.propagate(true)
					.build()
			} else {
				InvalidTransaction::Call.into()
			}
		}
	}
}
