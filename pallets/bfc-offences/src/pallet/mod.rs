mod impls;

use crate::{
	migrations, BalanceOf, NegativeImbalanceOf, OffenceCount, ValidatorOffenceInfo, WeightInfo,
};

use bp_staking::TierType;
use frame_support::{
	pallet_prelude::*,
	traits::{Currency, OnRuntimeUpgrade, OnUnbalanced, ReservableCurrency, StorageVersion},
};
use frame_system::pallet_prelude::*;
use sp_staking::SessionIndex;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	/// The current storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(3);

	/// Pallet for bfc offences
	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	/// Configuration trait of this pallet
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Overarching event type
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		/// The currency type
		type Currency: Currency<Self::AccountId> + ReservableCurrency<Self::AccountId>;
		/// Handler for the unbalanced reduction when slashing a preimage deposit.
		type Slash: OnUnbalanced<NegativeImbalanceOf<Self>>;
		/// The default offence expiration in sessions
		#[pallet::constant]
		type DefaultOffenceExpirationInSessions: Get<SessionIndex>;
		/// The default maximum offence count for all full nodes
		#[pallet::constant]
		type DefaultFullMaximumOffenceCount: Get<OffenceCount>;
		/// The default maximum offence count for all basic nodes
		#[pallet::constant]
		type DefaultBasicMaximumOffenceCount: Get<OffenceCount>;
		/// The activation of validator offence management
		#[pallet::constant]
		type IsOffenceActive: Get<bool>;
		/// The activation of validator slashing
		#[pallet::constant]
		type IsSlashActive: Get<bool>;
		/// Weight information for extrinsics in this pallet
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Cannot set the value below the minimum value
		CannotSetBelowMin,
		/// Cannot set the value as identical to the previous value
		NoWritingSameValue,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Set the offence expiration
		OffenceExpirationSet { old: SessionIndex, new: SessionIndex },
		/// Set the maximum offence count
		MaximumOffenceCountSet { old: OffenceCount, new: OffenceCount, tier: TierType },
		/// Set the activation of validator offence management
		OffenceActivationSet { is_active: bool },
		/// Set the activation of validator slashing
		SlashActivationSet { is_active: bool },
		/// A validator or nominator has been slashed due to misbehavior
		Slashed { who: T::AccountId, amount: BalanceOf<T> },
	}

	#[pallet::storage]
	#[pallet::unbounded]
	#[pallet::getter(fn validator_offences)]
	/// The current offence state of a specific validator
	pub type ValidatorOffences<T: Config> =
		StorageMap<_, Twox64Concat, T::AccountId, ValidatorOffenceInfo<BalanceOf<T>>, OptionQuery>;

	#[pallet::storage]
	#[pallet::getter(fn offence_expiration_in_sessions)]
	/// The current offence expiration in sessions
	pub type OffenceExpirationInSessions<T: Config> = StorageValue<_, SessionIndex, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn full_maximum_offence_count)]
	/// The current maximum offence count for all full nodes
	pub type FullMaximumOffenceCount<T: Config> = StorageValue<_, OffenceCount, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn basic_maximum_offence_count)]
	/// The current maximum offence count for all basic nodes
	pub type BasicMaximumOffenceCount<T: Config> = StorageValue<_, OffenceCount, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn is_offence_active)]
	/// The current activation of validator offence management
	pub type IsOffenceActive<T: Config> = StorageValue<_, bool, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn is_slash_active)]
	/// The current activation of validator slashing
	pub type IsSlashActive<T: Config> = StorageValue<_, bool, ValueQuery>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_runtime_upgrade() -> Weight {
			migrations::v3::MigrateToV3::<T>::on_runtime_upgrade()
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T> {
		pub default_offence_expiration_in_sessions: SessionIndex,
		pub default_full_maximum_offence_count: OffenceCount,
		pub default_basic_maximum_offence_count: OffenceCount,
		pub is_offence_active: bool,
		pub is_slash_active: bool,
		#[serde(skip)]
		pub _config: PhantomData<T>,
	}

	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				default_offence_expiration_in_sessions: T::DefaultOffenceExpirationInSessions::get(
				),
				default_full_maximum_offence_count: T::DefaultFullMaximumOffenceCount::get(),
				default_basic_maximum_offence_count: T::DefaultBasicMaximumOffenceCount::get(),
				is_offence_active: T::IsOffenceActive::get(),
				is_slash_active: T::IsSlashActive::get(),
				_config: Default::default(),
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			OffenceExpirationInSessions::<T>::put(self.default_offence_expiration_in_sessions);
			FullMaximumOffenceCount::<T>::put(self.default_full_maximum_offence_count);
			BasicMaximumOffenceCount::<T>::put(self.default_basic_maximum_offence_count);
			IsOffenceActive::<T>::put(self.is_offence_active);
			IsSlashActive::<T>::put(self.is_slash_active);
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(
			<T as Config>::WeightInfo::set_offence_expiration()
		)]
		/// Set a new offence expiration for all validators. It must be specified in sessions.
		pub fn set_offence_expiration(
			origin: OriginFor<T>,
			new: SessionIndex,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			ensure!(new > 0u32, Error::<T>::CannotSetBelowMin);
			let old = <OffenceExpirationInSessions<T>>::get();
			ensure!(old != new, Error::<T>::NoWritingSameValue);
			<OffenceExpirationInSessions<T>>::put(new);
			Self::deposit_event(Event::OffenceExpirationSet { old, new });
			Ok(().into())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(
			<T as Config>::WeightInfo::set_max_offence_count()
		)]
		/// Set a new maximum offence count for all validators.
		pub fn set_max_offence_count(
			origin: OriginFor<T>,
			new: OffenceCount,
			tier: TierType,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			ensure!(new > 0u32, Error::<T>::CannotSetBelowMin);
			match tier {
				TierType::Full => {
					let old = <FullMaximumOffenceCount<T>>::get();
					ensure!(old != new, Error::<T>::NoWritingSameValue);
					<FullMaximumOffenceCount<T>>::put(new);
					Self::deposit_event(Event::MaximumOffenceCountSet { old, new, tier });
				},
				TierType::Basic => {
					let old = <BasicMaximumOffenceCount<T>>::get();
					ensure!(old != new, Error::<T>::NoWritingSameValue);
					<BasicMaximumOffenceCount<T>>::put(new);
					Self::deposit_event(Event::MaximumOffenceCountSet { old, new, tier });
				},
				TierType::All => {
					let old_full = <FullMaximumOffenceCount<T>>::get();
					ensure!(old_full != new, Error::<T>::NoWritingSameValue);
					let old_basic = <BasicMaximumOffenceCount<T>>::get();
					ensure!(old_basic != new, Error::<T>::NoWritingSameValue);

					<FullMaximumOffenceCount<T>>::put(new);
					<BasicMaximumOffenceCount<T>>::put(new);

					Self::deposit_event(Event::MaximumOffenceCountSet {
						old: old_full,
						new,
						tier: TierType::Full,
					});
					Self::deposit_event(Event::MaximumOffenceCountSet {
						old: old_basic,
						new,
						tier: TierType::Basic,
					});
				},
			}
			Ok(().into())
		}

		#[pallet::call_index(2)]
		#[pallet::weight(
			<T as Config>::WeightInfo::set_offence_activation()
		)]
		/// Set the activation of validator offence management.
		pub fn set_offence_activation(
			origin: OriginFor<T>,
			is_active: bool,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			ensure!(is_active != <IsOffenceActive<T>>::get(), Error::<T>::NoWritingSameValue);
			<IsOffenceActive<T>>::put(is_active);
			Self::deposit_event(Event::OffenceActivationSet { is_active });
			Ok(().into())
		}

		#[pallet::call_index(3)]
		#[pallet::weight(
			<T as Config>::WeightInfo::set_slash_activation()
		)]
		/// Set the activation of validator slashing.
		pub fn set_slash_activation(
			origin: OriginFor<T>,
			is_active: bool,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			ensure!(is_active != <IsSlashActive<T>>::get(), Error::<T>::NoWritingSameValue);
			<IsSlashActive<T>>::put(is_active);
			Self::deposit_event(Event::SlashActivationSet { is_active });
			Ok(().into())
		}
	}
}
