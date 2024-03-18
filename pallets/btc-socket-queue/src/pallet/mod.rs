mod impls;

use crate::{
	ReqId, SignedPsbtMessage, UnsignedPsbtMessage, WeightInfo, MAX_QUEUE_SIZE,
	MULTI_SIG_MAX_ACCOUNTS,
};

use frame_support::{pallet_prelude::*, traits::StorageVersion};
use frame_system::pallet_prelude::*;

use sp_runtime::traits::{IdentifyAccount, Verify};

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
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {}

	#[pallet::storage]
	#[pallet::getter(fn signer)]
	/// The signer address. The signature is originated from this account.
	pub type Signer<T: Config> = StorageValue<_, T::Signer, OptionQuery>;

	#[pallet::storage]
	#[pallet::unbounded]
	#[pallet::getter(fn queue)]
	pub type Queue<T: Config> =
		StorageValue<_, BoundedVec<UnsignedPsbtMessage, ConstU32<MAX_QUEUE_SIZE>>, ValueQuery>;

	#[pallet::storage]
	#[pallet::unbounded]
	#[pallet::getter(fn pending_psbt)]
	pub type PendingPsbt<T: Config> =
		StorageMap<_, Twox64Concat, ReqId, SignedPsbtMessage<T::AccountId>>;

	#[pallet::storage]
	#[pallet::unbounded]
	#[pallet::getter(fn accepted_psbt)]
	pub type AcceptedPsbt<T: Config> = StorageMap<
		_,
		Twox64Concat,
		ReqId,
		BoundedVec<SignedPsbtMessage<T::AccountId>, ConstU32<MULTI_SIG_MAX_ACCOUNTS>>,
	>;

	#[pallet::storage]
	#[pallet::unbounded]
	#[pallet::getter(fn rejected_psbt)]
	pub type RejectedPsbt<T: Config> = StorageMap<
		_,
		Twox64Concat,
		ReqId,
		BoundedVec<SignedPsbtMessage<T::AccountId>, ConstU32<MULTI_SIG_MAX_ACCOUNTS>>,
	>;
}
