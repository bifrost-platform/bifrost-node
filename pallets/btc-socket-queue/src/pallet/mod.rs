mod impls;

use crate::{FinalizePsbtMessage, PsbtRequest, SignedPsbtMessage, UnsignedPsbtMessage, WeightInfo};

use frame_support::{
	pallet_prelude::*,
	traits::{SortedMembers, StorageVersion},
};
use frame_system::pallet_prelude::*;
use scale_info::prelude::format;

use bp_multi_sig::{
	traits::{MultiSigManager, PoolManager},
	UnboundedBytes,
};
use sp_core::{H160, H256};
use sp_runtime::traits::{IdentifyAccount, Verify};
use sp_std::{str, vec};

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
		AuthorityDNE,
		SocketDNE,
		SocketMessageDNE,
		UserDNE,
		SystemVaultDNE,
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
		UnsignedPsbtSubmitted {
			psbt: UnboundedBytes,
		},
		/// A signed PSBT for an outbound request has been submitted.
		SignedPsbtSubmitted {
			psbt_hash: H256,
			authority_id: T::AccountId,
			signed_psbt: UnboundedBytes,
		},
		/// An outbound request has been accepted.
		RequestAccepted {
			psbt_hash: H256,
		},
		/// An outbound request has been finalized.
		RequestFinalized {
			psbt_hash: H256,
		},
		/// An authority has been set.
		AuthoritySet {
			new: T::Signer,
		},
		SocketSet {
			new: T::AccountId,
		},
	}

	#[pallet::storage]
	#[pallet::getter(fn socket_contract)]
	/// The Socket contract address.
	pub type Socket<T: Config> = StorageValue<_, T::AccountId, OptionQuery>;

	#[pallet::storage]
	#[pallet::getter(fn authority)]
	/// The core authority address.
	pub type Authority<T: Config> = StorageValue<_, T::Signer, OptionQuery>;

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

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		pub authority: Option<T::AccountId>,
		#[serde(skip)]
		pub _config: PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T>
	where
		T::Signer: From<[u8; 20]>,
		T::AccountId: AsRef<[u8; 20]>,
	{
		fn build(&self) {
			if let Some(a) = &self.authority {
				Authority::<T>::put(T::Signer::from(*a.as_ref()));
			}
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T>
	where
		T::AccountId: Into<H160>,
		H160: Into<T::AccountId>,
	{
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::set_authority())]
		/// Set the authority address.
		pub fn set_authority(origin: OriginFor<T>, new: T::Signer) -> DispatchResultWithPostInfo {
			T::SetOrigin::ensure_origin(origin)?;

			if let Some(old) = <Authority<T>>::get() {
				ensure!(old != new, Error::<T>::NoWritingSameValue);
			}

			<Authority<T>>::put(new.clone());
			Self::deposit_event(Event::AuthoritySet { new });

			Ok(().into())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::set_socket())]
		/// Set the authority address.
		pub fn set_socket(origin: OriginFor<T>, new: T::AccountId) -> DispatchResultWithPostInfo {
			T::SetOrigin::ensure_origin(origin)?;

			if let Some(old) = <Socket<T>>::get() {
				ensure!(old != new, Error::<T>::NoWritingSameValue);
			}

			<Socket<T>>::put(new.clone());
			Self::deposit_event(Event::SocketSet { new });

			Ok(().into())
		}

		#[pallet::call_index(2)]
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

			let unchecked = Self::try_build_unchecked_outputs(&socket_messages)?;
			Self::try_psbt_output_verification(&psbt, unchecked)?;

			<PendingRequests<T>>::insert(
				&psbt_hash,
				PsbtRequest::new(psbt.clone(), socket_messages),
			);
			Self::deposit_event(Event::UnsignedPsbtSubmitted { psbt });

			Ok(().into())
		}

		#[pallet::call_index(3)]
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

			let psbt_hash = Self::hash_bytes(&unsigned_psbt);

			let mut pending_request =
				<PendingRequests<T>>::get(&psbt_hash).ok_or(Error::<T>::RequestDNE)?;

			ensure!(
				!pending_request.is_authority_submitted(&authority_id),
				Error::<T>::AuthorityAlreadySubmitted
			);
			ensure!(
				!pending_request.is_signed_psbt_submitted(&signed_psbt),
				Error::<T>::SignedPsbtAlreadySubmitted
			);
			ensure!(!pending_request.is_unsigned_psbt(&signed_psbt), Error::<T>::InvalidPsbt);
			Self::try_signed_psbt_verification(&unsigned_psbt, &signed_psbt)?;

			pending_request
				.signed_psbts
				.try_insert(authority_id.clone(), signed_psbt.clone())
				.map_err(|_| Error::<T>::OutOfRange)?;

			if T::RegistrationPool::is_finalizable(pending_request.signed_psbts.len() as u8) {
				// if finalizable (quorum reached m), then accept the request
				<AcceptedRequests<T>>::insert(&psbt_hash, pending_request);
				<PendingRequests<T>>::remove(&psbt_hash);
				Self::deposit_event(Event::RequestAccepted { psbt_hash });
			} else {
				// if not, remain as pending
				<PendingRequests<T>>::insert(&psbt_hash, pending_request);
				Self::deposit_event(Event::SignedPsbtSubmitted {
					psbt_hash,
					authority_id,
					signed_psbt,
				});
			}

			Ok(().into())
		}

		#[pallet::call_index(4)]
		#[pallet::weight(<T as Config>::WeightInfo::finalize_request())]
		pub fn finalize_request(
			origin: OriginFor<T>,
			msg: FinalizePsbtMessage<T::AccountId>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			let FinalizePsbtMessage { psbt_hash, .. } = msg;

			let request = <AcceptedRequests<T>>::get(&psbt_hash).ok_or(Error::<T>::RequestDNE)?;
			<AcceptedRequests<T>>::remove(&psbt_hash);
			<FinalizedRequests<T>>::insert(&psbt_hash, request);
			Self::deposit_event(Event::RequestFinalized { psbt_hash });

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
					let UnsignedPsbtMessage { authority_id, psbt, .. } = msg;

					// verify if the authority_id is valid
					if let Some(a) = <Authority<T>>::get() {
						if a.into_account() != *authority_id {
							return InvalidTransaction::BadSigner.into();
						}
					} else {
						return InvalidTransaction::BadSigner.into();
					}

					// verify if the signature was originated from the authority_id.
					let message = format!("{:?}", Self::hash_bytes(psbt));
					if !signature.verify(message.as_bytes(), authority_id) {
						return InvalidTransaction::BadProof.into();
					}

					ValidTransaction::with_tag_prefix("UnsignedPsbtSubmission")
						.priority(TransactionPriority::MAX)
						.and_provides(authority_id)
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
				Call::finalize_request { msg, signature } => {
					let FinalizePsbtMessage { authority_id, psbt_hash } = msg;

					// verify if the authority_id is valid
					if let Some(a) = <Authority<T>>::get() {
						if a.into_account() != *authority_id {
							return InvalidTransaction::BadSigner.into();
						}
					} else {
						return InvalidTransaction::BadSigner.into();
					}

					// verify if the signature was originated from the authority_id.
					let message = format!("{:?}", psbt_hash);
					if !signature.verify(message.as_bytes(), authority_id) {
						return InvalidTransaction::BadProof.into();
					}

					ValidTransaction::with_tag_prefix("FinalizePsbtSubmission")
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
