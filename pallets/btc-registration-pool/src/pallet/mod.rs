mod impls;

use crate::{
	BitcoinRelayTarget, BoundedBitcoinAddress, MultiSigAccount, PoolRound, VaultKeySubmission,
	WeightInfo, ADDRESS_U64,
};

use frame_support::{
	pallet_prelude::*,
	traits::{SortedMembers, StorageVersion},
};
use frame_system::pallet_prelude::*;

use bp_multi_sig::{MigrationSequence, Network, Public, PublicKey, UnboundedBytes};
use sp_core::H160;
use sp_runtime::{
	traits::{IdentifyAccount, Verify},
	Percent,
};
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
		/// The minimum required number of signatures to send a transaction with the vault account. (in percentage)
		#[pallet::constant]
		type DefaultMultiSigRatio: Get<Percent>;
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
		/// Service is under maintenance mode.
		UnderMaintenance,
		/// Do not control migration sequence in this state.
		DoNotInterceptMigration,
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
		/// A user's refund address has been (re-)set.
		RefundSet { who: T::AccountId, old: BoundedBitcoinAddress, new: BoundedBitcoinAddress },
		/// Round's infos dropped.
		RoundDropped(PoolRound),
		/// The migration sequence has started. Waiting for prepare next system vault.
		MigrationStarted,
		/// The migration has been completed.
		MigrationCompleted,
	}

	#[pallet::storage]
	#[pallet::getter(fn current_round)]
	/// The current round of the registration pool.
	pub type CurrentRound<T: Config> = StorageValue<_, PoolRound, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn service_state)]
	/// The migration sequence of the registration pool.
	pub type ServiceState<T: Config> = StorageValue<_, MigrationSequence, ValueQuery>;

	#[pallet::storage]
	#[pallet::unbounded]
	#[pallet::getter(fn system_vault)]
	/// The system vault account that is used for fee refunds.
	pub type SystemVault<T: Config> =
		StorageMap<_, Twox64Concat, PoolRound, MultiSigAccount<T::AccountId>, OptionQuery>;

	#[pallet::storage]
	#[pallet::unbounded]
	#[pallet::getter(fn registration_pool)]
	/// Registered addresses that are permitted to relay Bitcoin.
	pub type RegistrationPool<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		PoolRound,
		Twox64Concat,
		T::AccountId,
		BitcoinRelayTarget<T::AccountId>,
	>;

	#[pallet::storage]
	#[pallet::getter(fn bonded_vault)]
	/// Mapped Bitcoin vault addresses. The key is the vault address and the value is the user's Bifrost address.
	/// For system vault, the value will be set to the precompile address.
	pub type BondedVault<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		PoolRound,
		Twox64Concat,
		BoundedBitcoinAddress,
		T::AccountId,
	>;

	#[pallet::storage]
	#[pallet::unbounded]
	#[pallet::getter(fn bonded_refund)]
	/// Mapped Bitcoin refund addresses. The key is the refund address and the value is the user's Bifrost address(s).
	pub type BondedRefund<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		PoolRound,
		Twox64Concat,
		BoundedBitcoinAddress,
		Vec<T::AccountId>,
		ValueQuery,
	>;

	#[pallet::storage]
	#[pallet::getter(fn bonded_pub_key)]
	/// Mapped public keys used for vault account generation. The key is the public key and the value is user's Bifrost address.
	/// For system vault, the value will be set to the precompile address.
	pub type BondedPubKey<T: Config> =
		StorageDoubleMap<_, Twox64Concat, PoolRound, Twox64Concat, Public, T::AccountId>;

	#[pallet::storage]
	#[pallet::unbounded]
	#[pallet::getter(fn bonded_descriptor)]
	/// Mapped descriptors. The key is the vault address and the value is the descriptor.
	pub type BondedDescriptor<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		PoolRound,
		Twox64Concat,
		BoundedBitcoinAddress,
		UnboundedBytes,
	>;

	#[pallet::storage]
	#[pallet::getter(fn m_n_ratio)]
	/// The minimum required ratio of signatures to unlock the vault account's txo.
	pub type MultiSigRatio<T: Config> = StorageValue<_, Percent, ValueQuery>;

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T> {
		#[serde(skip)]
		pub _config: PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			MultiSigRatio::<T>::put(T::DefaultMultiSigRatio::get());
			CurrentRound::<T>::put(1);
			ServiceState::<T>::put(MigrationSequence::Normal);
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T>
	where
		H160: Into<T::AccountId>,
	{
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		/// (Re-)set the user's refund address.
		pub fn set_refund(origin: OriginFor<T>, new: UnboundedBytes) -> DispatchResultWithPostInfo {
			ensure!(
				Self::service_state() == MigrationSequence::Normal,
				Error::<T>::UnderMaintenance
			);

			let who = ensure_signed(origin)?;
			let new: BoundedBitcoinAddress = Self::get_checked_bitcoin_address(&new)?;
			let current_round = Self::current_round();

			let mut relay_target =
				<RegistrationPool<T>>::get(current_round, &who).ok_or(Error::<T>::UserDNE)?;
			let old = relay_target.refund_address.clone();
			ensure!(old != new, Error::<T>::NoWritingSameValue);

			ensure!(
				!<BondedVault<T>>::contains_key(current_round, &new),
				Error::<T>::AddressAlreadyRegistered
			);

			// remove from previous bond
			<BondedRefund<T>>::mutate(current_round, &old, |users| {
				users.retain(|u| *u != who);
			});
			// add to new bond
			<BondedRefund<T>>::mutate(current_round, &new, |users| {
				users.push(who.clone());
			});

			relay_target.set_refund_address(new.clone());
			<RegistrationPool<T>>::insert(current_round, &who, relay_target);

			Self::deposit_event(Event::RefundSet { who, old, new });

			Ok(().into())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		/// Request a vault address. Initially, the vault address will be in pending state.
		pub fn request_vault(
			origin: OriginFor<T>,
			refund_address: UnboundedBytes,
		) -> DispatchResultWithPostInfo {
			ensure!(
				Self::service_state() == MigrationSequence::Normal,
				Error::<T>::UnderMaintenance
			);

			let who = ensure_signed(origin)?;
			let refund_address: BoundedBitcoinAddress =
				Self::get_checked_bitcoin_address(&refund_address)?;
			let current_round = Self::current_round();

			ensure!(
				!<BondedVault<T>>::contains_key(current_round, &refund_address),
				Error::<T>::AddressAlreadyRegistered
			);
			ensure!(
				!<RegistrationPool<T>>::contains_key(current_round, &who),
				Error::<T>::AddressAlreadyRegistered
			);

			<BondedRefund<T>>::mutate(current_round, &refund_address, |users| {
				users.push(who.clone());
			});
			<RegistrationPool<T>>::insert(
				current_round,
				who.clone(),
				BitcoinRelayTarget::new::<T>(refund_address.clone(), Self::get_m(), Self::get_n()),
			);

			Self::deposit_event(Event::VaultPending { who, refund_address });

			Ok(().into())
		}

		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		/// Request a system vault address. Initially, the vault address will be in pending state.
		pub fn request_system_vault(
			origin: OriginFor<T>,
			migration_prepare: bool,
		) -> DispatchResultWithPostInfo {
			T::SetOrigin::ensure_origin(origin)?;

			ensure!(
				Self::service_state() == MigrationSequence::Normal,
				Error::<T>::UnderMaintenance
			);

			let target_round =
				if migration_prepare { Self::current_round() + 1 } else { Self::current_round() };

			ensure!(
				<SystemVault<T>>::get(target_round).is_none(),
				Error::<T>::VaultAlreadyGenerated
			);

			<SystemVault<T>>::insert(
				target_round,
				MultiSigAccount::new(Self::get_m(), Self::get_n()),
			);
			Self::deposit_event(Event::SystemVaultPending);

			Ok(().into())
		}

		#[pallet::call_index(3)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		/// Submit a public key for the given target. If the quorum reach, the vault address will be generated.
		pub fn submit_vault_key(
			origin: OriginFor<T>,
			key_submission: VaultKeySubmission<T::AccountId>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			// make sure this cannot be executed by a signed transaction.
			ensure_none(origin)?;

			ensure!(
				Self::service_state() == MigrationSequence::Normal,
				Error::<T>::UnderMaintenance
			);

			let current_round = Self::current_round();

			let VaultKeySubmission { authority_id, who, pub_key } = key_submission;
			let mut relay_target =
				<RegistrationPool<T>>::get(current_round, &who).ok_or(Error::<T>::UserDNE)?;

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
				<BondedPubKey<T>>::get(current_round, &pub_key).is_none(),
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

				<BondedVault<T>>::insert(current_round, &vault_address, who.clone());
				<BondedDescriptor<T>>::insert(current_round, &vault_address, descriptor);
				Self::deposit_event(Event::VaultGenerated {
					who: who.clone(),
					refund_address: relay_target.refund_address.clone(),
					vault_address,
				});
			}

			<BondedPubKey<T>>::insert(current_round, &pub_key, who.clone());
			<RegistrationPool<T>>::insert(current_round, &who, relay_target);

			Ok(().into())
		}

		#[pallet::call_index(4)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		/// Submit a public key for the system vault. If the quorum reach, the vault address will be generated.
		pub fn submit_system_vault_key(
			origin: OriginFor<T>,
			key_submission: VaultKeySubmission<T::AccountId>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			// make sure this cannot be executed by a signed transaction.
			ensure_none(origin)?;

			let service_state = Self::service_state();

			let target_round;
			match Self::service_state() {
				MigrationSequence::Normal => {
					target_round = Self::current_round();
				},
				MigrationSequence::PrepareNextSystemVault => {
					target_round = Self::current_round() + 1;
				},
				MigrationSequence::UTXOTransfer => {
					return Err(Error::<T>::UnderMaintenance)?;
				},
			}

			let VaultKeySubmission { authority_id, who, pub_key } = key_submission;

			let precompile: T::AccountId = H160::from_low_u64_be(ADDRESS_U64).into();
			ensure!(precompile == who, Error::<T>::VaultDNE);

			if let Some(mut system_vault) = <SystemVault<T>>::get(target_round) {
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
					<BondedPubKey<T>>::get(target_round, &pub_key).is_none(),
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

					<BondedVault<T>>::insert(target_round, &vault_address, precompile.clone());
					<BondedDescriptor<T>>::insert(target_round, &vault_address, descriptor);
					Self::deposit_event(Event::VaultGenerated {
						who: precompile.clone(),
						refund_address: Default::default(),
						vault_address,
					});

					if service_state == MigrationSequence::PrepareNextSystemVault {
						<ServiceState<T>>::put(MigrationSequence::UTXOTransfer);
					}
				}
				<BondedPubKey<T>>::insert(target_round, &pub_key, precompile);
				<SystemVault<T>>::insert(target_round, system_vault);
			} else {
				return Err(Error::<T>::VaultDNE)?;
			}

			Ok(().into())
		}

		#[pallet::call_index(5)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn clear_vault(
			origin: OriginFor<T>,
			vault_address: UnboundedBytes,
		) -> DispatchResultWithPostInfo {
			T::SetOrigin::ensure_origin(origin)?;

			ensure!(
				Self::service_state() == MigrationSequence::Normal,
				Error::<T>::UnderMaintenance
			);

			let current_round = Self::current_round();
			let vault_address: BoundedBitcoinAddress =
				Self::get_checked_bitcoin_address(&vault_address)?;

			let who =
				<BondedVault<T>>::get(current_round, &vault_address).ok_or(Error::<T>::VaultDNE)?;
			if who == H160::from_low_u64_be(ADDRESS_U64).into() {
				// system vault
				let system_vault =
					<SystemVault<T>>::get(Self::current_round()).ok_or(Error::<T>::VaultDNE)?;
				for pubkey in system_vault.pub_keys() {
					<BondedPubKey<T>>::remove(current_round, &pubkey);
				}
				<SystemVault<T>>::remove(Self::current_round());
			} else {
				// user
				let target =
					Self::registration_pool(current_round, &who).ok_or(Error::<T>::UserDNE)?;
				for pubkey in target.vault.pub_keys() {
					<BondedPubKey<T>>::remove(current_round, &pubkey);
				}
				<RegistrationPool<T>>::remove(current_round, &who);
				<BondedRefund<T>>::remove(current_round, &target.refund_address);
			}

			<BondedVault<T>>::remove(current_round, &vault_address);
			<BondedDescriptor<T>>::remove(current_round, &vault_address);

			Ok(().into())
		}

		#[pallet::call_index(6)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn migration_control(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			ensure_root(origin.clone())?;

			match Self::service_state() {
				MigrationSequence::Normal => {
					Self::deposit_event(Event::MigrationStarted);
					Self::request_system_vault(origin, true)?;
					<ServiceState<T>>::put(MigrationSequence::PrepareNextSystemVault);
				},
				MigrationSequence::PrepareNextSystemVault => {
					return Err(<Error<T>>::DoNotInterceptMigration)?;
				},
				MigrationSequence::UTXOTransfer => {
					Self::deposit_event(Event::MigrationCompleted);
					<CurrentRound<T>>::mutate(|r| *r += 1);
					<ServiceState<T>>::put(MigrationSequence::Normal);
				},
			}

			Ok(().into())
		}

		#[pallet::call_index(7)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		#[allow(deprecated)]
		pub fn drop_previous_round(
			origin: OriginFor<T>,
			round: PoolRound,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;

			ensure!(
				Self::service_state() == MigrationSequence::Normal,
				Error::<T>::UnderMaintenance
			);
			ensure!(round < Self::current_round(), Error::<T>::OutOfRange);

			// remove all data related to the round
			<SystemVault<T>>::remove(round);
			<RegistrationPool<T>>::remove_prefix(round, None);
			<BondedVault<T>>::remove_prefix(round, None);
			<BondedRefund<T>>::remove_prefix(round, None);
			<BondedPubKey<T>>::remove_prefix(round, None);
			<BondedDescriptor<T>>::remove_prefix(round, None);

			Self::deposit_event(Event::RoundDropped(round));

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
