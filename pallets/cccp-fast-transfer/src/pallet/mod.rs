use crate::{weights::WeightInfo, AssetCapInfo, AssetId, BalanceOf, FastTransfer};

use frame_support::{
	pallet_prelude::*,
	traits::{Currency, ReservableCurrency, StorageVersion},
};
use frame_system::pallet_prelude::*;

use sp_core::U256;
use sp_runtime::traits::{Block, Header, IdentifyAccount, Verify};
use sp_std::{fmt::Display, vec, vec::Vec};

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The currency type
		type Currency: Currency<Self::AccountId> + ReservableCurrency<Self::AccountId>;
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T> {}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {}

	#[pallet::storage]
	#[pallet::unbounded]
	/// Asset caps.
	/// key: The asset address.
	/// value: The asset cap information.
	pub type AssetCaps<T: Config> =
		StorageMap<_, Twox64Concat, AssetId, AssetCapInfo<BalanceOf<T>>>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// Pending fast transfers.
	/// key: The asset address.
	/// key: The sequence ID.
	/// value: The fast transfer information.
	pub type PendingTransfers<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		AssetId,
		Twox64Concat,
		U256,
		FastTransfer<T::AccountId, BalanceOf<T>>,
	>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// On-flight fast transfers.
	/// key: The asset address.
	/// key: The sequence ID.
	/// value: The fast transfer information.
	pub type OnFlightTransfers<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		AssetId,
		Twox64Concat,
		U256,
		FastTransfer<T::AccountId, BalanceOf<T>>,
	>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// Finalized fast transfers.
	/// key: The asset address.
	/// key: The sequence ID.
	/// value: The fast transfer information.
	pub type FinalizedTransfers<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		AssetId,
		Twox64Concat,
		U256,
		FastTransfer<T::AccountId, BalanceOf<T>>,
	>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn fast_transfer_poll(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			// TODO: SocketMessage validation
			// 			1. SocketMessage variants validation
			// 				1.1. if required
			// 			2. Bridge asset must be in AssetCaps
			// 			3. refund address must be Executor's address
			// 			4. SocketMessage bytes must be valid onchain (`get_request` method)
			// 			5. SocketMessage status must be REQUESTED
			Ok(().into())
		}
	}

	#[pallet::validate_unsigned]
	impl<T: Config> ValidateUnsigned for Pallet<T>
	where
		<<<T as frame_system::Config>::Block as Block>::Header as Header>::Number: Display,
	{
		type Call = Call<T>;

		fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
			match call {
				_ => InvalidTransaction::Call.into(),
			}
		}
	}
}
