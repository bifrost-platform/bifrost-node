mod impls;

use crate::{
	BitcoinRelayTarget, BoundedBitcoinAddress, KeySubmission, MultiSigAccount, WeightInfo,
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
		/// This address is already registered or used.
		AddressAlreadyRegistered,
		/// The vault address has already been generated.
		VaultAlreadyGenerated,
		/// The vault address already contains the given public key.
		VaultAlreadyContainsPubKey,
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
	#[pallet::getter(fn system_vault)]
	/// The system vault account that is used for fee refunds.
	pub type SystemVault<T: Config> = StorageValue<_, MultiSigAccount<T::AccountId>, OptionQuery>;

	#[pallet::storage]
	#[pallet::unbounded]
	#[pallet::getter(fn registration_pool)]
	/// Registered addresses that are permitted to relay Bitcoin.
	pub type RegistrationPool<T: Config> =
		StorageMap<_, Twox64Concat, T::AccountId, BitcoinRelayTarget<T::AccountId>>;

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

			if let Some(system_vault) = <SystemVault<T>>::get() {
				ensure!(
					!system_vault.is_address(&refund_address),
					Error::<T>::AddressAlreadyRegistered
				);
			}
			ensure!(
				!<BondedRefund<T>>::contains_key(&refund_address),
				Error::<T>::AddressAlreadyRegistered
			);
			ensure!(
				!<BondedVault<T>>::contains_key(&refund_address),
				Error::<T>::AddressAlreadyRegistered
			);
			ensure!(
				!<RegistrationPool<T>>::contains_key(&who),
				Error::<T>::AddressAlreadyRegistered
			);

			<BondedRefund<T>>::insert(&refund_address, &who);
			<RegistrationPool<T>>::insert(
				who.clone(),
				BitcoinRelayTarget::new::<T>(refund_address.clone()),
			);

			Self::deposit_event(Event::VaultPending { who, refund_address });

			Ok(().into())
		}

		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::request_system_vault())]
		/// Request a system vault address. Initially, the vault address will be in pending state.
		pub fn request_system_vault(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			T::SetOrigin::ensure_origin(origin)?;

			Ok(().into())
		}

		#[pallet::call_index(3)]
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
			let mut relay_target = <RegistrationPool<T>>::get(&who).ok_or(Error::<T>::UserDNE)?;

			ensure!(relay_target.vault.is_pending(), Error::<T>::VaultAlreadyGenerated);
			ensure!(
				!relay_target.vault.is_authority_submitted(&authority_id),
				Error::<T>::AuthorityAlreadySubmittedPubKey
			);
			ensure!(
				!relay_target.vault.is_key_submitted(&pub_key),
				Error::<T>::VaultAlreadyContainsPubKey
			);
			ensure!(PublicKey::from_slice(pub_key.as_ref()).is_ok(), Error::<T>::InvalidPublicKey);

			relay_target.vault.insert_pub_key::<T>(authority_id, pub_key)?;

			if relay_target.vault.is_generation_ready::<T>() {
				// generate vault address
				let vault_address = Self::generate_vault_address(
					relay_target.vault.pub_keys.values().cloned().collect(),
				)?;
				relay_target.set_vault_address(vault_address.clone());

				<BondedVault<T>>::insert(&vault_address, who.clone());
				Self::deposit_event(Event::VaultGenerated {
					who: who.clone(),
					refund_address: relay_target.refund_address.clone(),
					vault_address,
				});
			}

			<RegistrationPool<T>>::insert(&who, relay_target);

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
