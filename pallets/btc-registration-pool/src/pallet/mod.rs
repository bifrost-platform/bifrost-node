mod impls;

use crate::{
	BitcoinRelayTarget, BoundedBitcoinAddress, MultiSigAccount, VaultKeySubmission, WeightInfo,
	ADDRESS_U64,
};

use frame_support::{
	pallet_prelude::*,
	traits::{SortedMembers, StorageVersion},
};
use frame_system::pallet_prelude::*;

use bp_multi_sig::{Network, Public, PublicKey, UnboundedBytes, MULTI_SIG_MAX_ACCOUNTS};
use sp_core::H160;
use sp_runtime::traits::{IdentifyAccount, Verify};
use sp_std::{str, vec};

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
		/// The custom Bitcoin's chain ID for CCCP.
		#[pallet::constant]
		type BitcoinChainId: Get<u32>;
		/// The flag that represents whether the target Bitcoin network is the mainnet.
		type BitcoinNetwork: Get<Network>;
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
		/// Descriptor generation error.
		DescriptorGeneration,
		/// Cannot set the value as identical to the previous value.
		NoWritingSameValue,
		/// The user does not exist.
		UserDNE,
		/// The (system) vault does not exist.
		VaultDNE,
		/// The vault is out of range.
		OutOfRange,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new system vault has been requested.
		SystemVaultPending,
		/// A new user registered its credentials and received a pending vault address.
		VaultPending { who: T::AccountId, refund_address: BoundedBitcoinAddress },
		/// A vault address has been successfully generated.
		VaultGenerated {
			who: T::AccountId,
			refund_address: BoundedBitcoinAddress,
			vault_address: BoundedBitcoinAddress,
		},
		/// A new vault configuration has been set.
		VaultConfigSet { m: u8, n: u8 },
		/// A user's refund address has been (re-)set.
		RefundSet { who: T::AccountId, old: BoundedBitcoinAddress, new: BoundedBitcoinAddress },
	}

	#[pallet::storage]
	#[pallet::unbounded]
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
	/// For system vault, the value will be set to the precompile address.
	pub type BondedVault<T: Config> =
		StorageMap<_, Twox64Concat, BoundedBitcoinAddress, T::AccountId>;

	#[pallet::storage]
	#[pallet::getter(fn bonded_refund)]
	/// Mapped Bitcoin refund addresses. The key is the refund address and the value is the user's Bifrost address.
	pub type BondedRefund<T: Config> =
		StorageMap<_, Twox64Concat, BoundedBitcoinAddress, T::AccountId>;

	#[pallet::storage]
	#[pallet::getter(fn bonded_pub_key)]
	/// Mapped public keys used for vault account generation. The key is the public key and the value is user's Bifrost address.
	/// For system vault, the value will be set to the precompile address.
	pub type BondedPubKey<T: Config> = StorageMap<_, Twox64Concat, Public, T::AccountId>;

	#[pallet::storage]
	#[pallet::unbounded]
	#[pallet::getter(fn bonded_descriptor)]
	/// Mapped descriptors. The key is the vault address and the value is the descriptor.
	pub type BondedDescriptor<T: Config> =
		StorageMap<_, Twox64Concat, BoundedBitcoinAddress, UnboundedBytes>;

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
	impl<T: Config> Pallet<T>
	where
		H160: Into<T::AccountId>,
	{
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::set_vault_config())]
		/// (Re-)set the vault configurations.
		pub fn set_vault_config(origin: OriginFor<T>, m: u8, n: u8) -> DispatchResultWithPostInfo {
			T::SetOrigin::ensure_origin(origin)?;

			ensure!(n >= 1 && u32::from(n) <= MULTI_SIG_MAX_ACCOUNTS, Error::<T>::OutOfRange);
			ensure!(m >= 1 && m <= n, Error::<T>::OutOfRange);

			Self::deposit_event(Event::VaultConfigSet { m, n });

			<RequiredM<T>>::put(m);
			<RequiredN<T>>::put(n);

			Ok(().into())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::set_refund())]
		/// (Re-)set the user's refund address.
		pub fn set_refund(origin: OriginFor<T>, new: UnboundedBytes) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let new: BoundedBitcoinAddress = Self::get_checked_bitcoin_address(&new)?;

			let mut relay_target = <RegistrationPool<T>>::get(&who).ok_or(Error::<T>::UserDNE)?;
			let old = relay_target.refund_address.clone();
			ensure!(old != new, Error::<T>::NoWritingSameValue);

			ensure!(!<BondedRefund<T>>::contains_key(&new), Error::<T>::AddressAlreadyRegistered);
			ensure!(!<BondedVault<T>>::contains_key(&new), Error::<T>::AddressAlreadyRegistered);

			<BondedRefund<T>>::remove(&old);
			<BondedRefund<T>>::insert(&new, who.clone());

			relay_target.set_refund_address(new.clone());
			<RegistrationPool<T>>::insert(&who, relay_target);

			Self::deposit_event(Event::RefundSet { who, old, new });

			Ok(().into())
		}

		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::request_vault())]
		/// Request a vault address. Initially, the vault address will be in pending state.
		pub fn request_vault(
			origin: OriginFor<T>,
			refund_address: UnboundedBytes,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let refund_address: BoundedBitcoinAddress =
				Self::get_checked_bitcoin_address(&refund_address)?;

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

		#[pallet::call_index(3)]
		#[pallet::weight(<T as Config>::WeightInfo::request_system_vault())]
		/// Request a system vault address. Initially, the vault address will be in pending state.
		pub fn request_system_vault(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			T::SetOrigin::ensure_origin(origin)?;

			ensure!(<SystemVault<T>>::get().is_none(), Error::<T>::VaultAlreadyGenerated);

			<SystemVault<T>>::put(MultiSigAccount::new(
				<RequiredM<T>>::get(),
				<RequiredN<T>>::get(),
			));
			Self::deposit_event(Event::SystemVaultPending);

			Ok(().into())
		}

		#[pallet::call_index(4)]
		#[pallet::weight(<T as Config>::WeightInfo::submit_vault_key())]
		/// Submit a public key for the given target. If the quorum reach, the vault address will be generated.
		pub fn submit_vault_key(
			origin: OriginFor<T>,
			key_submission: VaultKeySubmission<T::AccountId>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			// make sure this cannot be executed by a signed transaction.
			ensure_none(origin)?;

			let VaultKeySubmission { authority_id, who, pub_key } = key_submission;
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
			ensure!(
				<BondedPubKey<T>>::get(&pub_key).is_none(),
				Error::<T>::VaultAlreadyContainsPubKey
			);

			relay_target
				.vault
				.pub_keys
				.try_insert(authority_id, pub_key)
				.map_err(|_| Error::<T>::OutOfRange)?;

			if relay_target.vault.is_key_generation_ready() {
				// generate vault address
				let (vault_address, descriptor) =
					Self::generate_vault_address(relay_target.vault.pub_keys())?;
				relay_target.set_vault_address(vault_address.clone());
				relay_target.vault.set_descriptor(descriptor.clone());

				<BondedVault<T>>::insert(&vault_address, who.clone());
				<BondedDescriptor<T>>::insert(&vault_address, descriptor);
				Self::deposit_event(Event::VaultGenerated {
					who: who.clone(),
					refund_address: relay_target.refund_address.clone(),
					vault_address,
				});
			}

			<BondedPubKey<T>>::insert(&pub_key, who.clone());
			<RegistrationPool<T>>::insert(&who, relay_target);

			Ok(().into())
		}

		#[pallet::call_index(5)]
		#[pallet::weight(<T as Config>::WeightInfo::submit_system_vault_key())]
		/// Submit a public key for the system vault. If the quorum reach, the vault address will be generated.
		pub fn submit_system_vault_key(
			origin: OriginFor<T>,
			key_submission: VaultKeySubmission<T::AccountId>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			// make sure this cannot be executed by a signed transaction.
			ensure_none(origin)?;

			let VaultKeySubmission { authority_id, who, pub_key } = key_submission;

			let precompile: T::AccountId = H160::from_low_u64_be(ADDRESS_U64).into();
			ensure!(precompile == who, Error::<T>::VaultDNE);

			if let Some(mut system_vault) = <SystemVault<T>>::get() {
				ensure!(system_vault.is_pending(), Error::<T>::VaultAlreadyGenerated);
				ensure!(
					!system_vault.is_authority_submitted(&authority_id),
					Error::<T>::AuthorityAlreadySubmittedPubKey
				);
				ensure!(
					!system_vault.is_key_submitted(&pub_key),
					Error::<T>::VaultAlreadyContainsPubKey
				);
				ensure!(
					PublicKey::from_slice(pub_key.as_ref()).is_ok(),
					Error::<T>::InvalidPublicKey
				);
				ensure!(
					<BondedPubKey<T>>::get(&pub_key).is_none(),
					Error::<T>::VaultAlreadyContainsPubKey
				);

				system_vault
					.pub_keys
					.try_insert(authority_id, pub_key)
					.map_err(|_| Error::<T>::OutOfRange)?;

				if system_vault.is_key_generation_ready() {
					// generate vault address
					let (vault_address, descriptor) =
						Self::generate_vault_address(system_vault.pub_keys())?;
					system_vault.set_address(vault_address.clone());
					system_vault.set_descriptor(descriptor.clone());

					<BondedVault<T>>::insert(&vault_address, precompile.clone());
					<BondedDescriptor<T>>::insert(&vault_address, descriptor);
					Self::deposit_event(Event::VaultGenerated {
						who: precompile.clone(),
						refund_address: Default::default(),
						vault_address,
					});
				}
				<BondedPubKey<T>>::insert(&pub_key, precompile);
				<SystemVault<T>>::put(system_vault);
			} else {
				return Err(Error::<T>::VaultDNE)?;
			}

			Ok(().into())
		}

		#[pallet::call_index(6)]
		#[pallet::weight(<T as Config>::WeightInfo::clear_vault())]
		pub fn clear_vault(
			origin: OriginFor<T>,
			vault_address: UnboundedBytes,
		) -> DispatchResultWithPostInfo {
			T::SetOrigin::ensure_origin(origin)?;
			let vault_address: BoundedBitcoinAddress =
				Self::get_checked_bitcoin_address(&vault_address)?;

			let bfc_address = <BondedVault<T>>::get(&vault_address).ok_or(Error::<T>::VaultDNE)?;
			let target = <RegistrationPool<T>>::get(&bfc_address).ok_or(Error::<T>::UserDNE)?;

			<BondedVault<T>>::remove(&vault_address);
			<BondedRefund<T>>::remove(&target.refund_address);
			<BondedDescriptor<T>>::remove(&vault_address);

			for pubkey in target.vault.pub_keys() {
				<BondedPubKey<T>>::remove(&pubkey);
			}
			<RegistrationPool<T>>::remove(&bfc_address);

			Ok(().into())
		}
	}

	#[pallet::validate_unsigned]
	impl<T: Config> ValidateUnsigned for Pallet<T>
	where
		H160: Into<T::AccountId>,
	{
		type Call = Call<T>;

		fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
			match call {
				Call::submit_vault_key { key_submission, signature } => {
					let VaultKeySubmission { authority_id, who, pub_key } = key_submission;

					// verify if the authority is a relay executive member.
					if !T::Executives::contains(&authority_id) {
						return InvalidTransaction::BadSigner.into();
					}

					// verify if the signature was originated from the authority.
					let message = array_bytes::bytes2hex("0x", pub_key);
					if !signature.verify(message.as_bytes(), authority_id) {
						return InvalidTransaction::BadProof.into();
					}

					ValidTransaction::with_tag_prefix("RegPoolKeySubmission")
						.priority(TransactionPriority::MAX)
						.and_provides((authority_id, who))
						.propagate(true)
						.build()
				},
				Call::submit_system_vault_key { key_submission, signature } => {
					let VaultKeySubmission { authority_id, who, pub_key } = key_submission;

					// verify if the authority is a relay executive member.
					if !T::Executives::contains(&authority_id) {
						return InvalidTransaction::BadSigner.into();
					}

					// verify if the signature was originated from the authority.
					let message = array_bytes::bytes2hex("0x", pub_key);
					if !signature.verify(message.as_bytes(), authority_id) {
						return InvalidTransaction::BadProof.into();
					}

					ValidTransaction::with_tag_prefix("SystemKeySubmission")
						.priority(TransactionPriority::MAX)
						.and_provides((authority_id, who))
						.propagate(true)
						.build()
				},
				_ => InvalidTransaction::Call.into(),
			}
		}
	}
}
