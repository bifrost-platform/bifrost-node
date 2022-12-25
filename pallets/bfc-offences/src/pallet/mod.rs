mod impls;
pub use impls::*;

use crate::{
	BalanceOf, NegativeImbalanceOf, OffenceCount, Releases, ValidatorOffenceInfo, WeightInfo,
};

use bp_staking::TierType;
use frame_support::{
	pallet_prelude::*,
	traits::{Currency, OnUnbalanced, ReservableCurrency},
};
use frame_system::pallet_prelude::*;
use sp_staking::SessionIndex;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	/// Pallet for bfc offences
	#[pallet::pallet]
	#[pallet::generate_store(pub(crate) trait Store)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	/// Configuration trait of this pallet
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Overarching event type
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
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
	/// Storage version of the pallet
	pub(crate) type StorageVersion<T: Config> = StorageValue<_, Releases, ValueQuery>;

	#[pallet::storage]
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
			StorageVersion::<T>::put(Releases::V1_0_0);
			OffenceExpirationInSessions::<T>::put(T::DefaultOffenceExpirationInSessions::get());
			FullMaximumOffenceCount::<T>::put(T::DefaultFullMaximumOffenceCount::get());
			BasicMaximumOffenceCount::<T>::put(T::DefaultBasicMaximumOffenceCount::get());
			IsOffenceActive::<T>::put(T::IsOffenceActive::get());
			IsSlashActive::<T>::put(T::IsSlashActive::get());
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(
			<T as Config>::WeightInfo::set_offence_expiration()
		)]
		/// Set a new offence expiration for all validators. It must be specified in sessions.
		pub fn set_offence_expiration(
			origin: OriginFor<T>,
			new: SessionIndex,
		) -> DispatchResultWithPostInfo {
			frame_system::ensure_root(origin)?;
			ensure!(new > 0u32, Error::<T>::CannotSetBelowMin);
			let old = <OffenceExpirationInSessions<T>>::get();
			ensure!(old != new, Error::<T>::NoWritingSameValue);
			<OffenceExpirationInSessions<T>>::put(new);
			Self::deposit_event(Event::OffenceExpirationSet { old, new });
			Ok(().into())
		}

		#[pallet::weight(
			<T as Config>::WeightInfo::set_max_offence_count()
		)]
		/// Set a new maximum offence count for all validators.
		pub fn set_max_offence_count(
			origin: OriginFor<T>,
			new: OffenceCount,
			tier: TierType,
		) -> DispatchResultWithPostInfo {
			frame_system::ensure_root(origin)?;
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

		#[pallet::weight(
			<T as Config>::WeightInfo::set_offence_activation()
		)]
		/// Set the activation of validator offence management.
		pub fn set_offence_activation(
			origin: OriginFor<T>,
			is_active: bool,
		) -> DispatchResultWithPostInfo {
			frame_system::ensure_root(origin)?;
			ensure!(is_active != <IsOffenceActive<T>>::get(), Error::<T>::NoWritingSameValue);
			<IsOffenceActive<T>>::put(is_active);
			Self::deposit_event(Event::OffenceActivationSet { is_active });
			Ok(().into())
		}

		#[pallet::weight(
			<T as Config>::WeightInfo::set_slash_activation()
		)]
		/// Set the activation of validator slashing.
		pub fn set_slash_activation(
			origin: OriginFor<T>,
			is_active: bool,
		) -> DispatchResultWithPostInfo {
			frame_system::ensure_root(origin)?;
			ensure!(is_active != <IsSlashActive<T>>::get(), Error::<T>::NoWritingSameValue);
			<IsSlashActive<T>>::put(is_active);
			Self::deposit_event(Event::SlashActivationSet { is_active });
			Ok(().into())
		}
	}
}
