mod impls;

use crate::{CallInfo, ScheduledCall, WeightInfo};

use frame_support::{pallet_prelude::*, traits::StorageVersion};
use frame_system::pallet_prelude::*;
use pallet_ethereum::RawOrigin as EthereumRawOrigin;
use pallet_evm::ExitReason;

use sp_core::{H160, H256, U256};
use sp_runtime::traits::Hash;
use sp_std::{vec, vec::Vec};

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_evm::Config + pallet_ethereum::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		/// The hashing algorithm.
		type Hashing: Hash<Output = H256>;
		/// The default maximum number of failed calls to be banned.
		#[pallet::constant]
		type DefaultMaxBannedCount: Get<u32>;
		/// The default maximum number of scheduled calls.
		#[pallet::constant]
		type DefaultMaxScheduledCalls: Get<u32>;
		/// The default maximum gas limit per call.
		#[pallet::constant]
		type DefaultMaxGasLimitPerCall: Get<u64>;
		/// Weight information for extrinsics.
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The call capacity has been exceeded.
		CallCapacityExceeded,
		/// The call is already scheduled.
		CallAlreadyScheduled,
		/// The caller is not whitelisted.
		NotWhitelisted,
		/// The caller is already whitelisted.
		AlreadyWhitelisted,
		/// The interval is invalid.
		InvalidInterval,
		/// The gas limit is too high.
		InvalidGasLimit,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// The call has been estimated.
		Estimated {
			from: T::AccountId,
			to: T::AccountId,
			value: U256,
			input: Vec<u8>,
			gas_used: U256,
			exit_reason: ExitReason,
		},
		/// The call has been executed.
		Executed {
			from: T::AccountId,
			to: T::AccountId,
			value: U256,
			input: Vec<u8>,
			gas_used: U256,
			exit_reason: ExitReason,
		},
	}

	#[pallet::storage]
	pub type MaxBannedCount<T: Config> = StorageValue<_, u32, ValueQuery>;

	#[pallet::storage]
	pub type MaxScheduledCalls<T: Config> = StorageValue<_, u32, ValueQuery>;

	#[pallet::storage]
	pub type MaxGasLimitPerCall<T: Config> = StorageValue<_, u64, ValueQuery>;

	#[pallet::storage]
	#[pallet::unbounded]
	pub type WhitelistedOwners<T: Config> = StorageValue<_, Vec<T::AccountId>, ValueQuery>;

	#[pallet::storage]
	#[pallet::unbounded]
	pub type ScheduledCalls<T: Config> =
		StorageMap<_, Twox64Concat, H256, ScheduledCall<T::AccountId, BlockNumberFor<T>>>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T>
	where
		T::AccountId: Into<H160>,
		H160: Into<T::AccountId>,
		OriginFor<T>: Into<Result<EthereumRawOrigin, OriginFor<T>>>,
	{
		fn on_initialize(n: BlockNumberFor<T>) -> Weight {
			Self::execute_contract_calls(n)
		}
	}

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		pub whitelist: Vec<T::AccountId>,
		#[serde(skip)]
		pub _config: PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			MaxBannedCount::<T>::put(T::DefaultMaxBannedCount::get());
			MaxScheduledCalls::<T>::put(T::DefaultMaxScheduledCalls::get());
			MaxGasLimitPerCall::<T>::put(T::DefaultMaxGasLimitPerCall::get());
			WhitelistedOwners::<T>::put(self.whitelist.clone());
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T>
	where
		T::AccountId: Into<H160>,
		H160: Into<T::AccountId>,
		OriginFor<T>: Into<Result<EthereumRawOrigin, OriginFor<T>>>,
	{
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn schedule_call(
			origin: OriginFor<T>,
			mut call: CallInfo<T::AccountId>,
		) -> DispatchResultWithPostInfo {
			Self::ensure_whitelisted(origin)?;

			// The call capacity must not be exceeded
			let max_scheduled_calls = MaxScheduledCalls::<T>::get();
			let current_capacity = ScheduledCalls::<T>::iter().count() as u32;
			ensure!(current_capacity < max_scheduled_calls, Error::<T>::CallCapacityExceeded);

			// The interval must be greater than 0
			ensure!(call.interval > 0, Error::<T>::InvalidInterval);

			// The gas limit must not be too high
			let max_gas_limit_per_call = U256::from(MaxGasLimitPerCall::<T>::get());
			ensure!(call.gas <= max_gas_limit_per_call, Error::<T>::InvalidGasLimit);

			// If the gas limit is not set, set it to the maximum allowed value
			if call.gas == U256::zero() {
				call.gas = max_gas_limit_per_call;
			}

			// Hash the scheduled call info
			let call_hash = <T as Config>::Hashing::hash(&call.encode());
			ensure!(
				!ScheduledCalls::<T>::contains_key(&call_hash),
				Error::<T>::CallAlreadyScheduled
			);
			ScheduledCalls::<T>::insert(call_hash, ScheduledCall::new(call));

			Ok(().into())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn add_whitelist(
			origin: OriginFor<T>,
			who: T::AccountId,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;

			let mut whitelist = WhitelistedOwners::<T>::get();
			ensure!(!whitelist.contains(&who), Error::<T>::AlreadyWhitelisted);

			whitelist.push(who);
			WhitelistedOwners::<T>::put(whitelist);

			Ok(().into())
		}

		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn remove_whitelist(
			origin: OriginFor<T>,
			who: T::AccountId,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;

			let mut whitelist = WhitelistedOwners::<T>::get();
			ensure!(whitelist.contains(&who), Error::<T>::NotWhitelisted);

			whitelist.retain(|w| w != &who);
			WhitelistedOwners::<T>::put(whitelist);

			Ok(().into())
		}
	}
}
