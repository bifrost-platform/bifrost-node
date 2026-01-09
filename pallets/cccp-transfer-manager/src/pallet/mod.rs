mod impls;

use crate::{
	weights::WeightInfo, AssetCapInfo, AssetId, AssetIndexHash, BalanceOf, SocketMessageSubmission,
	TransferInfo, TransferOption, TransferStatus,
};

use frame_support::{
	pallet_prelude::*,
	traits::{Currency, ReservableCurrency, StorageVersion},
};
use frame_system::pallet_prelude::*;

use bp_cccp::{SocketMessage, UserRequest};
use bp_staking::traits::Authorities;
use sp_core::{H160, U256};
use sp_io::hashing::keccak_256;
use sp_runtime::traits::{Block, Header, IdentifyAccount, Verify};
use sp_std::{fmt::Display, vec};

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_evm::Config {
		/// The currency type
		type Currency: Currency<Self::AccountId> + ReservableCurrency<Self::AccountId>;
		/// The signature signed by the issuer.
		type Signature: Verify<Signer = Self::Signer> + Encode + Decode + Parameter;
		/// The signer of the message.
		type Signer: IdentifyAccount<AccountId = Self::AccountId> + Encode + Decode + MaxEncodedLen;
		/// The Bifrost relayers.
		type Relayers: Authorities<Self::AccountId>;
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The submission is empty.
		EmptySubmission,
		/// The socket message is invalid.
		InvalidSocketMessage,
		/// The request information is invalid.
		InvalidRequestInfo,
		/// The asset index is unknown.
		UnknownAssetIndex,
		/// The asset address is unknown.
		UnknownAssetAddress,
		/// The asset cap is insufficient.
		InsufficientAssetCap,
		/// The socket contract does not exist.
		SocketDNE,
		/// The transfer is already finalized.
		TransferAlreadyFinalized,
		/// The transfer is already on flight.
		TransferAlreadyOnFlight,
		/// The transfer is not on flight.
		TransferNotOnFlight,
		/// The transfer does not exist.
		TransferDNE,
		/// The authority has already voted.
		AlreadyVoted,
		/// The value is out of range.
		OutOfRange,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A transfer has been polled.
		TransferPolled {
			asset_index_hash: AssetIndexHash,
			sequence_id: U256,
			authority_id: T::AccountId,
			option: TransferOption,
			amount: BalanceOf<T>,
			status: TransferStatus,
		},
		/// A finalization has been polled.
		FinalizationPolled {
			asset_index_hash: AssetIndexHash,
			sequence_id: U256,
			authority_id: T::AccountId,
			is_finalized: bool,
		},
	}

	#[pallet::storage]
	/// The `Socket` contract address.
	pub type Socket<T: Config> = StorageValue<_, T::AccountId, OptionQuery>;

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
	impl<T: Config> Pallet<T>
	where
		T::AccountId: Into<H160>,
		BalanceOf<T>: Into<U256> + TryFrom<U256>,
	{
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn on_flight_poll(
			origin: OriginFor<T>,
			socket_message_submission: SocketMessageSubmission<T::AccountId>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			let SocketMessageSubmission { authority_id, message } = socket_message_submission;
			ensure!(!message.is_empty(), Error::<T>::EmptySubmission);

			// the bytes should be a valid socket message
			let msg = SocketMessage::try_from(message.clone())
				.map_err(|_| Error::<T>::InvalidSocketMessage)?;
			ensure!(msg.is_requested(), Error::<T>::InvalidSocketMessage);
			let amount = msg.params.amount.try_into().map_err(|_| Error::<T>::OutOfRange)?;

			// the bridge asset must be in AssetIndexes & AssetCaps
			let asset_index_hash = AssetIndexHash::from_slice(&msg.params.token_idx0);
			let asset_id =
				AssetIndexes::<T>::get(asset_index_hash).ok_or(Error::<T>::UnknownAssetIndex)?;
			let mut asset_cap =
				AssetCaps::<T>::get(asset_id).ok_or(Error::<T>::UnknownAssetAddress)?;

			// the asset cap must be greater than zero
			ensure!(asset_cap.max_on_flight_cap > Zero::zero(), Error::<T>::InsufficientAssetCap);

			// calculate the new on-flight cap
			let cap_after = asset_cap
				.on_flight_cap
				.into()
				.checked_add(msg.params.amount)
				.ok_or(Error::<T>::OutOfRange)?;

			// the transfer must not be finalized yet
			ensure!(
				!FinalizedTransfers::<T>::contains_key(asset_index_hash, msg.req_id.sequence),
				Error::<T>::TransferAlreadyFinalized
			);
			let on_flight_transfer =
				OnFlightTransfers::<T>::get(asset_index_hash, msg.req_id.sequence);

			if msg.is_outbound(<T as pallet_evm::Config>::ChainId::get() as u32) {
				// the message must be valid onchain (only for outbound)
				let msg_hash = Self::hash_bytes(
					&UserRequest::new(msg.ins_code.clone(), msg.params.clone()).encode(),
				);
				let request_info = Self::try_get_request(&msg.encode_req_id())?;
				ensure!(request_info.is_msg_hash(msg_hash), Error::<T>::InvalidSocketMessage);
				ensure!(request_info.is_requested(), Error::<T>::InvalidSocketMessage);
				ensure!(on_flight_transfer.is_none(), Error::<T>::TransferAlreadyOnFlight);

				// if the asset cap is exceeded, the transfer will fallback to standard-transfer.
				let option = if cap_after > asset_cap.max_on_flight_cap.into() {
					TransferOption::Standard
				} else {
					TransferOption::Fast
				};

				OnFlightTransfers::<T>::insert(
					asset_index_hash,
					msg.req_id.sequence,
					TransferInfo {
						amount,
						option,
						status: TransferStatus::OnFlight,
						socket_message: message.clone(),
						on_flight_voters: BoundedVec::try_from(vec![authority_id.clone()])
							.map_err(|_| Error::<T>::OutOfRange)?,
						finalization_voters: BoundedVec::new(),
					},
				);

				// increase the asset cap if fast transfer
				if option == TransferOption::Fast {
					asset_cap.on_flight_cap =
						cap_after.try_into().map_err(|_| Error::<T>::OutOfRange)?;
					AssetCaps::<T>::insert(asset_id, asset_cap);
				}

				Self::deposit_event(Event::TransferPolled {
					asset_index_hash,
					sequence_id: msg.req_id.sequence,
					authority_id,
					option,
					amount,
					status: TransferStatus::OnFlight,
				});
			} else {
				// for inbound requests, we require voting
				if let Some(mut on_flight_transfer) = on_flight_transfer {
					ensure!(
						on_flight_transfer.status == TransferStatus::Pending,
						Error::<T>::TransferAlreadyOnFlight
					);
					ensure!(
						on_flight_transfer.socket_message == message,
						Error::<T>::InvalidSocketMessage
					);
					ensure!(
						!on_flight_transfer.on_flight_voters.contains(&authority_id),
						Error::<T>::AlreadyVoted
					);
					on_flight_transfer
						.on_flight_voters
						.try_push(authority_id.clone())
						.map_err(|_| Error::<T>::OutOfRange)?;

					// check if the transfer is approved
					if on_flight_transfer.on_flight_voters.len() as u32 >= T::Relayers::majority() {
						on_flight_transfer.status = TransferStatus::OnFlight;

						// increase the asset cap if fast transfer
						if on_flight_transfer.option == TransferOption::Fast {
							asset_cap.on_flight_cap =
								cap_after.try_into().map_err(|_| Error::<T>::OutOfRange)?;
							AssetCaps::<T>::insert(asset_id, asset_cap);
						}
					}
					Self::deposit_event(Event::TransferPolled {
						asset_index_hash,
						sequence_id: msg.req_id.sequence,
						authority_id,
						option: on_flight_transfer.option,
						amount: on_flight_transfer.amount,
						status: on_flight_transfer.status,
					});
					OnFlightTransfers::<T>::insert(
						asset_index_hash,
						msg.req_id.sequence,
						on_flight_transfer,
					);
				} else {
					// if the asset cap is exceeded, the transfer will fallback to standard-transfer.
					let option = if cap_after > asset_cap.max_on_flight_cap.into() {
						TransferOption::Standard
					} else {
						TransferOption::Fast
					};
					OnFlightTransfers::<T>::insert(
						asset_index_hash,
						msg.req_id.sequence,
						TransferInfo {
							amount,
							option,
							status: TransferStatus::Pending,
							socket_message: message.clone(),
							on_flight_voters: BoundedVec::try_from(vec![authority_id.clone()])
								.map_err(|_| Error::<T>::OutOfRange)?,
							finalization_voters: BoundedVec::new(),
						},
					);
					Self::deposit_event(Event::TransferPolled {
						asset_index_hash,
						sequence_id: msg.req_id.sequence,
						authority_id,
						option,
						amount,
						status: TransferStatus::Pending,
					});
				}
			}

			Ok(().into())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn finalize_poll(
			origin: OriginFor<T>,
			socket_message_submission: SocketMessageSubmission<T::AccountId>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			let SocketMessageSubmission { authority_id, message } = socket_message_submission;
			ensure!(!message.is_empty(), Error::<T>::EmptySubmission);

			// the bytes should be a valid socket message
			let msg = SocketMessage::try_from(message.clone())
				.map_err(|_| Error::<T>::InvalidSocketMessage)?;
			ensure!(msg.is_committed() || msg.is_rollbacked(), Error::<T>::InvalidSocketMessage);
			let amount = msg.params.amount.try_into().map_err(|_| Error::<T>::OutOfRange)?;

			// the bridge asset must be in AssetIndexes & AssetCaps
			let asset_index_hash = AssetIndexHash::from_slice(&msg.params.token_idx0);
			let asset_id =
				AssetIndexes::<T>::get(asset_index_hash).ok_or(Error::<T>::UnknownAssetIndex)?;
			let mut asset_cap =
				AssetCaps::<T>::get(asset_id).ok_or(Error::<T>::UnknownAssetAddress)?;

			// the transfer must not be finalized yet
			ensure!(
				!FinalizedTransfers::<T>::contains_key(asset_index_hash, msg.req_id.sequence),
				Error::<T>::TransferAlreadyFinalized
			);
			// the transfer must exist in OnFlightTransfers and approved
			let mut on_flight_transfer =
				OnFlightTransfers::<T>::get(asset_index_hash, msg.req_id.sequence)
					.ok_or(Error::<T>::TransferDNE)?;
			ensure!(
				on_flight_transfer.status == TransferStatus::OnFlight,
				Error::<T>::TransferNotOnFlight
			);
			// the submitted socket message must be the same as the one in OnFlightTransfers (except for the status)
			let mut initial_socket_message =
				SocketMessage::try_from(on_flight_transfer.socket_message.clone())
					.map_err(|_| Error::<T>::InvalidSocketMessage)?;
			initial_socket_message.status = msg.status;
			ensure!(initial_socket_message.encode() == message, Error::<T>::InvalidSocketMessage);

			// the message must be valid onchain
			let msg_hash = Self::hash_bytes(
				&UserRequest::new(msg.ins_code.clone(), msg.params.clone()).encode(),
			);
			let request_info = Self::try_get_request(&msg.encode_req_id())?;
			ensure!(request_info.is_msg_hash(msg_hash), Error::<T>::InvalidSocketMessage);

			ensure!(
				!on_flight_transfer.finalization_voters.contains(&authority_id),
				Error::<T>::AlreadyVoted
			);
			on_flight_transfer
				.finalization_voters
				.try_push(authority_id.clone())
				.map_err(|_| Error::<T>::OutOfRange)?;

			if msg.is_outbound(<T as pallet_evm::Config>::ChainId::get() as u32) {
				// for outbound requests, after commit/rollback, the status on Bifrost must be Committed|Rollbacked
				ensure!(
					(request_info.is_committed() && msg.is_committed())
						|| (request_info.is_rollbacked() && msg.is_rollbacked()),
					Error::<T>::InvalidSocketMessage
				);

				// for outbound requests, if the socket message status on Bifrost is committed, the transfer will be finalized immediately.
				on_flight_transfer.status = TransferStatus::Finalized;
				FinalizedTransfers::<T>::insert(
					asset_index_hash,
					msg.req_id.sequence,
					on_flight_transfer.clone(),
				);
				OnFlightTransfers::<T>::remove(asset_index_hash, msg.req_id.sequence);

				if on_flight_transfer.option == TransferOption::Fast {
					// calculate the new on-flight cap
					let cap_after = asset_cap
						.on_flight_cap
						.into()
						.checked_sub(amount)
						.ok_or(Error::<T>::OutOfRange)?;
					asset_cap.on_flight_cap =
						cap_after.try_into().map_err(|_| Error::<T>::OutOfRange)?;
					AssetCaps::<T>::insert(asset_id, asset_cap);
				}

				Self::deposit_event(Event::FinalizationPolled {
					asset_index_hash,
					sequence_id: msg.req_id.sequence,
					authority_id,
					is_finalized: true,
				});
				return Ok(().into());
			} else {
				// for inbound requests, after commit/rollback, the status on Bifrost must be Accepted|Rejected
				ensure!(
					(request_info.is_accepted() && msg.is_committed())
						|| (request_info.is_rejected() && msg.is_rollbacked()),
					Error::<T>::InvalidSocketMessage
				);
			}

			if on_flight_transfer.finalization_voters.len() as u32 >= T::Relayers::majority() {
				on_flight_transfer.status = TransferStatus::Finalized;
				FinalizedTransfers::<T>::insert(
					asset_index_hash,
					msg.req_id.sequence,
					on_flight_transfer.clone(),
				);
				OnFlightTransfers::<T>::remove(asset_index_hash, msg.req_id.sequence);

				if on_flight_transfer.option == TransferOption::Fast {
					// calculate the new on-flight cap
					let cap_after = asset_cap
						.on_flight_cap
						.into()
						.checked_sub(amount)
						.ok_or(Error::<T>::OutOfRange)?;
					asset_cap.on_flight_cap =
						cap_after.try_into().map_err(|_| Error::<T>::OutOfRange)?;
					AssetCaps::<T>::insert(asset_id, asset_cap);
				}

				Self::deposit_event(Event::FinalizationPolled {
					asset_index_hash,
					sequence_id: msg.req_id.sequence,
					authority_id,
					is_finalized: true,
				});
			} else {
				OnFlightTransfers::<T>::insert(
					asset_index_hash,
					msg.req_id.sequence,
					on_flight_transfer.clone(),
				);
				Self::deposit_event(Event::FinalizationPolled {
					asset_index_hash,
					sequence_id: msg.req_id.sequence,
					authority_id,
					is_finalized: false,
				});
			}

			Ok(().into())
		}
	}

	#[pallet::validate_unsigned]
	impl<T: Config> ValidateUnsigned for Pallet<T>
	where
		<<<T as frame_system::Config>::Block as Block>::Header as Header>::Number: Display,
		T::AccountId: Into<H160>,
		BalanceOf<T>: Into<U256> + TryFrom<U256>,
	{
		type Call = Call<T>;

		fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
			match call {
				Call::on_flight_poll { socket_message_submission, signature } => {
					let SocketMessageSubmission { authority_id, message } =
						socket_message_submission;
					Self::verify_authority(authority_id)?;

					// verify if the signature was originated from the authority_id.
					let message =
						[keccak_256("OnFlightPoll".as_bytes()).as_slice(), message].concat();
					if !signature.verify(&*message, authority_id) {
						return InvalidTransaction::BadProof.into();
					}

					ValidTransaction::with_tag_prefix("OnFlightPoll")
						.priority(TransactionPriority::MAX)
						.and_provides((authority_id, signature))
						.propagate(true)
						.build()
				},
				Call::finalize_poll { socket_message_submission, signature } => {
					let SocketMessageSubmission { authority_id, message } =
						socket_message_submission;
					Self::verify_authority(authority_id)?;

					// verify if the signature was originated from the authority_id.
					let message =
						[keccak_256("FinalizePoll".as_bytes()).as_slice(), message].concat();
					if !signature.verify(&*message, authority_id) {
						return InvalidTransaction::BadProof.into();
					}

					ValidTransaction::with_tag_prefix("FinalizePoll")
						.priority(TransactionPriority::MAX)
						.and_provides((authority_id, signature))
						.propagate(true)
						.build()
				},
				_ => InvalidTransaction::Call.into(),
			}
		}
	}
}
