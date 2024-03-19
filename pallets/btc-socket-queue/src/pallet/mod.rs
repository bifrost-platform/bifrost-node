mod impls;

use crate::{
	ReqId, SignStatus, SignedPsbtMessage, UnsignedPsbtMessage, WeightInfo, MAX_QUEUE_SIZE,
	MULTI_SIG_MAX_ACCOUNTS,
};

use frame_support::{
	pallet_prelude::*,
	traits::{SortedMembers, StorageVersion},
};
use frame_system::pallet_prelude::*;

use bp_multi_sig::traits::MultiSigManager;
use sp_runtime::traits::{IdentifyAccount, Verify};

#[frame_support::pallet]
pub mod pallet {
	use miniscript::bitcoin::Psbt;
	use sp_core::{keccak_256, H256};

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
		/// The relay executive members.
		type Executives: SortedMembers<Self::AccountId>;
		type MultiSig: MultiSigManager;
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T> {
		AuthorityAlreadySubmittedSignedPsbt,
		RequestAlreadySubmitted,
		RequestDNE,
		OutOfRange,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		UnsignedPsbtSubmitted { req_id: ReqId },
	}

	#[pallet::storage]
	#[pallet::getter(fn unsigned_submitter)]
	/// The unsigned psbt submitter address.
	pub type UnsignedSubmitter<T: Config> = StorageValue<_, T::Signer, OptionQuery>;

	#[pallet::storage]
	#[pallet::unbounded]
	#[pallet::getter(fn queue)]
	pub type Queue<T: Config> = StorageValue<
		_,
		BoundedVec<UnsignedPsbtMessage<T::AccountId>, ConstU32<MAX_QUEUE_SIZE>>,
		ValueQuery,
	>;

	#[pallet::storage]
	#[pallet::unbounded]
	#[pallet::getter(fn pending_psbt)]
	pub type PendingPsbt<T: Config> = StorageMap<
		_,
		Twox64Concat,
		ReqId,
		BoundedVec<SignedPsbtMessage<T::AccountId>, ConstU32<MULTI_SIG_MAX_ACCOUNTS>>,
	>;

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

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::submit_unsigned_psbt())]
		pub fn submit_unsigned_psbt(
			origin: OriginFor<T>,
			unsigned_psbt: UnsignedPsbtMessage<T::AccountId>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			let UnsignedPsbtMessage { req_id, .. } = unsigned_psbt;

			ensure!(!<PendingPsbt<T>>::contains_key(req_id), Error::<T>::RequestAlreadySubmitted);
			ensure!(!<AcceptedPsbt<T>>::contains_key(req_id), Error::<T>::RequestAlreadySubmitted);
			ensure!(!<RejectedPsbt<T>>::contains_key(req_id), Error::<T>::RequestAlreadySubmitted);

			let mut queue = <Queue<T>>::get();
			queue.try_push(unsigned_psbt).map_err(|_| Error::<T>::OutOfRange)?;
			<Queue<T>>::put(queue);
			<PendingPsbt<T>>::insert(req_id, BoundedVec::default());

			Self::deposit_event(Event::UnsignedPsbtSubmitted { req_id });

			Ok(().into())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::submit_signed_psbt())]
		pub fn submit_signed_psbt(
			origin: OriginFor<T>,
			signed_psbt: SignedPsbtMessage<T::AccountId>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			let SignedPsbtMessage { authority_id, req_id, .. } = &signed_psbt;

			let mut pending_psbt = <PendingPsbt<T>>::get(req_id).ok_or(Error::<T>::RequestDNE)?;
			ensure!(
				!pending_psbt.iter().any(|p| &p.authority_id == authority_id),
				Error::<T>::AuthorityAlreadySubmittedSignedPsbt // TODO: allow change?
			);
			pending_psbt.try_push(signed_psbt).map_err(|_| Error::<T>::OutOfRange)?;

			let accepted_len = pending_psbt
				.iter()
				.filter(|p| matches!(p.status, SignStatus::Accepted))
				.collect::<Vec<_>>()
				.len();

			let rejected_len = pending_psbt
				.iter()
				.filter(|p| matches!(p.status, SignStatus::Rejected))
				.collect::<Vec<_>>()
				.len();
			// if T::MultiSig::is_multi_signed(
			// 	,
			// ) {}

			Ok(().into())
		}
	}

	#[pallet::validate_unsigned]
	impl<T: Config> ValidateUnsigned for Pallet<T> {
		type Call = Call<T>;

		fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
			match call {
				Call::submit_unsigned_psbt { unsigned_psbt, signature } => {
					let UnsignedPsbtMessage { submitter, req_id, psbt } = unsigned_psbt;

					if let Some(s) = <UnsignedSubmitter<T>>::get() {
						if s.into_account() != *submitter {
							return InvalidTransaction::BadSigner.into();
						}
					} else {
						return InvalidTransaction::BadSigner.into();
					}

					if Psbt::deserialize(&psbt).is_err() {
						return InvalidTransaction::BadProof.into();
					}

					let message = format!("{:?}:{}:{}", submitter, req_id, H256(keccak_256(&psbt)));
					if !signature.verify(message.as_bytes(), submitter) {
						return InvalidTransaction::BadProof.into();
					}

					ValidTransaction::with_tag_prefix("UnsignedPsbtSubmission")
						.priority(TransactionPriority::MAX)
						.and_provides((submitter, req_id))
						.propagate(true)
						.build()
				},
				Call::submit_signed_psbt { signed_psbt, signature } => {
					let SignedPsbtMessage { authority_id, req_id, psbt, status } = signed_psbt;

					// verify if the authority is a relay executive member.
					if !T::Executives::contains(&authority_id) {
						return InvalidTransaction::BadSigner.into();
					}

					if Psbt::deserialize(&psbt).is_err() {
						return InvalidTransaction::BadProof.into();
					}

					// verify if the signature was originated from the authority.
					let message =
						format!("{:?}:{}:{}", authority_id, req_id, H256(keccak_256(&psbt)));
					if !signature.verify(message.as_bytes(), authority_id) {
						return InvalidTransaction::BadProof.into();
					}

					ValidTransaction::with_tag_prefix("SignedPsbtSubmission")
						.priority(TransactionPriority::MAX)
						.and_provides((authority_id, req_id, status))
						.propagate(true)
						.build()
				},
				_ => InvalidTransaction::Call.into(),
			}
		}
	}
}
