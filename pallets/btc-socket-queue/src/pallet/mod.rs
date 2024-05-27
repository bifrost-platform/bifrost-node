mod impls;

use crate::{
	ExecutedPsbtMessage, PsbtRequest, RollbackPollMessage, RollbackPsbtMessage, RollbackRequest,
	SignedPsbtMessage, SocketMessage, UnsignedPsbtMessage, WeightInfo,
};

use frame_support::{
	pallet_prelude::*,
	traits::{SortedMembers, StorageVersion},
};
use frame_system::pallet_prelude::*;

use bp_multi_sig::{traits::PoolManager, Amount, UnboundedBytes};
use bp_staking::traits::Authorities;
use sp_core::{H160, H256, U256};
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
		type Relayers: Authorities<Self::AccountId>;
		/// The Bitcoin registration pool pallet.
		type RegistrationPool: PoolManager<Self::AccountId>;
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The authority has already submitted a signed PSBT.
		AuthorityAlreadySubmitted,
		/// The signed PSBT is already submitted by an authority.
		SignedPsbtAlreadySubmitted,
		/// The socket message is already submitted.
		SocketMessageAlreadySubmitted,
		/// The request has already been finalized or exists.
		RequestAlreadyExists,
		/// The request has already been approved.
		RequestAlreadyApproved,
		/// The authority account does not exist.
		AuthorityDNE,
		/// The socket contract does not exist.
		SocketDNE,
		/// The socket message does not exist.
		SocketMessageDNE,
		/// The user does not exist.
		UserDNE,
		/// The system vault does not exist.
		SystemVaultDNE,
		/// The request hasn't been submitted yet.
		RequestDNE,
		/// The submitted PSBT is invalid.
		InvalidPsbt,
		/// The contract calldata is invalid.
		InvalidCalldata,
		/// The socket message is invalid.
		InvalidSocketMessage,
		/// The request information is invalid.
		InvalidRequestInfo,
		/// The transaction information is invalid.
		InvalidTxInfo,
		/// The given bitcoin address is invalid.
		InvalidBitcoinAddress,
		/// The submitted system vout is invalid.
		InvalidSystemVout,
		/// Cannot finalize the PSBT.
		CannotFinalizePsbt,
		/// The value is out of range.
		OutOfRange,
		/// Cannot overwrite to the same value.
		NoWritingSameValue,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An unsigned PSBT for an outbound request has been submitted.
		UnsignedPsbtSubmitted { txid: H256 },
		/// A signed PSBT for an outbound request has been submitted.
		SignedPsbtSubmitted { txid: H256, authority_id: T::AccountId },
		/// An unsigned PSBT for a rollback request has been submitted.
		RollbackPsbtSubmitted { txid: H256 },
		/// A rollback poll has been submitted.
		RollbackPollSubmitted { txid: H256, authority_id: T::AccountId, is_approved: bool },
		/// A rollback request has been approved.
		RollbackApproved { txid: H256 },
		/// An outbound request has been finalized.
		RequestFinalized { txid: H256 },
		/// An outbound request has been executed.
		RequestExecuted { txid: H256 },
		/// An authority has been set.
		AuthoritySet { new: T::AccountId },
		/// A socket contract has been set.
		SocketSet { new: T::AccountId, is_bitcoin: bool },
	}

	#[pallet::storage]
	#[pallet::getter(fn socket_contract)]
	/// The Socket contract address.
	pub type Socket<T: Config> = StorageValue<_, T::AccountId, OptionQuery>;

	#[pallet::storage]
	#[pallet::getter(fn bitcoin_socket_contract)]
	/// The BitcoinSocket contract address.
	pub type BitcoinSocket<T: Config> = StorageValue<_, T::AccountId, OptionQuery>;

	#[pallet::storage]
	#[pallet::getter(fn authority)]
	/// The core authority address.
	pub type Authority<T: Config> = StorageValue<_, T::AccountId, OptionQuery>;

	#[pallet::storage]
	#[pallet::unbounded]
	#[pallet::getter(fn socket_messages)]
	/// The submitted `SocketMessage` instances.
	/// key: Request sequence ID.
	/// value: The socket message in bytes.
	pub type SocketMessages<T: Config> = StorageMap<_, Twox64Concat, U256, SocketMessage>;

	#[pallet::storage]
	#[pallet::unbounded]
	#[pallet::getter(fn pending_requests)]
	/// Pending outbound requests that are not ready to be finalized.
	/// key: The pending PSBT's txid.
	/// value: The PSBT information.
	pub type PendingRequests<T: Config> =
		StorageMap<_, Twox64Concat, H256, PsbtRequest<T::AccountId>>;

	#[pallet::storage]
	#[pallet::unbounded]
	#[pallet::getter(fn finalized_requests)]
	/// Finalized outbound requests.
	/// key: The finalized PSBT's txid.
	/// value: The PSBT information.
	pub type FinalizedRequests<T: Config> =
		StorageMap<_, Twox64Concat, H256, PsbtRequest<T::AccountId>>;

	#[pallet::storage]
	#[pallet::unbounded]
	#[pallet::getter(fn executed_requests)]
	/// Outbound requests that has been broadcasted to the Bitcoin network.
	/// key: The executed PSBT's txid.
	/// value: The PSBT information.
	pub type ExecutedRequests<T: Config> =
		StorageMap<_, Twox64Concat, H256, PsbtRequest<T::AccountId>>;

	#[pallet::storage]
	#[pallet::unbounded]
	#[pallet::getter(fn rollback_requests)]
	pub type RollbackRequests<T: Config> =
		StorageMap<_, Twox64Concat, H256, RollbackRequest<T::AccountId>>;

	#[pallet::storage]
	#[pallet::unbounded]
	#[pallet::getter(fn bonded_outbound_tx)]
	/// Mapped txid's.
	/// key: The PSBT's txid.
	/// value: The composed socket messages. This will be empty for rollback requests.
	pub type BondedOutboundTx<T: Config> = StorageMap<_, Twox64Concat, H256, Vec<UnboundedBytes>>;

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		pub authority: Option<T::AccountId>,
		#[serde(skip)]
		pub _config: PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			if let Some(a) = &self.authority {
				Authority::<T>::put(a);
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
		pub fn set_authority(
			origin: OriginFor<T>,
			new: T::AccountId,
		) -> DispatchResultWithPostInfo {
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
		pub fn set_socket(
			origin: OriginFor<T>,
			new: T::AccountId,
			is_bitcoin: bool,
		) -> DispatchResultWithPostInfo {
			T::SetOrigin::ensure_origin(origin)?;

			if is_bitcoin {
				if let Some(old) = <BitcoinSocket<T>>::get() {
					ensure!(old != new, Error::<T>::NoWritingSameValue);
				}

				<BitcoinSocket<T>>::put(new.clone());
				Self::deposit_event(Event::SocketSet { new, is_bitcoin });
			} else {
				if let Some(old) = <Socket<T>>::get() {
					ensure!(old != new, Error::<T>::NoWritingSameValue);
				}

				<Socket<T>>::put(new.clone());
				Self::deposit_event(Event::SocketSet { new, is_bitcoin });
			}

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

			let UnsignedPsbtMessage { system_vout, socket_messages, psbt, .. } = msg;

			// verify if psbt bytes are valid
			let psbt_obj = Self::try_get_checked_psbt(&psbt)?;
			let txid = Self::convert_txid(psbt_obj.unsigned_tx.txid());

			// prevent storage duplication
			ensure!(!<PendingRequests<T>>::contains_key(&txid), Error::<T>::RequestAlreadyExists);
			ensure!(!<FinalizedRequests<T>>::contains_key(&txid), Error::<T>::RequestAlreadyExists);
			ensure!(!<ExecutedRequests<T>>::contains_key(&txid), Error::<T>::RequestAlreadyExists);

			// verify PSBT outputs
			let system_vout =
				usize::try_from(system_vout).map_err(|_| Error::<T>::InvalidSystemVout)?;
			let (unchecked, msgs) =
				Self::try_build_unchecked_outputs(&socket_messages, system_vout)?;
			Self::try_psbt_output_verification(&psbt_obj, unchecked, system_vout)?;

			for msg in msgs {
				<SocketMessages<T>>::insert(msg.req_id.sequence, msg);
			}
			<PendingRequests<T>>::insert(
				&txid,
				PsbtRequest::new(psbt.clone(), socket_messages, false),
			);
			Self::deposit_event(Event::UnsignedPsbtSubmitted { txid });

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

			// verify if psbt bytes are valid
			let unsigned_psbt_obj = Self::try_get_checked_psbt(&unsigned_psbt)?;
			let signed_psbt_obj = Self::try_get_checked_psbt(&signed_psbt)?;

			let txid = Self::convert_txid(unsigned_psbt_obj.unsigned_tx.txid());

			let mut pending_request =
				<PendingRequests<T>>::get(&txid).ok_or(Error::<T>::RequestDNE)?;

			// prevent storage duplications
			ensure!(
				!pending_request.is_signed_psbt_submitted(&signed_psbt),
				Error::<T>::SignedPsbtAlreadySubmitted
			);
			ensure!(!pending_request.is_unsigned_psbt(&signed_psbt), Error::<T>::InvalidPsbt);

			// combine signed PSBT
			let combined_psbt_obj = Self::try_psbt_combination(
				&mut Self::try_get_checked_psbt(&pending_request.combined_psbt)?,
				&signed_psbt_obj,
			)?;
			pending_request.set_combined_psbt(combined_psbt_obj.serialize());
			pending_request
				.signed_psbts
				.try_insert(authority_id.clone(), signed_psbt.clone())
				.map_err(|_| Error::<T>::OutOfRange)?;

			// if finalizable (quorum reached m), then accept the request
			match Self::try_psbt_finalization(combined_psbt_obj) {
				Ok(finalized_psbt_obj) => {
					pending_request.set_finalized_psbt(finalized_psbt_obj.serialize());

					// move pending to finalized
					<FinalizedRequests<T>>::insert(&txid, pending_request.clone());
					<PendingRequests<T>>::remove(&txid);

					if !pending_request.is_rollback {
						<BondedOutboundTx<T>>::insert(&txid, pending_request.socket_messages);
					}

					Self::deposit_event(Event::SignedPsbtSubmitted { txid, authority_id });
					Self::deposit_event(Event::RequestFinalized { txid });
				},
				Err(_) => {
					// if not, remain as pending
					<PendingRequests<T>>::insert(&txid, pending_request);
					Self::deposit_event(Event::SignedPsbtSubmitted { txid, authority_id });
				},
			}

			Ok(().into())
		}

		#[pallet::call_index(4)]
		#[pallet::weight(<T as Config>::WeightInfo::submit_executed_request())]
		/// Submit an executed PSBT request.
		pub fn submit_executed_request(
			origin: OriginFor<T>,
			msg: ExecutedPsbtMessage<T::AccountId>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			let ExecutedPsbtMessage { txid, .. } = msg;

			let request = <FinalizedRequests<T>>::get(&txid).ok_or(Error::<T>::RequestDNE)?;
			<FinalizedRequests<T>>::remove(&txid);
			<ExecutedRequests<T>>::insert(&txid, request);
			Self::deposit_event(Event::RequestExecuted { txid });

			Ok(().into())
		}

		#[pallet::call_index(5)]
		#[pallet::weight(<T as Config>::WeightInfo::submit_rollback_request())]
		/// Submit a rollback PSBT request.
		pub fn submit_rollback_request(
			origin: OriginFor<T>,
			msg: RollbackPsbtMessage<T::AccountId>,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;

			let RollbackPsbtMessage { who, txid, vout, amount, unsigned_psbt } = msg;

			// verify if psbt bytes are valid
			let psbt_obj = Self::try_get_checked_psbt(&unsigned_psbt)?;
			let psbt_txid = Self::convert_txid(psbt_obj.unsigned_tx.txid());

			ensure!(
				!<RollbackRequests<T>>::contains_key(&psbt_txid),
				Error::<T>::RequestAlreadyExists
			);

			// user information must exist
			let vault = T::RegistrationPool::get_vault_address(&who).ok_or(Error::<T>::UserDNE)?;
			let refund =
				T::RegistrationPool::get_refund_address(&who).ok_or(Error::<T>::UserDNE)?;

			// the request must not exist on-chain
			let hash_key = Self::generate_hash_key(txid, vout, who.clone(), amount);
			let tx_info = Self::try_get_tx_info(hash_key)?;
			ensure!(tx_info.to.is_zero(), Error::<T>::RequestAlreadyExists);

			// the psbt must contain only one output
			let outputs = &psbt_obj.unsigned_tx.output;
			ensure!(outputs.len() == 1, Error::<T>::InvalidPsbt);

			// the output must be to the user's refund address
			let to =
				Self::try_convert_to_address_from_script(outputs[0].script_pubkey.as_script())?;
			ensure!(to == Self::try_convert_to_address_from_vec(refund)?, Error::<T>::InvalidPsbt);

			// the output amount must be less than the origin amount
			// (output.amount = origin amount - network fee)
			ensure!(
				Amount::from_sat(amount.as_u64()).checked_sub(outputs[0].value).is_some(),
				Error::<T>::InvalidPsbt
			);

			<RollbackRequests<T>>::insert(
				&psbt_txid,
				RollbackRequest::new(unsigned_psbt, who, txid, vout, vault, amount),
			);
			Self::deposit_event(Event::RollbackPsbtSubmitted { txid });

			Ok(().into())
		}

		#[pallet::call_index(6)]
		#[pallet::weight(<T as Config>::WeightInfo::submit_rollback_poll())]
		/// Submit a vote for a rollback request.
		pub fn submit_rollback_poll(
			origin: OriginFor<T>,
			msg: RollbackPollMessage<T::AccountId>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			let RollbackPollMessage { authority_id, unsigned_psbt, is_approved } = msg;

			// verify if psbt bytes are valid
			let psbt_obj = Self::try_get_checked_psbt(&unsigned_psbt)?;
			let psbt_txid = Self::convert_txid(psbt_obj.unsigned_tx.txid());

			let mut rollback_request =
				<RollbackRequests<T>>::get(&psbt_txid).ok_or(Error::<T>::RequestDNE)?;
			ensure!(!rollback_request.is_approved, Error::<T>::RequestAlreadyApproved);

			rollback_request
				.votes
				.try_insert(authority_id.clone(), is_approved)
				.map_err(|_| Error::<T>::OutOfRange)?;

			Self::deposit_event(Event::RollbackPollSubmitted {
				txid: psbt_txid,
				authority_id,
				is_approved,
			});

			if rollback_request.votes.iter().filter(|v| *v.1).count() as u32
				>= T::Relayers::majority()
			{
				// approve request and move the `PendingRequests`
				rollback_request.is_approved = true;
				<PendingRequests<T>>::insert(
					&psbt_txid,
					PsbtRequest::new(rollback_request.unsigned_psbt.clone(), vec![], true),
				);
				Self::deposit_event(Event::RollbackApproved { txid: psbt_txid });
			}
			<RollbackRequests<T>>::insert(&psbt_txid, rollback_request);

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
					Self::verify_authority(authority_id)?;

					// verify if the signature was originated from the authority_id.
					if !signature.verify(psbt.as_ref(), authority_id) {
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
					if !signature.verify(signed_psbt.as_ref(), authority_id) {
						return InvalidTransaction::BadProof.into();
					}

					ValidTransaction::with_tag_prefix("SignedPsbtSubmission")
						.priority(TransactionPriority::MAX)
						.and_provides(authority_id)
						.propagate(true)
						.build()
				},
				Call::submit_executed_request { msg, signature } => {
					let ExecutedPsbtMessage { authority_id, txid } = msg;
					Self::verify_authority(authority_id)?;

					// verify if the signature was originated from the authority_id.
					if !signature.verify(txid.as_ref(), authority_id) {
						return InvalidTransaction::BadProof.into();
					}

					ValidTransaction::with_tag_prefix("ExecutedPsbtSubmission")
						.priority(TransactionPriority::MAX)
						.and_provides(authority_id)
						.propagate(true)
						.build()
				},
				Call::submit_rollback_poll { msg, signature } => {
					let RollbackPollMessage { authority_id, unsigned_psbt, .. } = msg;

					// verify if the authority is a selected relayer.
					if !T::Relayers::is_authority(&authority_id) {
						return InvalidTransaction::BadSigner.into();
					}

					// verify if the signature was originated from the authority_id.
					if !signature.verify(unsigned_psbt.as_ref(), authority_id) {
						return InvalidTransaction::BadProof.into();
					}

					ValidTransaction::with_tag_prefix("RollbackPollSubmission")
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
