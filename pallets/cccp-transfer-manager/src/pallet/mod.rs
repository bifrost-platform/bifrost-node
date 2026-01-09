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

use bp_cccp::SocketMessage;
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
			option: TransferOption,
			amount: BalanceOf<T>,
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
		/// Submit a transfer request for on-flight approval voting.
		///
		/// This extrinsic handles the initial validation and approval of CCCP transfer requests.
		/// The behavior differs based on whether the transfer is outbound or inbound:
		///
		/// **Outbound transfers (Bifrost → External chain):**
		/// - Validated against on-chain Socket contract state
		/// - Immediately approved without voting (Bifrost is authoritative source)
		/// - Status: Directly to `OnFlight`
		///
		/// **Inbound transfers (External chain → Bifrost):**
		/// - Requires majority consensus from relayers
		/// - First vote creates transfer with `Pending` status
		/// - Subsequent votes update voter list
		/// - Status: `Pending` → `OnFlight` when majority reached
		///
		/// **Fast vs Standard transfers:**
		/// - Transfer option is determined based on available on-flight cap
		/// - If on-flight cap would be exceeded, automatically uses Standard
		/// - Fast transfers update the on-flight cap; Standard transfers do not
		///
		/// **Dynamic cap re-checking (inbound only):**
		/// - Asset cap is re-evaluated when majority is reached
		/// - Cap can change during voting due to other transfers finalizing or approving
		/// - Transfer option (Fast/Standard) is updated based on current cap at majority point
		/// - Example: Standard → Fast if other Fast transfers freed up cap during voting
		/// - Example: Fast → Standard if cap was consumed by other transfers during voting
		///
		/// # Arguments
		/// * `origin` - Must be `None` (unsigned transaction, validated in `validate_unsigned`)
		/// * `socket_message_submission` - Contains authority ID and socket message bytes
		/// * `_signature` - Signature over the message (validated in `validate_unsigned`)
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn on_flight_poll(
			origin: OriginFor<T>,
			socket_message_submission: SocketMessageSubmission<T::AccountId>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			let SocketMessageSubmission { authority_id, message } = socket_message_submission;

			// Parse and validate socket message (must be in REQUESTED status)
			let msg = Self::validate_and_parse_socket_message(&message, |m| m.is_requested())?;
			let amount = msg.params.amount.try_into().map_err(|_| Error::<T>::OutOfRange)?;

			// Get and validate asset information
			let asset_index_hash = AssetIndexHash::from_slice(&msg.params.token_idx0);
			let (asset_id, asset_cap) = Self::get_and_validate_asset(asset_index_hash)?;

			// Ensure transfer hasn't been finalized already
			ensure!(
				!FinalizedTransfers::<T>::contains_key(asset_index_hash, msg.req_id.sequence),
				Error::<T>::TransferAlreadyFinalized
			);
			let on_flight_transfer =
				OnFlightTransfers::<T>::get(asset_index_hash, msg.req_id.sequence);

			// Determine transfer option based on current cap: Fast if cap allows, otherwise Standard
			let transfer_option = Self::determine_transfer_option(&asset_cap, msg.params.amount);

			if msg.is_outbound(<T as pallet_evm::Config>::ChainId::get() as u32) {
				// ============================================================
				// OUTBOUND PATH: Immediate approval (Bifrost is source)
				// ============================================================

				// Validate against on-chain Socket contract state
				let request_info = Self::validate_on_chain_existence(&msg)?;
				ensure!(request_info.is_requested(), Error::<T>::InvalidSocketMessage);

				// Ensure transfer does not exist yet
				ensure!(on_flight_transfer.is_none(), Error::<T>::TransferAlreadyOnFlight);

				// Create transfer with OnFlight status (immediate approval)
				OnFlightTransfers::<T>::insert(
					asset_index_hash,
					msg.req_id.sequence,
					TransferInfo {
						amount,
						option: transfer_option,
						status: TransferStatus::OnFlight,
						socket_message: message.clone(),
						on_flight_voters: BoundedVec::try_from(vec![authority_id.clone()])
							.map_err(|_| Error::<T>::OutOfRange)?,
						finalization_voters: BoundedVec::new(),
					},
				);

				// Update cap for Fast transfers
				if transfer_option == TransferOption::Fast {
					Self::update_fast_transfer_cap(asset_id, asset_cap, msg.params.amount, true)?;
				}

				Self::deposit_event(Event::TransferPolled {
					asset_index_hash,
					sequence_id: msg.req_id.sequence,
					authority_id,
					option: transfer_option,
					amount,
					status: TransferStatus::OnFlight,
				});
			} else {
				// ============================================================
				// INBOUND PATH: Requires voting consensus
				// ============================================================
				//
				// IMPORTANT: Asset cap is dynamically re-checked when majority is reached.
				// During the voting period, the asset cap can change due to:
				// - Other Fast transfers being finalized (freeing cap)
				// - New Fast transfers being approved (consuming cap)
				//
				// Therefore, a transfer initially determined as Standard might become
				// eligible for Fast (or vice versa) by the time majority is reached.
				// The actual transfer option is re-determined at the majority checkpoint.

				if let Some(mut on_flight_transfer) = on_flight_transfer {
					// Subsequent vote on existing transfer
					ensure!(
						on_flight_transfer.status == TransferStatus::Pending,
						Error::<T>::TransferAlreadyOnFlight
					);
					ensure!(
						on_flight_transfer.socket_message == message,
						Error::<T>::InvalidSocketMessage
					);

					// Add voter with double-vote prevention
					Self::add_voter_to_list(
						&mut on_flight_transfer.on_flight_voters,
						&authority_id,
					)?;

					// Check if majority reached → transition to OnFlight
					if on_flight_transfer.on_flight_voters.len() as u32 >= T::Relayers::majority() {
						// Re-check asset cap with latest state to handle cap changes during voting
						let current_asset_cap =
							AssetCaps::<T>::get(asset_id).ok_or(Error::<T>::UnknownAssetAddress)?;
						let actual_transfer_option =
							Self::determine_transfer_option(&current_asset_cap, msg.params.amount);

						// Update transfer option if cap availability changed during voting
						on_flight_transfer.option = actual_transfer_option;
						on_flight_transfer.status = TransferStatus::OnFlight;

						// Update cap for Fast transfers
						if actual_transfer_option == TransferOption::Fast {
							Self::update_fast_transfer_cap(
								asset_id,
								current_asset_cap,
								msg.params.amount,
								true,
							)?;
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
					// First vote: create transfer with Pending status
					OnFlightTransfers::<T>::insert(
						asset_index_hash,
						msg.req_id.sequence,
						TransferInfo {
							amount,
							option: transfer_option,
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
						option: transfer_option,
						amount,
						status: TransferStatus::Pending,
					});
				}
			}

			Ok(().into())
		}

		/// Finalize a transfer by polling finalization status from relayers.
		/// This function must be called after committed/rollbacked status met consensus on the source chain.
		///
		/// This function implements dual-path finalization logic:
		/// - **Outbound transfers** (Bifrost → External): Finalized immediately upon first commit/rollback vote
		/// - **Inbound transfers** (External → Bifrost): Require majority consensus for finalization
		///
		/// # Dual-Path Finalization Architecture
		///
		/// ## Outbound Path (Immediate Finalization)
		/// When a transfer originates from Bifrost to an external chain:
		/// 1. First relayer submits commit/rollback status
		/// 2. Validate status matches on-chain Socket contract state
		/// 3. Immediately finalize without waiting for majority
		/// 4. Update asset cap if Fast transfer
		/// 5. Move to FinalizedTransfers
		///
		/// **Rationale**: Bifrost is the source chain, so the transfer execution is trusted.
		/// Only the final status (committed/rollbacked) needs to be recorded.
		///
		/// ## Inbound Path (Voting-Based Finalization)
		/// When a transfer originates from an external chain to Bifrost:
		/// 1. Each relayer submits commit/rollback vote
		/// 2. Votes are accumulated in `finalization_voters`
		/// 3. When majority is reached, finalize the transfer
		/// 4. Update asset cap if Fast transfer
		/// 5. Move to FinalizedTransfers
		///
		/// **Rationale**: External chains are not trusted, so majority consensus
		/// is required to confirm the transfer was properly committed/rollbacked.
		///
		/// # Validation Steps
		/// 1. Parse and validate socket message (must be COMMITTED or ROLLBACKED)
		/// 2. Verify asset exists and has configured cap
		/// 3. Ensure transfer is in OnFlight status (not Pending or already Finalized)
		/// 4. Verify submitted message matches initial message (except status field)
		/// 5. Validate against on-chain Socket contract state
		/// 6. Prevent double-voting by checking finalization_voters list
		///
		/// # Fast Transfer Cap Management
		/// For Fast transfers, the on-flight cap is decremented when finalized:
		/// - `on_flight_cap -= transfer_amount`
		/// - This frees up capacity for new Fast transfers
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn finalize_poll(
			origin: OriginFor<T>,
			socket_message_submission: SocketMessageSubmission<T::AccountId>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			let SocketMessageSubmission { authority_id, message } = socket_message_submission;

			// Parse and validate socket message (must be COMMITTED or ROLLBACKED)
			let msg = Self::validate_and_parse_socket_message(&message, |msg| {
				msg.is_committed() || msg.is_rollbacked()
			})?;

			// Get and validate asset information
			let asset_index_hash = AssetIndexHash::from_slice(&msg.params.token_idx0);
			let (asset_id, asset_cap) = Self::get_and_validate_asset(asset_index_hash)?;

			// Ensure transfer is not already finalized
			ensure!(
				!FinalizedTransfers::<T>::contains_key(asset_index_hash, msg.req_id.sequence),
				Error::<T>::TransferAlreadyFinalized
			);

			// Get transfer and ensure it's in OnFlight status
			let mut on_flight_transfer =
				OnFlightTransfers::<T>::get(asset_index_hash, msg.req_id.sequence)
					.ok_or(Error::<T>::TransferDNE)?;
			ensure!(
				on_flight_transfer.status == TransferStatus::OnFlight,
				Error::<T>::TransferNotOnFlight
			);

			// Verify submitted message matches initial message (except status field)
			let mut initial_socket_message =
				SocketMessage::try_from(on_flight_transfer.socket_message.clone())
					.map_err(|_| Error::<T>::InvalidSocketMessage)?;
			initial_socket_message.status = msg.status;
			ensure!(initial_socket_message.encode() == message, Error::<T>::InvalidSocketMessage);

			// Validate against on-chain Socket contract state
			let request_info = Self::validate_on_chain_existence(&msg)?;

			// Add voter to finalization voters list (prevents double-voting)
			Self::add_voter_to_list(&mut on_flight_transfer.finalization_voters, &authority_id)?;

			// Outbound path: Immediate finalization
			if msg.is_outbound(<T as pallet_evm::Config>::ChainId::get() as u32) {
				// For outbound, Socket contract must show Committed|Rollbacked status
				ensure!(
					(request_info.is_committed() && msg.is_committed())
						|| (request_info.is_rollbacked() && msg.is_rollbacked()),
					Error::<T>::InvalidSocketMessage
				);

				// Extract transfer option before moving
				let is_fast_transfer = on_flight_transfer.option == TransferOption::Fast;

				// Finalize immediately
				on_flight_transfer.status = TransferStatus::Finalized;
				FinalizedTransfers::<T>::insert(
					asset_index_hash,
					msg.req_id.sequence,
					on_flight_transfer.clone(),
				);
				OnFlightTransfers::<T>::remove(asset_index_hash, msg.req_id.sequence);

				// Update cap for Fast transfers
				if is_fast_transfer {
					Self::update_fast_transfer_cap(
						asset_id,
						asset_cap,
						msg.params.amount,
						false, // subtract from cap
					)?;
				}

				Self::deposit_event(Event::FinalizationPolled {
					asset_index_hash,
					sequence_id: msg.req_id.sequence,
					authority_id,
					option: on_flight_transfer.option,
					amount: on_flight_transfer.amount,
					is_finalized: true,
				});

				return Ok(().into());
			}

			// Inbound path: Voting-based finalization
			// For inbound, Socket contract must show Accepted|Rejected status
			ensure!(
				(request_info.is_accepted() && msg.is_committed())
					|| (request_info.is_rejected() && msg.is_rollbacked()),
				Error::<T>::InvalidSocketMessage
			);

			// Check if majority is reached
			if on_flight_transfer.finalization_voters.len() as u32 >= T::Relayers::majority() {
				// Extract transfer option before moving
				let is_fast_transfer = on_flight_transfer.option == TransferOption::Fast;

				// Finalize with majority consensus
				on_flight_transfer.status = TransferStatus::Finalized;
				FinalizedTransfers::<T>::insert(
					asset_index_hash,
					msg.req_id.sequence,
					on_flight_transfer.clone(),
				);
				OnFlightTransfers::<T>::remove(asset_index_hash, msg.req_id.sequence);

				// Update cap for Fast transfers
				if is_fast_transfer {
					Self::update_fast_transfer_cap(
						asset_id,
						asset_cap,
						msg.params.amount,
						false, // subtract from cap
					)?;
				}

				Self::deposit_event(Event::FinalizationPolled {
					asset_index_hash,
					sequence_id: msg.req_id.sequence,
					authority_id,
					option: on_flight_transfer.option,
					amount: on_flight_transfer.amount,
					is_finalized: true,
				});
			} else {
				// Majority not yet reached - persist vote and wait for more voters
				OnFlightTransfers::<T>::insert(
					asset_index_hash,
					msg.req_id.sequence,
					on_flight_transfer.clone(),
				);

				Self::deposit_event(Event::FinalizationPolled {
					asset_index_hash,
					sequence_id: msg.req_id.sequence,
					authority_id,
					option: on_flight_transfer.option,
					amount: on_flight_transfer.amount,
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
