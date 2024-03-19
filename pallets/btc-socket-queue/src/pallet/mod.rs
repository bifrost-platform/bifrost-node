mod impls;

use crate::{OutboundRequests, ReqId, SignedPsbtMessage, UnsignedPsbtMessage, WeightInfo};

use frame_support::{
	pallet_prelude::*,
	traits::{SortedMembers, StorageVersion},
};
use frame_system::pallet_prelude::*;

use bp_multi_sig::traits::MultiSigManager;
use miniscript::bitcoin::Psbt;
use sp_core::{keccak_256, H256};
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
		RequestAlreadyAccepted,
		RequestAlreadyRejected,
		RequestDNE,
		InvalidPsbt,
		InvalidSocketStatus,
		OutOfRange,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		UnsignedPsbtSubmitted { req_id: ReqId },
		SignedPsbtSubmitted { req_id: ReqId, authority_id: T::AccountId },
		RequestAccepted { req_id: ReqId },
	}

	#[pallet::storage]
	#[pallet::getter(fn unsigned_submitter)]
	/// The unsigned psbt submitter address.
	pub type UnsignedSubmitter<T: Config> = StorageValue<_, T::Signer, OptionQuery>;

	#[pallet::storage]
	#[pallet::unbounded]
	#[pallet::getter(fn pending_requests)]
	pub type PendingRequests<T: Config> =
		StorageMap<_, Twox64Concat, ReqId, OutboundRequests<T::AccountId>>;

	#[pallet::storage]
	#[pallet::unbounded]
	#[pallet::getter(fn accepted_requests)]
	pub type AcceptedRequests<T: Config> =
		StorageMap<_, Twox64Concat, ReqId, OutboundRequests<T::AccountId>>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::submit_unsigned_psbt())]
		pub fn submit_unsigned_psbt(
			origin: OriginFor<T>,
			msg: UnsignedPsbtMessage<T::AccountId>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			let UnsignedPsbtMessage { req_id, psbt, .. } = msg;

			ensure!(
				!<PendingRequests<T>>::contains_key(req_id),
				Error::<T>::RequestAlreadySubmitted
			);
			ensure!(
				!<AcceptedRequests<T>>::contains_key(req_id),
				Error::<T>::RequestAlreadyAccepted
			);

			if Psbt::deserialize(&psbt).is_err() {
				return Err(Error::<T>::InvalidPsbt)?;
			}

			<PendingRequests<T>>::insert(req_id, OutboundRequests::new(psbt));

			Self::deposit_event(Event::UnsignedPsbtSubmitted { req_id });

			Ok(().into())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::submit_signed_psbt())]
		pub fn submit_signed_psbt(
			origin: OriginFor<T>,
			msg: SignedPsbtMessage<T::AccountId>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			let SignedPsbtMessage { authority_id, req_id, origin_psbt, signed_psbt } = msg;

			let mut pending_request =
				<PendingRequests<T>>::get(&req_id).ok_or(Error::<T>::RequestDNE)?;

			ensure!(
				!pending_request.signed_psbts.contains_key(&authority_id),
				Error::<T>::AuthorityAlreadySubmittedSignedPsbt
			);
			ensure!(pending_request.origin_psbt == origin_psbt, Error::<T>::InvalidPsbt);
			ensure!(pending_request.origin_psbt != signed_psbt, Error::<T>::InvalidPsbt);

			let mut de_origin_psbt =
				Psbt::deserialize(&origin_psbt).map_err(|_| Error::<T>::InvalidPsbt)?;
			let de_signed_psbt =
				Psbt::deserialize(&signed_psbt).map_err(|_| Error::<T>::InvalidPsbt)?;
			if de_origin_psbt.combine(de_signed_psbt).is_err() {
				return Err(Error::<T>::InvalidPsbt)?;
			}

			pending_request
				.signed_psbts
				.try_insert(authority_id.clone(), signed_psbt)
				.map_err(|_| Error::<T>::OutOfRange)?;

			if T::MultiSig::is_finalizable(pending_request.signed_psbts.len() as u8) {
				<AcceptedRequests<T>>::insert(req_id, pending_request);
				<PendingRequests<T>>::remove(req_id);
				Self::deposit_event(Event::RequestAccepted { req_id });
			} else {
				<PendingRequests<T>>::insert(req_id, pending_request);
				Self::deposit_event(Event::SignedPsbtSubmitted { req_id, authority_id });
			}

			Ok(().into())
		}
	}

	#[pallet::validate_unsigned]
	impl<T: Config> ValidateUnsigned for Pallet<T> {
		type Call = Call<T>;

		fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
			match call {
				Call::submit_unsigned_psbt { msg, signature } => {
					let UnsignedPsbtMessage { submitter, req_id, psbt } = msg;

					if let Some(s) = <UnsignedSubmitter<T>>::get() {
						if s.into_account() != *submitter {
							return InvalidTransaction::BadSigner.into();
						}
					} else {
						return InvalidTransaction::BadSigner.into();
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
				Call::submit_signed_psbt { msg, signature } => {
					let SignedPsbtMessage { authority_id, req_id, origin_psbt, .. } = msg;

					// verify if the authority is a relay executive member.
					if !T::Executives::contains(&authority_id) {
						return InvalidTransaction::BadSigner.into();
					}

					// verify if the signature was originated from the authority.
					let message =
						format!("{:?}:{}:{}", authority_id, req_id, H256(keccak_256(&origin_psbt)));
					if !signature.verify(message.as_bytes(), authority_id) {
						return InvalidTransaction::BadProof.into();
					}

					ValidTransaction::with_tag_prefix("SignedPsbtSubmission")
						.priority(TransactionPriority::MAX)
						.and_provides((authority_id, req_id))
						.propagate(true)
						.build()
				},
				_ => InvalidTransaction::Call.into(),
			}
		}
	}
}
