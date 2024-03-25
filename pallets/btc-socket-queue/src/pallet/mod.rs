mod impls;

use crate::{PsbtRequest, SignedPsbtMessage, UnsignedPsbtMessage, WeightInfo};

use frame_support::{
	pallet_prelude::*,
	traits::{SortedMembers, StorageVersion},
};
use frame_system::pallet_prelude::*;
use scale_info::prelude::format;

use bp_multi_sig::traits::{MultiSigManager, PoolManager};
use sp_core::{H160, H256};
use sp_runtime::traits::{IdentifyAccount, Verify};
use sp_std::{str, vec, vec::Vec};

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_evm::Config {
		/// Overarching event type
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		/// Required origin for setting or resetting the configuration.
		type SetOrigin: EnsureOrigin<Self::RuntimeOrigin>;
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
		/// The Bitcon registration pool pallet.
		type RegistrationPool: MultiSigManager + PoolManager<Self::AccountId>;
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The authority has already submitted a signed PSBT.
		AuthorityAlreadySubmitted,
		/// The signed PSBT is already submitted by an authority.
		SignedPsbtAlreadySubmitted,
		/// The request has already been finalized or exists.
		RequestAlreadyExists,
		SubmitterDNE,
		SocketDNE,
		SocketMessageDNE,
		UserDNE,
		SystemVaultDne,
		/// The request hasn't been submitted yet.
		RequestDNE,
		/// The submitted PSBT is invalid.
		InvalidPsbt,
		InvalidCalldata,
		InvalidSocketMessage,
		InvalidRequestInfo,
		/// The value is out of range.
		OutOfRange,
		/// Cannot overwrite to the same value.
		NoWritingSameValue,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An unsigned PSBT for an outbound request has been submitted.
		UnsignedPsbtSubmitted { psbt: Vec<u8> },
		/// A signed PSBT for an outbound request has been submitted.
		SignedPsbtSubmitted { req_id: Vec<u8>, authority_id: T::AccountId },
		/// An outbound request has been finalized.
		RequestFinalized { req_id: Vec<u8> },
		/// A submitter has been set.
		SubmitterSet { new: T::Signer },
	}

	#[pallet::storage]
	#[pallet::getter(fn socket)]
	/// The Socket contract address.
	pub type Socket<T: Config> = StorageValue<_, T::AccountId, OptionQuery>;

	#[pallet::storage]
	#[pallet::getter(fn unsigned_psbt_submitter)]
	/// The unsigned PSBT submitter address.
	pub type UnsignedPsbtSubmitter<T: Config> = StorageValue<_, T::Signer, OptionQuery>;

	#[pallet::storage]
	#[pallet::unbounded]
	#[pallet::getter(fn pending_requests)]
	/// Pending outbound requests that are not ready to be finalized.
	pub type PendingRequests<T: Config> =
		StorageMap<_, Twox64Concat, H256, PsbtRequest<T::AccountId>>;

	#[pallet::storage]
	#[pallet::unbounded]
	#[pallet::getter(fn accepted_requests)]
	/// Accepted outbound requests that are ready to be combined and finalized.
	pub type AcceptedRequests<T: Config> =
		StorageMap<_, Twox64Concat, H256, PsbtRequest<T::AccountId>>;

	#[pallet::storage]
	#[pallet::unbounded]
	#[pallet::getter(fn finalized_requests)]
	/// Finalized outbound requests that has been finalized and broadcasted to the Bitcoin network.
	pub type FinalizedRequests<T: Config> =
		StorageMap<_, Twox64Concat, H256, PsbtRequest<T::AccountId>>;

	#[pallet::call]
	impl<T: Config> Pallet<T>
	where
		T::AccountId: Into<H160>,
		H160: Into<T::AccountId>,
	{
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::set_submitter())]
		/// Set the unsigned PSBT submitter.
		pub fn set_submitter(origin: OriginFor<T>, new: T::Signer) -> DispatchResultWithPostInfo {
			T::SetOrigin::ensure_origin(origin)?;

			if let Some(old) = <UnsignedPsbtSubmitter<T>>::get() {
				ensure!(old != new, Error::<T>::NoWritingSameValue);
			}

			<UnsignedPsbtSubmitter<T>>::put(new.clone());
			Self::deposit_event(Event::SubmitterSet { new });

			Ok(().into())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::submit_unsigned_psbt())]
		/// Submit an unsigned PSBT of an outbound request.
		/// This extrinsic can only be executed by the `UnsignedPsbtSubmitter`.
		pub fn submit_unsigned_psbt(
			origin: OriginFor<T>,
			msg: UnsignedPsbtMessage<T::AccountId>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			let UnsignedPsbtMessage { socket_messages, psbt, .. } = msg;

			let psbt_hash = Self::hash_bytes(&psbt);

			// the request shouldn't been handled yet
			ensure!(
				!<PendingRequests<T>>::contains_key(&psbt_hash),
				Error::<T>::RequestAlreadyExists
			);
			ensure!(
				!<AcceptedRequests<T>>::contains_key(&psbt_hash),
				Error::<T>::RequestAlreadyExists
			);
			ensure!(
				!<FinalizedRequests<T>>::contains_key(&psbt_hash),
				Error::<T>::RequestAlreadyExists
			);

			Self::try_psbt_output_verification(
				&psbt,
				Self::try_build_psbt_outputs(socket_messages)?,
			)?;

			<PendingRequests<T>>::insert(&psbt_hash, PsbtRequest::new(psbt.clone()));
			Self::deposit_event(Event::UnsignedPsbtSubmitted { psbt });

			Ok(().into())
		}

		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::submit_signed_psbt())]
		/// Submit a signed PSBT of a pending outbound request.
		/// This extrinsic can only be executed by relay executives.
		pub fn submit_signed_psbt(
			origin: OriginFor<T>,
			msg: SignedPsbtMessage<T::AccountId>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			let SignedPsbtMessage { authority_id, unsigned_psbt, signed_psbt } = msg;

			// let mut pending_request =
			// 	<PendingRequests<T>>::get(&req_id).ok_or(Error::<T>::RequestDNE)?;

			// ensure!(
			// 	!pending_request.is_authority_submitted(&authority_id),
			// 	Error::<T>::AuthorityAlreadySubmitted
			// );
			// ensure!(
			// 	!pending_request.is_signed_psbt_submitted(&signed_psbt),
			// 	Error::<T>::SignedPsbtAlreadySubmitted
			// );
			// ensure!(pending_request.is_unsigned_psbt(&unsigned_psbt), Error::<T>::InvalidPsbt);
			// ensure!(!pending_request.is_unsigned_psbt(&signed_psbt), Error::<T>::InvalidPsbt);
			// Self::verify_signed_psbt(&unsigned_psbt, &signed_psbt)?;

			// pending_request
			// 	.insert_signed_psbt(authority_id.clone(), signed_psbt)
			// 	.map_err(|_| Error::<T>::OutOfRange)?;

			// if T::MultiSig::is_finalizable(pending_request.signed_psbts.len() as u8) {
			// 	// if finalizable (quorum reached m), then accept the request
			// 	<FinalizedRequests<T>>::insert(&req_id, pending_request);
			// 	<PendingRequests<T>>::remove(&req_id);
			// 	Self::deposit_event(Event::RequestFinalized { req_id });
			// } else {
			// 	// if not, remain as pending
			// 	<PendingRequests<T>>::insert(&req_id, pending_request);
			// 	Self::deposit_event(Event::SignedPsbtSubmitted { req_id, authority_id });
			// }

			Ok(().into())
		}
	}

	#[pallet::validate_unsigned]
	impl<T: Config> ValidateUnsigned for Pallet<T>
	where
		T::AccountId: Into<H160>,
		H160: Into<T::AccountId>,
	{
		type Call = Call<T>;

		fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
			match call {
				Call::submit_unsigned_psbt { msg, signature } => {
					let UnsignedPsbtMessage { submitter, psbt, .. } = msg;

					// verify if the submitter is valid
					if let Some(s) = <UnsignedPsbtSubmitter<T>>::get() {
						if s.into_account() != *submitter {
							return InvalidTransaction::BadSigner.into();
						}
					} else {
						return InvalidTransaction::BadSigner.into();
					}

					// verify if the signature was originated from the submitter.
					let message = format!("{:?}", Self::hash_bytes(psbt));
					if !signature.verify(message.as_bytes(), submitter) {
						return InvalidTransaction::BadProof.into();
					}

					ValidTransaction::with_tag_prefix("UnsignedPsbtSubmission")
						.priority(TransactionPriority::MAX)
						.and_provides(submitter)
						.propagate(true)
						.build()
				},
				Call::submit_signed_psbt { msg, signature } => {
					let SignedPsbtMessage { authority_id, signed_psbt, .. } = msg;

					// verify if the authority is a relay executive member.
					if !T::Executives::contains(&authority_id) {
						return InvalidTransaction::BadSigner.into();
					}

					// verify if the signature was originated from the authority.
					let message = format!("{:?}", Self::hash_bytes(signed_psbt));
					if !signature.verify(message.as_bytes(), authority_id) {
						return InvalidTransaction::BadProof.into();
					}

					ValidTransaction::with_tag_prefix("SignedPsbtSubmission")
						.priority(TransactionPriority::MAX)
						.and_provides(authority_id)
						.propagate(true)
						.build()
				},
				_ => InvalidTransaction::Call.into(),
			}
		}
	}
}
