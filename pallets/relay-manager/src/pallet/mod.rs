mod impls;

use crate::{
	migrations, DelayedRelayerSet, IdentificationTuple, Relayer, RelayerMetadata,
	UnresponsivenessOffence, WeightInfo,
};

use frame_support::{
	pallet_prelude::*,
	traits::{OnRuntimeUpgrade, StorageVersion, ValidatorSetWithIdentification},
	BoundedBTreeSet, Twox64Concat,
};
use frame_system::pallet_prelude::*;

use bp_btc_relay::traits::{PoolManager, SocketQueueManager};
use bp_staking::{RoundIndex, MAX_AUTHORITIES};
use sp_runtime::Perbill;
use sp_staking::{offence::ReportOffence, SessionIndex};
use sp_std::{collections::btree_map::BTreeMap, prelude::*};

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	/// The current storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(4);

	/// Pallet for relay manager
	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(PhantomData<T>);

	/// Configuration trait of this pallet
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Overarching event type
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		/// Interface of Bitcoin Socket Queue pallet.
		type SocketQueue: SocketQueueManager<Self::AccountId>;
		/// Interface of Bitcoin Registration Pool pallet.
		type RegistrationPool: PoolManager<Self::AccountId>;
		/// A type for retrieving the validators supposed to be well-behaved in a session.
		type ValidatorSet: ValidatorSetWithIdentification<Self::AccountId>;
		/// A type that gives us the ability to submit unresponsiveness offence reports.
		type ReportUnresponsiveness: ReportOffence<
			Self::AccountId,
			IdentificationTuple<Self>,
			UnresponsivenessOffence<IdentificationTuple<Self>, Self>,
		>;
		/// The max lifetime in rounds for storage data to be cached
		#[pallet::constant]
		type StorageCacheLifetimeInRounds: Get<u32>;
		/// The activation of relayer heartbeat offence management
		#[pallet::constant]
		type IsHeartbeatOffenceActive: Get<bool>;
		/// The default slash fraction for heartbeat offences
		#[pallet::constant]
		type DefaultHeartbeatSlashFraction: Get<Perbill>;
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The relayer is already joined
		RelayerAlreadyJoined,
		/// The relayer is already bonded
		RelayerAlreadyBonded,
		/// The relayer is inactive
		RelayerInactive,
		/// The relayer does not exist
		RelayerDNE,
		/// The controller does not exist
		ControllerDNE,
		/// A relayer set request does not exist with the target account.
		RelayerSetDNE,
		/// Cannot set the value as identical to the previous value
		NoWritingSameValue,
		/// Cannot set the value below one
		CannotSetBelowOne,
		/// RelayerPool out of bound
		TooManyRelayers,
		/// SelectedRelayers out of bound
		TooManySelectedRelayers,
		/// The given account has already requested a relayer set.
		AlreadyRelayerSetRequested,
		/// DelayedRelayerSets out of bound.
		TooManyDelayedRelayers,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Account joined the set of relayers
		JoinedRelayers { relayer: T::AccountId, controller: T::AccountId },
		/// Active relayer set update
		RelayerChosen { round: u32, relayer: T::AccountId, controller: T::AccountId },
		/// (Re-)set the relayer account
		RelayerSet { old: T::AccountId, new: T::AccountId },
		/// Set the storage cache lifetime.
		StorageCacheLifetimeSet { old: u32, new: u32 },
		/// A new heartbeat was received from relayer.
		HeartbeatReceived { relayer: T::AccountId },
		/// At the end of the session, no offence was committed.
		AllGood,
		/// At the end of the session, at least one relayer was found to be offline.
		SomeOffline { offline: Vec<IdentificationTuple<T>> },
		/// Set the activation of relayer heartbeat offence management
		HeartbeatOffenceActivationSet { is_active: bool },
		/// Set the slash fraction for heartbeat offences
		HeartbeatSlashFractionSet { old: Perbill, new: Perbill },
		/// Cancel the relayer set.
		RelayerSetCancelled { relayer: T::AccountId },
	}

	#[pallet::storage]
	#[pallet::getter(fn storage_cache_lifetime)]
	/// The max storage lifetime for storage data to be cached
	pub type StorageCacheLifetime<T: Config> = StorageValue<_, u32, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn round)]
	/// The current round index
	pub type Round<T: Config> = StorageValue<_, RoundIndex, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn bonded_controller)]
	/// Mapped controller accounts to the relayer account.
	/// key: controller, value: relayer
	pub type BondedController<T: Config> = StorageMap<_, Twox64Concat, T::AccountId, T::AccountId>;

	#[pallet::storage]
	#[pallet::getter(fn relayer_pool)]
	/// The pool of relayers of the current round (including selected and non-selected relayers)
	pub type RelayerPool<T: Config> =
		StorageValue<_, BoundedVec<Relayer<T::AccountId>, ConstU32<MAX_AUTHORITIES>>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn relayer_state)]
	/// The current state of a specific relayer
	pub type RelayerState<T: Config> = StorageMap<
		_,
		Twox64Concat,
		T::AccountId,
		RelayerMetadata<T::AccountId, T::Hash>,
		OptionQuery,
	>;

	#[pallet::storage]
	#[pallet::getter(fn selected_relayers)]
	/// The active relayer set selected for the current round. This storage is sorted by address.
	pub type SelectedRelayers<T: Config> =
		StorageValue<_, BoundedBTreeSet<T::AccountId, ConstU32<MAX_AUTHORITIES>>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn initial_selected_relayers)]
	/// The active relayer set selected at the beginning of the current round. This storage is sorted by address.
	/// This is used to differentiate with kicked out relayers.
	pub type InitialSelectedRelayers<T: Config> =
		StorageValue<_, BoundedBTreeSet<T::AccountId, ConstU32<MAX_AUTHORITIES>>, ValueQuery>;

	#[pallet::storage]
	#[pallet::unbounded]
	#[pallet::getter(fn cached_selected_relayers)]
	/// The cached active relayer set selected from previous rounds. This storage is sorted by address.
	pub type CachedSelectedRelayers<T: Config> = StorageValue<
		_,
		BTreeMap<RoundIndex, BoundedBTreeSet<T::AccountId, ConstU32<MAX_AUTHORITIES>>>,
		ValueQuery,
	>;

	#[pallet::storage]
	#[pallet::unbounded]
	#[pallet::getter(fn cached_initial_selected_relayers)]
	/// The cached active relayer set selected from the beginning of each previous rounds. This storage is sorted by address.
	/// This is used to differentiate with kicked out relayers.
	pub type CachedInitialSelectedRelayers<T: Config> = StorageValue<
		_,
		BTreeMap<RoundIndex, BoundedBTreeSet<T::AccountId, ConstU32<MAX_AUTHORITIES>>>,
		ValueQuery,
	>;

	#[pallet::storage]
	#[pallet::getter(fn majority)]
	/// The majority of the current active relayer set
	pub type Majority<T: Config> = StorageValue<_, u32, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn initial_majority)]
	/// The majority of the current active relayer set at the beginning of the current round.
	/// This is used to differentiate with kicked out relayers.
	pub type InitialMajority<T: Config> = StorageValue<_, u32, ValueQuery>;

	#[pallet::storage]
	#[pallet::unbounded]
	#[pallet::getter(fn cached_majority)]
	/// The cached majority based on the active relayer set selected from previous rounds
	pub type CachedMajority<T: Config> = StorageValue<_, BTreeMap<RoundIndex, u32>, ValueQuery>;

	#[pallet::storage]
	#[pallet::unbounded]
	#[pallet::getter(fn cached_initial_majority)]
	/// The cached majority based on the active relayer set selected from the beginning of each previous rounds.
	/// This is used to differentiate with kicked out relayers.
	pub type CachedInitialMajority<T: Config> =
		StorageValue<_, BTreeMap<RoundIndex, u32>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn received_heartbeats)]
	/// The received heartbeats of a specific relayer in the current session
	pub type ReceivedHeartbeats<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		SessionIndex,
		Twox64Concat,
		T::AccountId,
		bool,
		ValueQuery,
	>;

	#[pallet::storage]
	#[pallet::getter(fn is_heartbeat_offence_active)]
	/// The activation of relayer heartbeat offence management
	pub type IsHeartbeatOffenceActive<T: Config> = StorageValue<_, bool, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn heartbeat_slash_fraction)]
	/// The slash fraction for heartbeat offences
	pub type HeartbeatSlashFraction<T: Config> = StorageValue<_, Perbill, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn delayed_relayer_sets)]
	/// Delayed relayer address update requests
	pub type DelayedRelayerSets<T: Config> = StorageMap<
		_,
		Twox64Concat,
		RoundIndex,
		BoundedVec<DelayedRelayerSet<T::AccountId>, ConstU32<MAX_AUTHORITIES>>,
		ValueQuery,
	>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_runtime_upgrade() -> Weight {
			migrations::v4::MigrateToV4::<T>::on_runtime_upgrade()
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T> {
		pub storage_cache_lifetime: u32,
		pub is_heartbeat_offence_active: bool,
		pub heartbeat_slash_fraction: Perbill,
		#[serde(skip)]
		pub _config: PhantomData<T>,
	}

	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				storage_cache_lifetime: T::StorageCacheLifetimeInRounds::get(),
				is_heartbeat_offence_active: T::IsHeartbeatOffenceActive::get(),
				heartbeat_slash_fraction: T::DefaultHeartbeatSlashFraction::get(),
				_config: Default::default(),
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			StorageCacheLifetime::<T>::put(self.storage_cache_lifetime);
			IsHeartbeatOffenceActive::<T>::put(self.is_heartbeat_offence_active);
			HeartbeatSlashFraction::<T>::put(self.heartbeat_slash_fraction);
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::set_storage_cache_lifetime())]
		/// Set the `StorageCacheLifetime` round length
		pub fn set_storage_cache_lifetime(
			origin: OriginFor<T>,
			new: u32,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			ensure!(new >= 1u32, Error::<T>::CannotSetBelowOne);
			let old = <StorageCacheLifetime<T>>::get();
			ensure!(old != new, Error::<T>::NoWritingSameValue);
			<StorageCacheLifetime<T>>::put(new);
			Self::deposit_event(Event::StorageCacheLifetimeSet { old, new });
			Ok(().into())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::set_heartbeat_offence_activation())]
		/// Set the activation of relayer heartbeat management
		pub fn set_heartbeat_offence_activation(
			origin: OriginFor<T>,
			is_active: bool,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			ensure!(
				is_active != <IsHeartbeatOffenceActive<T>>::get(),
				Error::<T>::NoWritingSameValue
			);
			<IsHeartbeatOffenceActive<T>>::put(is_active);
			Self::deposit_event(Event::HeartbeatOffenceActivationSet { is_active });
			Ok(().into())
		}

		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::set_heartbeat_slash_fraction())]
		/// Set a new slash fraction for heartbeat offences
		pub fn set_heartbeat_slash_fraction(
			origin: OriginFor<T>,
			new: Perbill,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			let old = <HeartbeatSlashFraction<T>>::get();
			ensure!(old != new, Error::<T>::NoWritingSameValue);
			<HeartbeatSlashFraction<T>>::put(new);
			Self::deposit_event(Event::HeartbeatSlashFractionSet { old, new });
			Ok(().into())
		}

		#[pallet::call_index(3)]
		#[pallet::weight(<T as Config>::WeightInfo::set_relayer())]
		/// (Re-)set the bonded relayer account. The origin must be the bonded controller account.
		/// The state reflection will be applied on the next round update.
		/// - origin should be the controller account
		pub fn set_relayer(origin: OriginFor<T>, new: T::AccountId) -> DispatchResultWithPostInfo {
			let controller = ensure_signed(origin)?;
			let old = Self::bonded_controller(&controller).ok_or(Error::<T>::ControllerDNE)?;
			ensure!(old != new, Error::<T>::NoWritingSameValue);
			ensure!(Self::is_relayer(&old), Error::<T>::RelayerDNE);
			ensure!(!Self::is_relayer(&new), Error::<T>::RelayerAlreadyJoined);
			ensure!(
				!Self::is_relayer_set_requested(old.clone()),
				Error::<T>::AlreadyRelayerSetRequested
			);
			Self::add_to_relayer_sets(old.clone(), new.clone())?;
			Self::deposit_event(Event::RelayerSet { old, new });
			Ok(().into())
		}

		#[pallet::call_index(4)]
		#[pallet::weight(<T as Config>::WeightInfo::cancel_relayer_set())]
		/// Cancel the request for (re-)setting the bonded relayer account.
		/// - origin should be the controller account.
		pub fn cancel_relayer_set(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let controller = ensure_signed(origin)?;
			let relayer = Self::bonded_controller(&controller).ok_or(Error::<T>::ControllerDNE)?;
			ensure!(Self::is_relayer_set_requested(controller.clone()), Error::<T>::RelayerSetDNE);
			Self::remove_relayer_set(&relayer)?;
			Self::deposit_event(Event::RelayerSetCancelled { relayer });
			Ok(().into())
		}

		#[pallet::call_index(5)]
		#[pallet::weight(<T as Config>::WeightInfo::heartbeat())]
		/// DEPRECATED, this extrinsic will be removed later on. Please use `heartbeat_v2()`
		/// instead. Sends a new heartbeat to manage relayer liveness for the current session. The
		/// origin must be the registered relayer account, and only the selected relayers can
		/// request.
		pub fn heartbeat(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let relayer = ensure_signed(origin)?;
			ensure!(Self::is_relayer(&relayer), Error::<T>::RelayerDNE);
			ensure!(Self::is_selected_relayer(&relayer, false), Error::<T>::RelayerInactive);
			let is_pulsed = Self::pulse_heartbeat(&relayer);
			if is_pulsed {
				let mut relayer_state =
					<RelayerState<T>>::get(&relayer).expect("RelayerState must exist");
				relayer_state.go_online();
				<RelayerState<T>>::insert(&relayer, relayer_state);
				Self::deposit_event(Event::<T>::HeartbeatReceived { relayer });
			}
			Ok(().into())
		}

		#[pallet::call_index(6)]
		#[pallet::weight(<T as Config>::WeightInfo::heartbeat_v2())]
		/// Sends a new heartbeat to manage relayer liveness for the current session. The origin
		/// must be the registered relayer account, and only the selected relayers can request.
		pub fn heartbeat_v2(
			origin: OriginFor<T>,
			impl_version: u32,
			spec_version: T::Hash,
		) -> DispatchResultWithPostInfo {
			let relayer = ensure_signed(origin)?;
			ensure!(Self::is_relayer(&relayer), Error::<T>::RelayerDNE);
			ensure!(Self::is_selected_relayer(&relayer, false), Error::<T>::RelayerInactive);
			let is_pulsed = Self::pulse_heartbeat(&relayer);
			if is_pulsed {
				let mut relayer_state =
					<RelayerState<T>>::get(&relayer).expect("RelayerState must exist");
				relayer_state.go_online();
				relayer_state.set_impl_version(Some(impl_version));
				relayer_state.set_spec_version(Some(spec_version));
				<RelayerState<T>>::insert(&relayer, relayer_state);
				Self::deposit_event(Event::<T>::HeartbeatReceived { relayer });
			}
			Ok(().into())
		}
	}
}
