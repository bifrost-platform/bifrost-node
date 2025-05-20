mod impls;

use crate::WeightInfo;

use frame_support::{pallet_prelude::*, traits::StorageVersion};
use frame_system::pallet_prelude::*;

use sp_core::H160;
use sp_std::fmt::Debug;

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
		/// Weight information for extrinsics
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The scheduled call already exists
		CallAlreadyExists,
		/// The scheduled call does not exist
		CallDoesNotExist,
		/// The interval is too short
		IntervalTooShort,
		/// Too many scheduled calls
		TooManyScheduledCalls,
		/// The contract call failed
		ContractCallFailed,
		EstimationFailed,
		/// The contract call reverted
		ContractCallReverted,
		/// The contract call ran out of gas
		ContractCallOutOfGas,
		/// The contract call failed due to invalid input
		ContractCallInvalidInput,
		InvalidHex,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		Done,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T>
	where
		<<T as pallet_evm::Config>::Runner as pallet_evm::Runner<T>>::Error: Debug,
		Result<pallet_ethereum::RawOrigin, <T as frame_system::Config>::RuntimeOrigin>:
			From<<T as frame_system::Config>::RuntimeOrigin>,
		<T as frame_system::Config>::RuntimeOrigin: From<pallet_ethereum::RawOrigin>,
		<T as frame_system::Config>::AccountId: From<H160>,
	{
		fn on_initialize(n: BlockNumberFor<T>) -> Weight {
			if let Err(e) = Self::execute_contract_call(n) {
				// Log the error but don't fail the block
				log::error!("Failed to execute contract call: {:?}", e);
			}
			Weight::from_parts(0, 0)
		}
	}
}
