mod impls;

use crate::{ScheduledCallInfo, WeightInfo};

use frame_support::{pallet_prelude::*, traits::StorageVersion};
use frame_system::pallet_prelude::*;
use pallet_ethereum::RawOrigin as EthereumRawOrigin;

use sp_core::H160;
use sp_std::{vec, vec::Vec};

#[frame_support::pallet]
pub mod pallet {
	use pallet_evm::ExitReason;

	use super::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_evm::Config + pallet_ethereum::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		#[pallet::constant]
		/// The default maximum number of scheduled calls
		type DefaultMaxScheduledCalls: Get<u32>;
		#[pallet::constant]
		/// The default maximum gas limit per call
		type DefaultMaxGasLimitPerCall: Get<u64>;
		/// Weight information for extrinsics
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T> {
		AlreadyBonded,
		CallCapacityExceeded,
		NotWhitelisted,
		AlreadyWhitelisted,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		Executed {
			from: T::AccountId,
			to: T::AccountId,
			value: u64,
			input: Vec<u8>,
			exit_reason: ExitReason,
		},
	}

	#[pallet::storage]
	pub type MaxScheduledCalls<T: Config> = StorageValue<_, u32, ValueQuery>;

	#[pallet::storage]
	pub type MaxGasLimitPerCall<T: Config> = StorageValue<_, u64, ValueQuery>;

	#[pallet::storage]
	#[pallet::unbounded]
	pub type BondedGasPayers<T: Config> = StorageValue<_, Vec<T::AccountId>, ValueQuery>;

	#[pallet::storage]
	#[pallet::unbounded]
	pub type WhitelistedOwners<T: Config> = StorageValue<_, Vec<T::AccountId>, ValueQuery>;

	#[pallet::storage]
	#[pallet::unbounded]
	pub type ScheduledCalls<T: Config> =
		StorageValue<_, Vec<ScheduledCallInfo<T::AccountId>>, ValueQuery>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T>
	where
		T::AccountId: Into<H160>,
		H160: Into<T::AccountId>,
		OriginFor<T>: Into<Result<EthereumRawOrigin, OriginFor<T>>>,
	{
		fn on_initialize(n: BlockNumberFor<T>) -> Weight {
			Self::execute_contract_call(n)
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
			MaxScheduledCalls::<T>::put(T::DefaultMaxScheduledCalls::get());
			MaxGasLimitPerCall::<T>::put(T::DefaultMaxGasLimitPerCall::get());
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
			call: ScheduledCallInfo<T::AccountId>,
		) -> DispatchResultWithPostInfo {
			let _owner = Self::ensure_whitelisted(origin)?;
			let gas_payer = call.from.clone();
			ensure!(!BondedGasPayers::<T>::get().contains(&gas_payer), Error::<T>::AlreadyBonded);

			let max_scheduled_calls = MaxScheduledCalls::<T>::get();
			let scheduled_calls = ScheduledCalls::<T>::get();
			ensure!(
				scheduled_calls.len() as u32 == max_scheduled_calls,
				Error::<T>::CallCapacityExceeded
			);

			let mut bonded_gas_payers = BondedGasPayers::<T>::get();
			bonded_gas_payers.push(gas_payer);
			BondedGasPayers::<T>::put(bonded_gas_payers);

			let mut scheduled_calls = ScheduledCalls::<T>::get();
			scheduled_calls.push(call);
			ScheduledCalls::<T>::put(scheduled_calls);

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
