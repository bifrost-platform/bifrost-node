mod impls;

use crate::{
	migrations, BitcoinRelayTarget, BoundedBitcoinAddress, MultiSigAccount, PoolRound,
	SetRefundState, SetRefundsApproval, VaultKeyPreSubmission, VaultKeySubmission, WeightInfo,
	ADDRESS_U64,
};

use frame_support::{
	pallet_prelude::*,
	traits::{OnRuntimeUpgrade, SortedMembers, StorageVersion},
};
use frame_system::pallet_prelude::*;

use bp_btc_relay::{
	traits::SocketQueueManager, MigrationSequence, Network, Public, PublicKey, UnboundedBytes,
};
use sp_core::{H160, H256};
use sp_runtime::{
	traits::{Block, Header, IdentifyAccount, Verify},
	Percent,
};
use sp_std::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	fmt::Display,
	vec::Vec,
};

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
		/// The relay executive members.
		type Executives: SortedMembers<Self::AccountId>;
		/// Interface of Bitcoin Socket Queue pallet.
		type SocketQueue: SocketQueueManager<Self::AccountId>;
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
		/// The refund set request does not exist.
		RefundSetDNE,
		/// Some value is out of the permitted range.
		OutOfRange,
		/// Service is under maintenance mode.
		UnderMaintenance,
		/// SocketQueue is not ready for migration.
		SocketQueueNotReady,
		/// Do not control migration sequence in this state.
		DoNotInterceptMigration,
		/// Presubmission does not exist.
		PreSubmissionDNE,
		/// The pool round is outdated.
		PoolRoundOutdated,
		/// Refund set is already requested.
		RefundSetAlreadyRequested,
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
		/// A user's refund address (re-)set has been requested.
		RefundSetRequested {
			who: T::AccountId,
			old: BoundedBitcoinAddress,
			new: BoundedBitcoinAddress,
		},
		/// A user's refund address (re-)set has been approved.
		RefundSetApproved {
			who: T::AccountId,
			old: BoundedBitcoinAddress,
			new: BoundedBitcoinAddress,
		},
		/// Round's infos dropped.
		RoundDropped(PoolRound),
		/// The migration sequence has started. Waiting for prepare next system vault.
		MigrationStarted,
		/// The migration has been completed.
		MigrationCompleted,
		/// A new multi-sig ratio has been set.
		MultiSigRatioSet { old: Percent, new: Percent },
		/// Vault key has been submitted.
		VaultKeySubmitted { who: T::AccountId, pub_key: Public },
		/// Vault key has been pre-submitted.
		VaultKeyPresubmitted { authority_id: T::AccountId, len: u32 },
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

	#[pallet::storage]
	#[pallet::unbounded]
	#[pallet::getter(fn presubmitted_pubkeys)]
	/// The public keys that are pre-submitted by the relay executives.
	pub type PreSubmittedPubKeys<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		PoolRound,
		Twox64Concat,
		T::AccountId,
		BTreeSet<Public>,
		ValueQuery,
	>;

	#[pallet::storage]
	#[pallet::getter(fn max_presubmission)]
	/// The maximum number of pre-submitted public keys.
	pub type MaxPreSubmission<T: Config> = StorageValue<_, u32, ValueQuery>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// The latest transaction(s) information used for the ongoing vault migration protocol.
	pub type OngoingVaultMigration<T: Config> = StorageValue<_, BTreeMap<H256, bool>, ValueQuery>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// The pending refund sets.
	/// The key is the pool round and user address, and the value is the (current, pending) refund addresses.
	pub type PendingSetRefunds<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		PoolRound,
		Twox64Concat,
		T::AccountId,
		SetRefundState,
		OptionQuery,
	>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_runtime_upgrade() -> Weight {
			migrations::init_v1::InitV1::<T>::on_runtime_upgrade()
		}
	}

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
			MaxPreSubmission::<T>::put(100);
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T>
	where
		H160: Into<T::AccountId>,
	{
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		/// Request to (re-)set the user's refund address.
		pub fn request_set_refund(
			origin: OriginFor<T>,
			new: UnboundedBytes,
		) -> DispatchResultWithPostInfo {
			ensure!(
				Self::service_state() == MigrationSequence::Normal,
				Error::<T>::UnderMaintenance
			);

			let who = ensure_signed(origin)?;
			let new: BoundedBitcoinAddress = Self::get_checked_bitcoin_address(&new)?;
			let current_round = Self::current_round();

			let relay_target =
				<RegistrationPool<T>>::get(current_round, &who).ok_or(Error::<T>::UserDNE)?;
			let old = relay_target.refund_address.clone();
			ensure!(old != new, Error::<T>::NoWritingSameValue);

			ensure!(
				!<BondedVault<T>>::contains_key(current_round, &new),
				Error::<T>::AddressAlreadyRegistered
			);
			ensure!(
				<PendingSetRefunds<T>>::get(current_round, &who).is_none(),
				Error::<T>::RefundSetAlreadyRequested
			);

			<PendingSetRefunds<T>>::insert(
				current_round,
				who.clone(),
				SetRefundState { old: old.clone(), new: new.clone() },
			);
			Self::deposit_event(Event::RefundSetRequested { who, old, new });

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

			let mut relay_target =
				BitcoinRelayTarget::new::<T>(refund_address.clone(), Self::get_m(), Self::get_n());

			let executives = T::Executives::sorted_members();
			for executive in executives {
				if let Some(pub_key) =
					<PreSubmittedPubKeys<T>>::mutate(current_round, &executive, |keys| {
						keys.pop_first()
					}) {
					if <BondedPubKey<T>>::get(current_round, &pub_key).is_none() {
						relay_target
							.vault
							.pub_keys
							.try_insert(executive, pub_key)
							.map_err(|_| <Error<T>>::OutOfRange)?;
					}
				}
			}

			if relay_target.vault.is_key_generation_ready() {
				match Self::try_bond_vault_address(
					&mut relay_target.vault,
					&relay_target.refund_address,
					who.clone(),
					current_round,
				) {
					Ok(_) => {
						for pub_key in relay_target.vault.pub_keys() {
							<BondedPubKey<T>>::insert(current_round, &pub_key, who.clone());
						}
					},
					Err(_) => {
						relay_target.vault.clear_pub_keys();
						Self::deposit_event(Event::VaultPending {
							who: who.clone(),
							refund_address: refund_address.clone(),
						});
					},
				}
			}
			<BondedRefund<T>>::mutate(current_round, &refund_address, |users| {
				users.push(who.clone());
			});
			<RegistrationPool<T>>::insert(current_round, who.clone(), relay_target);

			Ok(().into())
		}

		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		/// Request a system vault address. Initially, the vault address will be in pending state.
		pub fn request_system_vault(
			origin: OriginFor<T>,
			migration_prepare: bool,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin.clone())?;

			ensure!(
				matches!(
					Self::service_state(),
					MigrationSequence::Normal | MigrationSequence::PrepareNextSystemVault
				),
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

			let VaultKeySubmission { authority_id, who, pub_key, pool_round } = key_submission;

			let current_round = Self::current_round();
			ensure!(current_round == pool_round, Error::<T>::PoolRoundOutdated);

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

			Self::deposit_event(Event::VaultKeySubmitted {
				who: who.clone(),
				pub_key: pub_key.clone(),
			});

			if relay_target.vault.is_key_generation_ready() {
				Self::try_bond_vault_address(
					&mut relay_target.vault,
					&relay_target.refund_address,
					who.clone(),
					current_round,
				)?;
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
				MigrationSequence::SetExecutiveMembers | MigrationSequence::UTXOTransfer => {
					return Err(Error::<T>::UnderMaintenance)?;
				},
			}

			let VaultKeySubmission { authority_id, who, pub_key, pool_round } = key_submission;
			ensure!(target_round == pool_round, Error::<T>::PoolRoundOutdated);

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

				Self::deposit_event(Event::VaultKeySubmitted {
					who: who.clone(),
					pub_key: pub_key.clone(),
				});

				if system_vault.is_key_generation_ready() {
					Self::try_bond_vault_address(
						&mut system_vault,
						&Default::default(),
						precompile.clone(),
						target_round,
					)?;

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
		/// Submit public keys for prepare for the fast registration.
		pub fn vault_key_presubmission(
			origin: OriginFor<T>,
			key_submission: VaultKeyPreSubmission<T::AccountId>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			ensure!(
				Self::service_state() == MigrationSequence::Normal,
				Error::<T>::UnderMaintenance
			);

			let VaultKeyPreSubmission { authority_id, pub_keys, pool_round } = key_submission;

			let current_round = Self::current_round();
			ensure!(current_round == pool_round, Error::<T>::PoolRoundOutdated);

			// validate public keys
			for pub_key in &pub_keys {
				ensure!(
					PublicKey::from_slice(pub_key.as_ref()).is_ok(),
					Error::<T>::InvalidPublicKey
				);
			}

			let mut presubmitted = Self::presubmitted_pubkeys(current_round, &authority_id);
			ensure!(
				presubmitted.len() + pub_keys.len() <= Self::max_presubmission() as usize,
				Error::<T>::OutOfRange
			);

			// check if the public keys are already submitted
			ensure!(
				!pub_keys.iter().any(|x| presubmitted.contains(x)),
				Error::<T>::AuthorityAlreadySubmittedPubKey
			);

			// insert the public keys
			presubmitted.extend(pub_keys.clone());

			// update the storage
			<PreSubmittedPubKeys<T>>::insert(current_round, &authority_id, presubmitted);

			Self::deposit_event(Event::VaultKeyPresubmitted {
				authority_id,
				len: pub_keys.len() as u32,
			});

			Ok(().into())
		}

		#[pallet::call_index(6)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		/// Clear a vault and all its related data.
		pub fn clear_vault(
			origin: OriginFor<T>,
			vault_address: UnboundedBytes,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin.clone())?;

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

		#[pallet::call_index(7)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		/// Initiates and control the current state of the vault migration.
		/// Every specific calls will be blocked (except submitting a public key for the next system vault)
		/// until the migration successfully ends.
		///
		/// # Sequence Order
		/// * `Normal` → `SetExecutiveMembers` → `PrepareNextSystemVault` → `UTXOTransfer` → `Normal`
		///
		/// # Note
		/// The migration control is only available when the service is in `Normal` state.
		pub fn migration_control(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			ensure_root(origin.clone())?;

			match Self::service_state() {
				MigrationSequence::Normal => {
					ensure!(
						T::SocketQueue::is_ready_for_migrate(),
						Error::<T>::SocketQueueNotReady
					);
					Self::deposit_event(Event::MigrationStarted);
					<ServiceState<T>>::put(MigrationSequence::SetExecutiveMembers);
				},
				MigrationSequence::SetExecutiveMembers => {
					<ServiceState<T>>::put(MigrationSequence::PrepareNextSystemVault);
					Self::request_system_vault(origin, true)?;
				},
				MigrationSequence::PrepareNextSystemVault => {
					return Err(<Error<T>>::DoNotInterceptMigration)?;
				},
				MigrationSequence::UTXOTransfer => {
					// only permit when the latest migration transaction(s) has been broadcasted
					let state = <OngoingVaultMigration<T>>::get();
					if state.is_empty()
						|| !state
							.values()
							.cloned()
							.collect::<Vec<bool>>()
							.iter()
							.all(|is_executed| *is_executed)
					{
						return Err(<Error<T>>::DoNotInterceptMigration)?;
					}

					Self::deposit_event(Event::MigrationCompleted);
					<CurrentRound<T>>::mutate(|r| *r += 1);
					<ServiceState<T>>::put(MigrationSequence::Normal);
					<OngoingVaultMigration<T>>::put::<BTreeMap<H256, bool>>(Default::default());
				},
			}

			Ok(().into())
		}

		#[allow(unused_must_use)]
		#[pallet::call_index(8)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		/// Drop a previous round and all its related data.
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

			const REMOVE_LIMIT: u32 = u32::MAX;
			<RegistrationPool<T>>::clear_prefix(round, REMOVE_LIMIT, None);
			<BondedVault<T>>::clear_prefix(round, REMOVE_LIMIT, None);
			<BondedRefund<T>>::clear_prefix(round, REMOVE_LIMIT, None);
			<BondedPubKey<T>>::clear_prefix(round, REMOVE_LIMIT, None);
			<BondedDescriptor<T>>::clear_prefix(round, REMOVE_LIMIT, None);
			<PreSubmittedPubKeys<T>>::clear_prefix(round, REMOVE_LIMIT, None);

			Self::deposit_event(Event::RoundDropped(round));

			Ok(().into())
		}

		#[pallet::call_index(9)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		/// Set the maximum number of public keys that can be presubmitted.
		pub fn set_max_presubmission(origin: OriginFor<T>, max: u32) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;

			ensure!(max > 0, Error::<T>::OutOfRange);

			<MaxPreSubmission<T>>::put(max);

			Ok(().into())
		}

		#[pallet::call_index(10)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		/// Set the ratio of the multi-signature threshold.
		pub fn set_multi_sig_ratio(
			origin: OriginFor<T>,
			new: Percent,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;

			// we only permit ratio that is higher than 50%
			ensure!(new >= Percent::from_percent(50), Error::<T>::OutOfRange);

			let old = Self::m_n_ratio();
			ensure!(new != old, Error::<T>::NoWritingSameValue);

			<MultiSigRatio<T>>::set(new);
			Self::deposit_event(Event::MultiSigRatioSet { old, new });

			Ok(().into())
		}

		#[pallet::call_index(11)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		/// Approve the given pending set refund requests.
		pub fn approve_set_refunds(
			origin: OriginFor<T>,
			approval: SetRefundsApproval<T::AccountId, BlockNumberFor<T>>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			ensure!(
				Self::service_state() == MigrationSequence::Normal,
				Error::<T>::UnderMaintenance
			);

			let SetRefundsApproval { refund_sets, pool_round, .. } = approval;

			let current_round = Self::current_round();
			ensure!(current_round == pool_round, Error::<T>::PoolRoundOutdated);

			for refund_set in &refund_sets {
				let who = refund_set.0.clone();
				let pending = <PendingSetRefunds<T>>::get(current_round, &who)
					.ok_or(Error::<T>::RefundSetDNE)?;
				ensure!(pending.new == refund_set.1, Error::<T>::RefundSetDNE);

				// check if the new refund address is already bonded as a vault
				// if it is, then we just remove the pending refund set and do nothing
				if !<BondedVault<T>>::contains_key(current_round, &pending.new) {
					let mut relay_target = <RegistrationPool<T>>::get(current_round, &who)
						.ok_or(Error::<T>::UserDNE)?;
					// remove from previous bond
					let old = relay_target.refund_address.clone();
					<BondedRefund<T>>::mutate(current_round, &old, |users| {
						users.retain(|u| *u != who);
					});
					// add to new bond
					<BondedRefund<T>>::mutate(current_round, &pending.new, |users| {
						users.push(who.clone());
					});

					relay_target.set_refund_address(pending.new.clone());
					<RegistrationPool<T>>::insert(current_round, &who, relay_target);

					Self::deposit_event(Event::RefundSetApproved {
						who: who.clone(),
						old,
						new: pending.new,
					});
				}
				<PendingSetRefunds<T>>::remove(current_round, &who);
			}

			Ok(().into())
		}
	}

	#[pallet::validate_unsigned]
	impl<T: Config> ValidateUnsigned for Pallet<T>
	where
		H160: Into<T::AccountId>,
		<T as frame_system::Config>::AccountId: AsRef<[u8]>,
		<<<T as frame_system::Config>::Block as Block>::Header as Header>::Number: Display,
	{
		type Call = Call<T>;

		fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
			match call {
				Call::submit_vault_key { key_submission, signature } => {
					Self::verify_key_submission(key_submission, signature, "RegPoolKeySubmission")
				},
				Call::submit_system_vault_key { key_submission, signature } => {
					Self::verify_key_submission(key_submission, signature, "SystemKeySubmission")
				},
				Call::vault_key_presubmission { key_submission, signature } => {
					Self::verify_key_presubmission(key_submission, signature)
				},
				Call::approve_set_refunds { approval, signature } => {
					Self::verify_set_refunds_approval(approval, signature)
				},
				_ => InvalidTransaction::Call.into(),
			}
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
					Self::verify_key_submission(key_submission, signature, "RegPoolKeySubmission")
				},
				Call::submit_system_vault_key { key_submission, signature } => {
					Self::verify_key_submission(key_submission, signature, "SystemKeySubmission")
				},
				Call::vault_key_presubmission { key_submission, signature } => {
					Self::verify_key_presubmission(key_submission, signature)
				},
				_ => InvalidTransaction::Call.into(),
			}
		}
	}
}
