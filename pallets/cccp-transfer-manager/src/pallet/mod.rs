use crate::{
	weights::WeightInfo, AssetCapInfo, AssetId, AssetIndexHash, BalanceOf,
	SocketMessagesSubmission, TransferInfo,
};

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
		/// The signature signed by the issuer.
		type Signature: Verify<Signer = Self::Signer> + Encode + Decode + Parameter;
		/// The signer of the message.
		type Signer: IdentifyAccount<AccountId = Self::AccountId> + Encode + Decode + MaxEncodedLen;
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The submission is empty.
		EmptySubmission,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {}

	#[pallet::storage]
	#[pallet::unbounded]
	/// Asset indexes.
	/// key: The asset index hash (bytes32). A predefined hash in CCCP.
	/// 	- e.g. `BFC_ON_BFC_MAIN`: `0x000000010000000100000bfcffffffffffffffffffffffffffffffffffffffff`
	///
	/// value: The asset address. (Unified Token, Native BFC: `0xffffffffffffffffffffffffffffffffffffffff`)
	pub type AssetIndexes<T: Config> = StorageMap<_, Twox64Concat, AssetIndexHash, AssetId>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// Asset on-flight caps.
	/// key: The asset address. (Unified Token, Native BFC: `0xffffffffffffffffffffffffffffffffffffffff`)
	/// value: The asset on-flight cap information. The permitted amount of the asset that can be fast-transferred.
	pub type AssetCaps<T: Config> =
		StorageMap<_, Twox64Concat, AssetId, AssetCapInfo<BalanceOf<T>>>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// On-flight CCCP transfers.
	/// key: The asset index hash.
	/// key: The sequence ID.
	/// value: The CCCP transfer information.
	pub type OnFlightTransfers<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		AssetIndexHash,
		Twox64Concat,
		U256,
		TransferInfo<BalanceOf<T>, T::AccountId>,
	>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// Finalized CCCP transfers.
	/// key: The asset index hash.
	/// key: The sequence ID.
	/// value: The CCCP transfer information.
	pub type FinalizedTransfers<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		AssetIndexHash,
		Twox64Concat,
		U256,
		TransferInfo<BalanceOf<T>, T::AccountId>,
	>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn poll(
			origin: OriginFor<T>,
			socket_messages_submission: SocketMessagesSubmission<T::AccountId>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			let SocketMessagesSubmission { authority_id, messages } = socket_messages_submission;
			ensure!(!messages.is_empty(), Error::<T>::EmptySubmission);

			// TODO: SocketMessage validation
			// 			1. Bridge asset must be in AssetIndexes & AssetCaps
			// 			2. SocketMessage bytes must be valid onchain (`get_request` method)
			// 			3. SocketMessage status must be REQUESTED
			// 			4. Duplicate check

			// TODO: Determine bridge direction (inbound or outbound)
			//			1. Check if the source chain is non-bifrost chain. (=inbound) -> Voting is required
			//			2. Check if the source chain is bifrost chain. (=outbound) -> Voting is not required. Internal validation is performed.

			// TODO: AssetCap
			// 			1. AssetCap must be sufficient

			// TODO: Voting
			// 			1. If transfer approved, increase AssetCap.on_flight_cap
			Ok(().into())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn finalize(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			// TODO: this function can handle both committed and rollbacked transfers.

			// TODO: move the transfer from OnFlightTransfers to FinalizedTransfers
			// TODO: decrease AssetCap.on_flight_cap
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
