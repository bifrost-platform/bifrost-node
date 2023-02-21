mod impls;
pub use impls::*;

use crate::{
	IdentificationTuple, Relayer, RelayerMetadata, Releases, UnresponsivenessOffence, WeightInfo,
};

use frame_support::{pallet_prelude::*, traits::ValidatorSetWithIdentification, Twox64Concat};
use frame_system::pallet_prelude::*;

use bp_staking::{RoundIndex, MAX_AUTHORITIES};
use sp_runtime::Perbill;
use sp_staking::{offence::ReportOffence, SessionIndex};
use sp_std::prelude::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	/// Pallet for relay manager
	#[pallet::pallet]
	#[pallet::generate_store(pub(crate) trait Store)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(PhantomData<T>);

	/// Configuration trait of this pallet
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Overarching event type
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
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
		/// Cannot set the value as identical to the previous value
		NoWritingSameValue,
		/// Cannot set the value below one
		CannotSetBelowOne,
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
	}

	#[pallet::storage]
	/// Storage version of the pallet.
	pub(crate) type StorageVersion<T: Config> = StorageValue<_, Releases, ValueQuery>;

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
	/// The active relayer set selected for the current round
	pub type SelectedRelayers<T: Config> =
		StorageValue<_, BoundedVec<T::AccountId, ConstU32<MAX_AUTHORITIES>>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn initial_selected_relayers)]
	/// The active relayer set selected at the beginning of the current round
	pub type InitialSelectedRelayers<T: Config> =
		StorageValue<_, BoundedVec<T::AccountId, ConstU32<MAX_AUTHORITIES>>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn cached_selected_relayers)]
	/// The cached active relayer set selected from previous rounds
	pub type CachedSelectedRelayers<T: Config> =
		StorageValue<_, Vec<(RoundIndex, Vec<T::AccountId>)>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn cached_initial_selected_relayers)]
	/// The cached active relayer set selected from the beginning of each previous rounds
	pub type CachedInitialSelectedRelayers<T: Config> =
		StorageValue<_, Vec<(RoundIndex, Vec<T::AccountId>)>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn majority)]
	/// The majority of the current active relayer set
	pub type Majority<T: Config> = StorageValue<_, u32, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn initial_majority)]
	/// The majority of the current active relayer set at the beginning of the current round
	pub type InitialMajority<T: Config> = StorageValue<_, u32, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn cached_majority)]
	/// The cached majority based on the active relayer set selected from previous rounds
	pub type CachedMajority<T: Config> = StorageValue<_, Vec<(RoundIndex, u32)>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn cached_initial_majority)]
	/// The cached majority based on the active relayer set selected from the beginning of each
	/// previous rounds
	pub type CachedInitialMajority<T: Config> = StorageValue<_, Vec<(RoundIndex, u32)>, ValueQuery>;

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

	#[pallet::genesis_config]
	pub struct GenesisConfig {}

	#[cfg(feature = "std")]
	impl Default for GenesisConfig {
		fn default() -> Self {
			Self {}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig {
		fn build(&self) {
			StorageVersion::<T>::put(Releases::V3_0_0);
			StorageCacheLifetime::<T>::put(T::StorageCacheLifetimeInRounds::get());
			IsHeartbeatOffenceActive::<T>::put(T::IsHeartbeatOffenceActive::get());
			HeartbeatSlashFraction::<T>::put(T::DefaultHeartbeatSlashFraction::get());
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
			frame_system::ensure_root(origin)?;
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
			frame_system::ensure_root(origin)?;
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
			frame_system::ensure_root(origin)?;
			let old = <HeartbeatSlashFraction<T>>::get();
			ensure!(old != new, Error::<T>::NoWritingSameValue);
			<HeartbeatSlashFraction<T>>::put(new);
			Self::deposit_event(Event::HeartbeatSlashFractionSet { old, new });
			Ok(().into())
		}

		#[pallet::call_index(3)]
		#[pallet::weight(<T as Config>::WeightInfo::set_relayer())]
		/// (Re-)set the bonded relayer account. The origin must be the bonded controller account.
		/// The state reflection will be immediately applied.
		pub fn set_relayer(origin: OriginFor<T>, new: T::AccountId) -> DispatchResultWithPostInfo {
			let controller = ensure_signed(origin)?;
			let old = Self::bonded_controller(&controller).ok_or(Error::<T>::ControllerDNE)?;
			ensure!(old != new, Error::<T>::NoWritingSameValue);
			ensure!(Self::is_relayer(&old), Error::<T>::RelayerDNE);
			ensure!(!Self::is_relayer(&new), Error::<T>::RelayerAlreadyJoined);
			ensure!(Self::replace_bonded_relayer(&old, &new), Error::<T>::RelayerDNE);
			Self::deposit_event(Event::RelayerSet { old, new });
			Ok(().into())
		}

		#[pallet::call_index(4)]
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

		#[pallet::call_index(5)]
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
