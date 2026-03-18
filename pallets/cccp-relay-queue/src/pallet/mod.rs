mod impls;

use crate::{
	migrations, weights::WeightInfo, AssetCapInfo, AssetId, AssetIndexHash, AssetIndexInfo,
	BalanceOf, ChainId, FinalizePollSubmission, OnFlightPollSubmission, SocketMessageHash,
	SourceTransactionId, TransferInfo, TransferInfoWithTxId, TransferOption,
};

use frame_support::{
	pallet_prelude::*,
	traits::{Currency, OnRuntimeUpgrade, ReservableCurrency, StorageVersion},
};
use frame_system::pallet_prelude::*;

use bp_staking::traits::Authorities;
use sp_core::{H160, U256};
use sp_io::hashing::keccak_256;
use sp_runtime::traits::{Block, Header, IdentifyAccount, Verify};
use sp_std::{fmt::Display, vec, vec::Vec};

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(9);

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
		/// The on-chain existence does not match.
		OnChainExistenceMismatch,
		/// The message hash does not match.
		MessageHashMismatch,
		/// The message status does not match.
		MessageStatusMismatch,
		/// The request information is invalid.
		InvalidRequestInfo,
		/// The socket contract does not exist.
		SocketDNE,
		/// The transfer is already finalized.
		TransferAlreadyFinalized,
		/// The transfer is already on flight.
		TransferAlreadyOnFlight,
		/// The transfer is not on flight.
		TransferNotOnFlight,
		/// The authority has already voted.
		AlreadyVoted,
		/// The value is out of range.
		OutOfRange,
		/// Cannot set the value as identical to the previous value.
		NoWritingSameValue,
		/// The asset already exists.
		AssetAlreadyExists,
		/// The asset index already exists.
		AssetIndexAlreadyExists,
		/// The asset does not exist.
		AssetDNE,
		/// The asset index does not exist.
		AssetIndexDNE,
		/// The asset index does not belong to the asset.
		AssetIndexNotBelongToAsset,
		/// The asset has active on-flight transfers and cannot be removed.
		AssetHasActiveTransfers,
		/// Cannot reduce max_on_flight_cap below current on_flight_cap.
		CapReductionBelowCurrentUsage,
		/// The asset indexes vector is empty.
		EmptyAssetIndexes,
		/// Too many asset indexes in a single call.
		TooManyAssetIndexes,
		/// Duplicate asset index within the same transaction.
		DuplicateAssetIndex,
		/// Conflicting operations on the same asset index (add and remove).
		ConflictingAssetIndexOperation,
		/// Max on-flight cap must be greater than zero.
		InvalidMaxCap,
		/// Max on-flight cap exceeds maximum allowed value.
		CapTooLarge,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A transfer has been polled.
		TransferPolled {
			asset_index_hash: AssetIndexHash,
			sequence_id: U256,
			src_chain_id: ChainId,
			dst_chain_id: ChainId,
			authority_id: T::AccountId,
			option: TransferOption,
			amount: BalanceOf<T>,
			is_approved: bool,
		},
		/// A finalization has been polled.
		FinalizationPolled {
			asset_index_hash: AssetIndexHash,
			sequence_id: U256,
			src_chain_id: ChainId,
			dst_chain_id: ChainId,
			authority_id: T::AccountId,
			option: TransferOption,
			amount: BalanceOf<T>,
			is_finalized: bool,
		},
		/// An asset has been added.
		AssetAdded {
			asset_id: AssetId,
			max_on_flight_cap: BalanceOf<T>,
			asset_indexes: Vec<AssetIndexHash>,
		},
		/// An asset has been removed.
		AssetRemoved { asset_id: AssetId, asset_indexes: Vec<AssetIndexHash> },
		/// An asset has been updated.
		AssetUpdated {
			asset_id: AssetId,
			new_max_on_flight_cap: Option<BalanceOf<T>>,
			add_asset_indexes: Option<Vec<AssetIndexHash>>,
			update_asset_indexes: Option<Vec<AssetIndexHash>>,
			remove_asset_indexes: Option<Vec<AssetIndexHash>>,
		},
		/// The socket has been set.
		SocketSet { new: T::AccountId },
	}

	#[pallet::storage]
	/// The Socket contract address for CCCP message validation.
	///
	/// This stores the on-chain address of the Socket contract that handles cross-chain
	/// message validation. All transfer requests are validated against this contract's state
	/// to ensure authenticity and proper execution status.
	///
	/// - **Type**: `Option<T::AccountId>` - The Socket contract account ID
	/// - **Query**: Returns `None` if not configured, `Some(address)` otherwise
	pub type Socket<T: Config> = StorageValue<_, T::AccountId, OptionQuery>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// Mapping from CCCP asset index hashes to whether the asset index is currently hookable.
	///
	/// - **Key**: `AssetIndexHash` (H256) - The CCCP asset index hash
	/// - **Value**: `bool` - Whether the asset index is currently hookable
	pub type AssetIndexesHookState<T: Config> = StorageMap<_, Twox64Concat, AssetIndexHash, bool>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// Mapping from CCCP asset index hashes to asset addresses.
	///
	/// This storage maps predefined CCCP asset index identifiers to their corresponding
	/// on-chain asset addresses. Asset indexes are standardized identifiers used across
	/// the CCCP protocol to uniquely identify assets in cross-chain transfers.
	///
	/// - **Key**: `AssetIndexHash` (H256) - A predefined 32-byte hash identifying the asset in CCCP
	///   - Example: `BFC_ON_BFC_MAIN` = `0x000000010000000100000bfcffffffffffffffffffffffffffffffffffffffff`
	///   - Format: Combines chain ID, asset type, and asset identifier
	/// - **Value**: `AssetId` (H160) - The EVM-compatible asset contract address
	///   - For native BFC: `0xffffffffffffffffffffffffffffffffffffffff`
	///   - For ERC20 tokens: The actual token contract address (unified token address)
	/// - **Purpose**: Enables validation of cross-chain transfer requests by resolving
	///   CCCP asset indexes to blockchain-specific asset addresses
	pub type AssetIndexes<T: Config> = StorageMap<_, Twox64Concat, AssetIndexHash, AssetId>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// On-flight capacity limits for Fast transfer mode per asset.
	///
	/// This storage tracks the maximum and current on-flight capacity for each asset,
	/// which determines whether a transfer can use Fast mode (immediate bridging) or
	/// must use Standard mode (delayed with additional validation).
	///
	/// - **Key**: `AssetId` (H160) - The asset contract address
	///   - For native BFC: `0xffffffffffffffffffffffffffffffffffffffff`
	///   - For ERC20 tokens: The actual token contract address (unified token address)
	/// - **Value**: `AssetCapInfo<Balance>` containing:
	///   - `max_on_flight_cap`: The maximum total amount allowed in Fast transfers simultaneously
	///   - `on_flight_cap`: Current total amount locked in active Fast transfers
	/// - **Purpose**: Implements risk management by limiting Fast transfer exposure
	///   - When `on_flight_cap + new_transfer_amount <= max_on_flight_cap`: Transfer uses Fast mode
	///   - When capacity exceeded: Transfer automatically falls back to Standard mode
	///   - Cap decreases when Fast transfers are finalized (committed or rolled back)
	pub type AssetCaps<T: Config> =
		StorageMap<_, Twox64Concat, AssetId, AssetCapInfo<BalanceOf<T>>>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// Pending cross-chain transfers awaiting majority consensus approval.
	///
	/// This storage tracks transfers during the initial voting phase before they are approved
	/// for execution. Transfers enter this storage on the first relayer vote and remain here
	/// until majority consensus is reached, at which point they transition to `OnFlightTransfers`.
	///
	/// - **Key 1**: `SocketMessageHash` (H256) - Hash of the REQUESTED socket message (keccak256)
	///   - Uniquely identifies the transfer request across all chains
	///   - Computed from the original CCCP message with `status = REQUESTED (1)`
	/// - **Key 2**: `SourceTransactionId` (H256) - The transaction hash from the source chain
	///   - Enables duplicate detection: same message from different source transactions
	///   - Prevents replay attacks across multiple source chain reorganizations
	/// - **Value**: `TransferInfo<Balance, AccountId>` containing:
	///   - `amount`: Transfer amount in the asset's units
	///   - `sequence_id`: The sequence ID from the socket message request
	///   - `src_chain_id`: The source chain ID where the transfer originated
	///   - `dst_chain_id`: The destination chain ID for the transfer
	///   - `asset_index_hash`: The CCCP asset index identifying the transferred asset
	///   - `option`: Fast or Standard transfer mode (recalculated at each vote)
	///   - `socket_message`: Original CCCP message (status: REQUESTED)
	///   - `on_flight_voters`: Relayers who have voted to approve this transfer
	///
	/// **Transfer Lifecycle**:
	/// 1. **First Vote**: Creates entry in PendingTransfers with single voter
	/// 2. **Subsequent Votes**: Adds voters to `on_flight_voters` list
	/// 3. **Majority Reached**: Transfer moves to `OnFlightTransfers` and is removed from here
	/// 4. **Dynamic Option**: Transfer option (Fast/Standard) is re-evaluated at each vote
	///    based on current asset cap availability
	///
	/// **Purpose**:
	/// - Collects relayer votes before transfer execution approval
	/// - Prevents duplicate processing of the same transfer from different source transactions
	/// - Enables consensus-based security for all cross-chain transfers
	pub type PendingTransfers<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		SocketMessageHash,
		Twox64Concat,
		SourceTransactionId,
		TransferInfo<BalanceOf<T>, T::AccountId>,
	>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// Active cross-chain transfers awaiting finalization.
	///
	/// This storage tracks all transfers that have received majority approval and are now
	/// awaiting finalization (either committed or rolled back on the destination chain).
	/// Transfers move here from `PendingTransfers` after majority consensus, and remain
	/// until finalization consensus is reached.
	///
	/// - **Key**: `SocketMessageHash` (H256) - Hash of the original REQUESTED socket message
	///   - Same hash used in `PendingTransfers` for consistent lookup
	///   - Computed from CCCP message with `status = REQUESTED (1)`
	/// - **Value**: `TransferInfoWithTxId<Balance, AccountId>` containing:
	///   - `amount`: Transfer amount in the asset's units
	///   - `sequence_id`: The sequence ID from the socket message request
	///   - `src_chain_id`: The source chain ID where the transfer originated
	///   - `dst_chain_id`: The destination chain ID for the transfer
	///   - `asset_index_hash`: The CCCP asset index identifying the transferred asset
	///   - `option`: Fast or Standard transfer mode (finalized during approval)
	///   - `socket_message`: Original CCCP message (status: REQUESTED)
	///   - `on_flight_voters`: Complete list of relayers who approved the transfer
	///   - `finalization_voters`: Relayers voting for finalization (grows during this phase)
	///   - `src_tx_id`: Source transaction hash that originated the transfer request
	///
	/// **Transfer Lifecycle**:
	/// 1. **Entry**: Moved here from `PendingTransfers` when on-flight majority reached
	/// 2. **Finalization Voting**: Relayers vote by submitting COMMITTED/ROLLBACKED messages
	/// 3. **Majority Reached**: Transfer moves to `FinalizedTransfers` and is removed from here
	///
	/// **Purpose**:
	/// - Tracks approved transfers awaiting finalization confirmation
	/// - Collects finalization votes from relayers observing destination chain outcome
	/// - Prevents double-finalization by checking for existence in `FinalizedTransfers` first
	/// - For Fast transfers: Holds the on-flight cap until finalization releases it
	pub type OnFlightTransfers<T: Config> = StorageMap<
		_,
		Twox64Concat,
		SocketMessageHash,
		TransferInfoWithTxId<BalanceOf<T>, T::AccountId>,
	>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// Historical record of completed cross-chain transfers.
	///
	/// This storage maintains a permanent record of all transfers that have been finalized,
	/// either committed (successfully executed) or rolled back (cancelled). Transfers are
	/// moved here from `OnFlightTransfers` upon reaching finalization majority consensus.
	///
	/// - **Key**: `SocketMessageHash` (H256) - Hash of the original REQUESTED socket message
	///   - Same hash used throughout the transfer lifecycle for consistent identification
	///   - Enables duplicate detection: prevents reprocessing of already-finalized transfers
	/// - **Value**: `TransferInfoWithTxId<Balance, AccountId>` with complete finalization state:
	///   - `amount`: Transfer amount in the asset's units
	///   - `sequence_id`: The sequence ID from the socket message request
	///   - `src_chain_id`: The source chain ID where the transfer originated
	///   - `dst_chain_id`: The destination chain ID for the transfer
	///   - `asset_index_hash`: The CCCP asset index identifying the transferred asset
	///   - `option`: Fast or Standard transfer mode that was used
	///   - `socket_message`: Original CCCP message (status: REQUESTED)
	///   - `on_flight_voters`: Complete list of relayers who approved the transfer
	///   - `finalization_voters`: Complete list of relayers who confirmed finalization
	///   - `src_tx_id`: Source transaction hash that originated the transfer request
	///
	/// **Purpose**:
	/// - Prevents duplicate transfer processing by checking message hash before accepting new transfers
	/// - Provides permanent historical audit trail for cross-chain transfer operations
	/// - Enables queries for transfer outcomes and voting participation
	/// - Never removed (unbounded storage for permanent record-keeping)
	///
	/// **Note**: The actual finalization outcome (committed vs rolled back) is determined by
	/// querying the Socket contract's final status, not stored in this struct.
	pub type FinalizedTransfers<T: Config> = StorageMap<
		_,
		Twox64Concat,
		SocketMessageHash,
		TransferInfoWithTxId<BalanceOf<T>, T::AccountId>,
	>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T>
	where
		T::AccountId: Into<H160>,
		H160: Into<T::AccountId>,
	{
		fn on_runtime_upgrade() -> Weight {
			migrations::v9::V9::<T>::on_runtime_upgrade()
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T>
	where
		T::AccountId: Into<H160>,
		BalanceOf<T>: Into<U256> + TryFrom<U256>,
	{
		/// Submit a transfer request for on-flight approval voting.
		///
		/// This extrinsic handles the initial validation and consensus-based approval of CCCP
		/// transfer requests. ALL transfers (both inbound and outbound) follow a unified voting
		/// workflow requiring majority relayer consensus before approval.
		///
		/// # Transfer Lifecycle
		///
		/// **Stage 1: Pending (in `PendingTransfers` storage)**
		/// - First relayer vote creates a new entry in `PendingTransfers`
		/// - Subsequent relayer votes are accumulated in the `on_flight_voters` list
		/// - Transfer option (Fast/Standard) is dynamically re-evaluated at each vote
		/// - Keyed by `(msg_hash, src_tx_id)` for duplicate detection
		///
		/// **Stage 2: On-Flight (moved to `OnFlightTransfers` storage)**
		/// - When majority consensus is reached, transfer moves from `PendingTransfers`
		/// - For Fast transfers: on-flight cap is locked at this point
		/// - Transfer awaits finalization (commitment or rollback on destination chain)
		/// - Keyed by `msg_hash` for efficient lookup during finalization
		///
		/// **Stage 3: Finalized (moved to `FinalizedTransfers` storage)**
		/// - Handled by `finalize_poll` extrinsic (separate voting phase)
		///
		/// # Transfer Approval Workflow
		///
		/// ## Outbound Path (Direct Approval)
		/// When a transfer originates from Bifrost to an external chain:
		/// 1. Validate on-chain Socket contract shows REQUESTED (1) status via `get_request()`
		/// 2. **Immediately approve** without waiting for majority vote
		///
		/// **Rationale**: The Socket contract on Bifrost is the authoritative source.
		/// If the contract shows REQUESTED, the transfer is valid.
		///
		/// ## Inbound Path (Voting-Based Approval)
		/// When a transfer originates from an external chain to Bifrost:
		/// 1. Validate on-chain Socket contract shows zero (0) status (not yet registered)
		/// 2. Each relayer casts a vote in `PendingTransfers`
		/// 3. **When majority reached**: Transitions to `OnFlightTransfers`, locks Fast transfer cap
		/// - **First vote**: Creates entry in `PendingTransfers` with single voter
		/// - **Subsequent votes**: Adds voters, prevents double-voting
		///
		/// **Rationale**: Consensus-based approval provides security against Byzantine relayers
		/// for transfers originating from external (untrusted) chains.
		///
		/// # Fast vs Standard Transfer Mode
		///
		/// Transfer mode is dynamically determined based on asset cap availability:
		/// - **Fast**: Used when `on_flight_cap + amount <= max_on_flight_cap`
		/// - **Standard**: Used when cap would be exceeded or asset not registered
		/// - **Dynamic re-evaluation**: Mode recalculated at each vote and at majority point
		///   - Cap can change during voting (other transfers finalizing/approving)
		///   - Example: Standard → Fast if other transfers freed cap during voting
		///   - Example: Fast → Standard if cap consumed by other transfers
		///
		/// # Duplicate Detection
		///
		/// Multiple layers prevent duplicate transfer processing:
		/// - `msg_hash`: Prevents same message from being processed twice
		/// - `src_tx_id`: Distinguishes same message from different source transactions
		/// - Checks against `OnFlightTransfers` and `FinalizedTransfers` before accepting
		///
		/// # Arguments
		/// * `origin` - Must be `None` (unsigned transaction, validated in `validate_unsigned`)
		/// * `on_flight_poll_submission` - Contains:
		///   - `authority_id`: The relayer submitting this vote
		///   - `msg`: Original CCCP socket message (status: REQUESTED)
		///   - `msg_hash`: Keccak256 hash of the message (validated against computed hash)
		///   - `src_tx_id`: Source chain transaction hash that emitted the REQUESTED message
		/// * `_signature` - Signature over `(msg, msg_hash, src_tx_id)` (validated in `validate_unsigned`)
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn on_flight_poll(
			origin: OriginFor<T>,
			on_flight_poll_submission: OnFlightPollSubmission<T::AccountId>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			let OnFlightPollSubmission { authority_id, msg, msg_hash, src_tx_id } =
				on_flight_poll_submission;

			// Parse and validate socket message (must be in REQUESTED status)
			let parsed_msg = Self::validate_and_parse_socket_message(&msg, |m| m.is_requested())?;
			ensure!(msg_hash == Self::hash_bytes(&msg), Error::<T>::MessageHashMismatch);
			ensure!(
				msg_hash == Self::hash_bytes(&parsed_msg.encode()),
				Error::<T>::MessageHashMismatch
			);

			let sequence_id = parsed_msg.req_id.sequence;
			let amount = parsed_msg.params.amount.try_into().map_err(|_| Error::<T>::OutOfRange)?;
			let src_chain_id: ChainId = u32::from_be_bytes(
				parsed_msg
					.req_id
					.chain
					.as_slice()
					.try_into()
					.map_err(|_| Error::<T>::OutOfRange)?,
			);
			let dst_chain_id: ChainId = u32::from_be_bytes(
				parsed_msg
					.ins_code
					.chain
					.as_slice()
					.try_into()
					.map_err(|_| Error::<T>::OutOfRange)?,
			);

			// Get asset information (if registered)
			let asset_index_hash = AssetIndexHash::from_slice(&parsed_msg.params.token_idx0);
			let asset_info = Self::get_asset_info(asset_index_hash);

			// Ensure transfer hasn't been on-flight or finalized already
			ensure!(
				!OnFlightTransfers::<T>::contains_key(msg_hash),
				Error::<T>::TransferAlreadyOnFlight
			);
			ensure!(
				!FinalizedTransfers::<T>::contains_key(msg_hash),
				Error::<T>::TransferAlreadyFinalized
			);

			// Determine transfer option based on current cap:
			// - If asset is registered with cap: Fast if cap allows, otherwise Standard
			// - If asset is not registered: Always Standard (no Fast transfer support)
			let option = if let Some((_, ref asset_cap)) = asset_info {
				Self::determine_transfer_option(asset_cap, parsed_msg.params.amount)?
			} else {
				TransferOption::Standard
			};

			if parsed_msg.is_outbound(<T as pallet_evm::Config>::ChainId::get() as u32) {
				// Outbound: validate params match on-chain state and status must be REQUESTED (1)
				// validate_on_chain_existence cross-checks (ins_code, params) against request_info.msg_hash
				let request_info = Self::validate_on_chain_existence(&parsed_msg)?;
				ensure!(request_info.is_requested(), Error::<T>::MessageStatusMismatch);

				// Update cap for Fast transfers
				if let Some((asset_id, asset_cap)) = asset_info {
					if option == TransferOption::Fast {
						Self::update_fast_transfer_cap(
							asset_id,
							asset_cap,
							parsed_msg.params.amount,
							true,
						)?;
					}
				}

				OnFlightTransfers::<T>::insert(
					msg_hash,
					TransferInfoWithTxId::from_transfer_info(
						TransferInfo {
							amount,
							sequence_id,
							src_chain_id,
							dst_chain_id,
							asset_index_hash,
							option,
							socket_message: msg.clone(),
							on_flight_voters: BoundedVec::try_from(vec![authority_id.clone()])
								.map_err(|_| Error::<T>::OutOfRange)?,
						},
						src_tx_id,
					),
				);

				Self::deposit_event(Event::TransferPolled {
					asset_index_hash,
					sequence_id,
					src_chain_id,
					dst_chain_id,
					authority_id,
					option,
					amount,
					is_approved: true,
				});

				return Ok(().into());
			}

			// ============================================================
			// INBOUND PATH: Requires voting consensus
			// ============================================================
			//
			// Inbound: on-chain status must be zero (request not yet registered on Bifrost).
			// Note: validate_on_chain_existence cannot be used here because the msg_hash
			// field in RequestInfo would be H256::zero() for unregistered requests,
			// causing the content cross-check to fail. Only status validation is needed.
			//
			// IMPORTANT: Asset cap is dynamically re-checked when majority is reached.
			// During the voting period, the asset cap can change due to:
			// - Other Fast transfers being finalized (freeing cap)
			// - New Fast transfers being approved (consuming cap)
			//
			// Therefore, a transfer initially determined as Standard might become
			// eligible for Fast (or vice versa) by the time majority is reached.
			// The actual transfer option is re-determined at the majority checkpoint.
			let request_info = Self::try_get_request(&parsed_msg.encode_req_id())?;
			ensure!(request_info.field[0] == U256::zero(), Error::<T>::MessageStatusMismatch);

			let pending_transfer = PendingTransfers::<T>::get(msg_hash, src_tx_id);

			if let Some(mut pending_transfer) = pending_transfer {
				// Add voter with double-vote prevention
				Self::add_voter_to_list(&mut pending_transfer.on_flight_voters, &authority_id)?;

				// Re-set transfer option based on latest asset cap
				pending_transfer.option = option;

				// Check if majority reached → transition to OnFlight
				if pending_transfer.on_flight_voters.len() as u32 >= T::Relayers::majority() {
					// Update cap for Fast transfers
					if let Some((asset_id, asset_cap)) = asset_info {
						if option == TransferOption::Fast {
							Self::update_fast_transfer_cap(
								asset_id,
								asset_cap,
								parsed_msg.params.amount,
								true,
							)?;
						}
					}

					// Clear every entry with the same msg_hash since the transaction with id=src_tx_id has met consensus
					let _ = PendingTransfers::<T>::clear_prefix(msg_hash, u32::MAX, None);

					// Move to OnFlightTransfers
					OnFlightTransfers::<T>::insert(
						msg_hash,
						TransferInfoWithTxId::from_transfer_info(pending_transfer, src_tx_id),
					);

					Self::deposit_event(Event::TransferPolled {
						asset_index_hash,
						sequence_id,
						src_chain_id,
						dst_chain_id,
						authority_id,
						option,
						amount,
						is_approved: true,
					});

					return Ok(().into());
				}
				PendingTransfers::<T>::insert(msg_hash, src_tx_id, pending_transfer);
			} else {
				// First vote: create transfer with Pending status
				let transfer_info = TransferInfo {
					amount,
					sequence_id,
					src_chain_id,
					dst_chain_id,
					asset_index_hash,
					option,
					socket_message: msg.clone(),
					on_flight_voters: BoundedVec::try_from(vec![authority_id.clone()])
						.map_err(|_| Error::<T>::OutOfRange)?,
				};

				// Check if majority reached immediately (e.g. single validator network)
				if transfer_info.on_flight_voters.len() as u32 >= T::Relayers::majority() {
					// Update cap for Fast transfers
					if let Some((asset_id, asset_cap)) = asset_info {
						if option == TransferOption::Fast {
							Self::update_fast_transfer_cap(
								asset_id,
								asset_cap,
								parsed_msg.params.amount,
								true,
							)?;
						}
					}

					// Move to OnFlightTransfers
					OnFlightTransfers::<T>::insert(
						msg_hash,
						TransferInfoWithTxId::from_transfer_info(transfer_info, src_tx_id),
					);

					Self::deposit_event(Event::TransferPolled {
						asset_index_hash,
						sequence_id,
						src_chain_id,
						dst_chain_id,
						authority_id,
						option,
						amount,
						is_approved: true,
					});

					return Ok(().into());
				}

				PendingTransfers::<T>::insert(msg_hash, src_tx_id, transfer_info);
			}
			Self::deposit_event(Event::TransferPolled {
				asset_index_hash,
				sequence_id,
				src_chain_id,
				dst_chain_id,
				authority_id,
				option,
				amount,
				is_approved: false,
			});

			Ok(().into())
		}

		/// Finalize a transfer by polling finalization status from relayers.
		///
		/// This extrinsic handles the final stage of cross-chain transfers after execution
		/// on the destination chain. It implements dual-path finalization logic based on
		/// transfer direction, with outbound getting immediate finalization and inbound
		/// requiring majority consensus.
		///
		/// # Transfer Lookup via Hash Reconstruction
		///
		/// Finalization messages (COMMITTED/ROLLBACKED) are linked back to the original
		/// transfer by reconstructing the REQUESTED message hash:
		/// 1. Clone the finalization message
		/// 2. Set status field to `1` (REQUESTED)
		/// 3. Compute `msg_hash = keccak256(message.encode())`
		/// 4. Look up transfer in `OnFlightTransfers` using this hash
		///
		/// This enables consistent transfer identification across different message statuses
		/// while maintaining the hash-based storage architecture.
		///
		/// # Dual-Path Finalization Architecture
		///
		/// ## Outbound Path (Immediate Finalization)
		/// When a transfer originates from Bifrost to an external chain:
		/// 1. First relayer submits COMMITTED or ROLLBACKED message
		/// 2. Validate status matches on-chain Socket contract state
		/// 3. **Immediately finalize** without waiting for majority
		/// 4. Update asset cap if Fast transfer (frees locked capacity)
		/// 5. Move from `OnFlightTransfers` to `FinalizedTransfers`
		///
		/// **Rationale**: Bifrost is the source chain with authoritative execution state.
		/// Only the final status needs to be recorded for audit trail.
		///
		/// ## Inbound Path (Voting-Based Finalization)
		/// When a transfer originates from an external chain to Bifrost:
		/// 1. Each relayer submits COMMITTED or ROLLBACKED vote
		/// 2. Votes accumulate in `finalization_voters` list (with double-vote prevention)
		/// 3. **When majority reached**, finalize the transfer
		/// 4. Update asset cap if Fast transfer (frees locked capacity)
		/// 5. Move from `OnFlightTransfers` to `FinalizedTransfers`
		///
		/// **Rationale**: External chains are untrusted. Majority consensus required to
		/// confirm the transfer was properly executed or rolled back on destination chain.
		///
		/// # Socket Contract State Validation
		///
		/// **Outbound**: Socket contract must show COMMITTED or ROLLBACKED status
		/// - `request_info.is_committed() && msg.is_committed()`, OR
		/// - `request_info.is_rollbacked() && msg.is_rollbacked()`
		///
		/// **Inbound**: Socket contract must show ACCEPTED (5) or REJECTED (6) status
		/// - `request_info.is_accepted()` (transfer accepted on destination chain), OR
		/// - `request_info.is_rejected()` (transfer rejected on destination chain)
		///
		/// # Fast Transfer Cap Management
		///
		/// For Fast transfers, the on-flight cap is released upon finalization:
		/// - `on_flight_cap -= transfer_amount`
		/// - Frees capacity for new Fast transfers
		/// - Applied for both COMMITTED and ROLLBACKED outcomes
		///
		/// # Arguments
		/// * `origin` - Must be `None` (unsigned transaction, validated in `validate_unsigned`)
		/// * `finalize_poll_submission` - Contains:
		///   - `authority_id`: The relayer submitting this finalization vote
		///   - `msg`: CCCP socket message with status COMMITTED or ROLLBACKED
		/// * `_signature` - Signature over the message (validated in `validate_unsigned`)
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn finalize_poll(
			origin: OriginFor<T>,
			finalize_poll_submission: FinalizePollSubmission<T::AccountId>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			let FinalizePollSubmission { authority_id, msg } = finalize_poll_submission;

			// Parse and validate socket message (must be COMMITTED or ROLLBACKED or ACCEPTED or REJECTED)
			let parsed_msg = Self::validate_and_parse_socket_message(&msg, |msg| {
				msg.is_committed() || msg.is_rollbacked() || msg.is_accepted() || msg.is_rejected()
			})?;
			let sequence_id = parsed_msg.req_id.sequence;
			let src_chain_id: ChainId = u32::from_be_bytes(
				parsed_msg
					.req_id
					.chain
					.as_slice()
					.try_into()
					.map_err(|_| Error::<T>::OutOfRange)?,
			);
			let dst_chain_id: ChainId = u32::from_be_bytes(
				parsed_msg
					.ins_code
					.chain
					.as_slice()
					.try_into()
					.map_err(|_| Error::<T>::OutOfRange)?,
			);

			// Get asset information (if registered)
			let asset_index_hash = AssetIndexHash::from_slice(&parsed_msg.params.token_idx0);
			let asset_info = Self::get_asset_info(asset_index_hash);

			// Generate the initial socket message hash for the transfer (status: REQUESTED)
			let mut msg_cloned = parsed_msg.clone();
			msg_cloned.status = U256::from(1);
			let msg_hash = Self::hash_bytes(&msg_cloned.encode());

			// Ensure transfer is not already finalized
			ensure!(
				!FinalizedTransfers::<T>::contains_key(msg_hash),
				Error::<T>::TransferAlreadyFinalized
			);

			// Get transfer and ensure it's in OnFlight status
			let mut on_flight_transfer =
				OnFlightTransfers::<T>::get(msg_hash).ok_or(Error::<T>::TransferNotOnFlight)?;

			// Validate against on-chain Socket contract state
			let request_info = Self::validate_on_chain_existence(&parsed_msg)?;

			// Add voter to finalization voters list (prevents double-voting)
			Self::add_voter_to_list(&mut on_flight_transfer.finalization_voters, &authority_id)?;

			// Outbound path: Immediate finalization
			if parsed_msg.is_outbound(<T as pallet_evm::Config>::ChainId::get() as u32) {
				// For outbound, Socket contract must show Committed|Rollbacked status
				ensure!(
					(request_info.is_committed() && parsed_msg.is_committed())
						|| (request_info.is_rollbacked() && parsed_msg.is_rollbacked()),
					Error::<T>::MessageStatusMismatch
				);

				// Extract transfer option before moving
				let is_fast_transfer = on_flight_transfer.option == TransferOption::Fast;

				// Finalize immediately
				OnFlightTransfers::<T>::remove(msg_hash);
				FinalizedTransfers::<T>::insert(msg_hash, on_flight_transfer.clone());

				// Update cap for Fast transfers (only if asset is registered)
				if is_fast_transfer {
					if let Some((asset_id, asset_cap)) = asset_info {
						Self::update_fast_transfer_cap(
							asset_id,
							asset_cap,
							parsed_msg.params.amount,
							false, // subtract from cap
						)?;
					}
				}

				Self::deposit_event(Event::FinalizationPolled {
					asset_index_hash,
					sequence_id,
					src_chain_id,
					dst_chain_id,
					authority_id,
					option: on_flight_transfer.option,
					amount: on_flight_transfer.amount,
					is_finalized: true,
				});

				return Ok(().into());
			}

			// Inbound path: Voting-based finalization
			// For inbound, Socket contract must show Accepted (5) or Rejected (6) status
			ensure!(
				(request_info.is_accepted()
					&& (parsed_msg.is_accepted() || parsed_msg.is_committed()))
					|| (request_info.is_rejected()
						&& (parsed_msg.is_rejected() || parsed_msg.is_rollbacked())),
				Error::<T>::MessageStatusMismatch
			);

			// Check if majority is reached
			if on_flight_transfer.finalization_voters.len() as u32 >= T::Relayers::majority() {
				// Extract transfer option before moving
				let is_fast_transfer = on_flight_transfer.option == TransferOption::Fast;

				// Finalize with majority consensus
				OnFlightTransfers::<T>::remove(msg_hash);
				FinalizedTransfers::<T>::insert(msg_hash, on_flight_transfer.clone());

				// Update cap for Fast transfers (only if asset is registered)
				if is_fast_transfer {
					if let Some((asset_id, asset_cap)) = asset_info {
						Self::update_fast_transfer_cap(
							asset_id,
							asset_cap,
							parsed_msg.params.amount,
							false, // subtract from cap
						)?;
					}
				}

				Self::deposit_event(Event::FinalizationPolled {
					asset_index_hash,
					sequence_id,
					src_chain_id,
					dst_chain_id,
					authority_id,
					option: on_flight_transfer.option,
					amount: on_flight_transfer.amount,
					is_finalized: true,
				});
			} else {
				// Majority not yet reached - persist vote and wait for more voters
				OnFlightTransfers::<T>::insert(msg_hash, on_flight_transfer.clone());

				Self::deposit_event(Event::FinalizationPolled {
					asset_index_hash,
					sequence_id,
					src_chain_id,
					dst_chain_id,
					authority_id,
					option: on_flight_transfer.option,
					amount: on_flight_transfer.amount,
					is_finalized: false,
				});
			}

			Ok(().into())
		}

		/// Set or update the Socket contract address for CCCP message validation.
		///
		/// This extrinsic configures the on-chain Socket contract address used to validate
		/// all cross-chain transfer requests. The Socket contract maintains the authoritative
		/// state of transfer requests and their execution status.
		///
		/// # Parameters
		/// * `origin` - Must be `Root` (sudo access required)
		/// * `new` - The new Socket contract account ID to set
		///
		/// # Errors
		/// * `NoWritingSameValue` - If the new address is identical to the current address
		///
		/// # Events
		/// * `SocketSet { new }` - Emitted when the Socket address is successfully updated
		///
		/// # Important
		/// - This is a critical configuration parameter affecting all transfer validations
		/// - Changing the Socket address will redirect all future validations to the new contract
		/// - Ensure the new contract is properly deployed and initialized before setting
		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn set_socket(origin: OriginFor<T>, new: T::AccountId) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;

			if let Some(old) = <Socket<T>>::get() {
				ensure!(old != new, Error::<T>::NoWritingSameValue);
			}
			<Socket<T>>::put(new.clone());
			Self::deposit_event(Event::SocketSet { new });

			Ok(().into())
		}

		/// Register a new asset with Fast transfer mode support.
		///
		/// This extrinsic registers an asset for cross-chain transfers, configuring its
		/// maximum on-flight capacity and associating CCCP asset indexes with it. Assets
		/// registered through this extrinsic can utilize Fast transfer mode when capacity
		/// allows.
		///
		/// # Parameters
		/// * `origin` - Must be `Root` (sudo access required)
		/// * `asset_id` - The EVM-compatible asset contract address (H160)
		///   - For native BFC: `0xffffffffffffffffffffffffffffffffffffffff`
		///   - For ERC20 tokens: The unified token contract address
		/// * `max_on_flight_cap` - Maximum total amount allowed in Fast transfers simultaneously
		///   - Must be > 0 (cannot create non-functional assets)
		///   - Must be ≤ 100,000,000 * 10^18 (100M cap limit)
		///   - When exceeded, transfers automatically fall back to Standard mode
		/// * `asset_indexes` - Vector of CCCP asset index hashes (H256) to associate with this asset
		///   - Must contain at least 1 index (cannot be empty)
		///   - Maximum 100 indexes per call (DoS protection)
		///   - No duplicates allowed within the call
		///   - Format: Combines chain ID, asset type, and asset identifier
		///   - Example: `BFC_ON_BFC_MAIN` = `0x000000010000000100000bfcffffffffffffffffffffffffffffffffffffffff`
		///
		/// # Errors
		/// * `EmptyAssetIndexes` - If `asset_indexes` vector is empty
		/// * `TooManyAssetIndexes` - If more than 100 asset indexes provided
		/// * `InvalidMaxCap` - If `max_on_flight_cap` is zero
		/// * `CapTooLarge` - If `max_on_flight_cap` exceeds 100M limit
		/// * `DuplicateAssetIndex` - If duplicate indexes exist within the call
		/// * `AssetAlreadyExists` - If the asset_id is already registered
		/// * `AssetIndexAlreadyExists` - If any asset index is already associated with another asset
		///
		/// # Events
		/// * `AssetAdded { asset_id, max_on_flight_cap, asset_indexes }` - Emitted on successful registration
		///
		/// # Storage Modifications
		/// - `AssetCaps`: Inserts new entry with `max_on_flight_cap` and `on_flight_cap = 0`
		/// - `AssetIndexes`: Inserts mapping from each asset index to the asset_id
		///
		/// # Important
		/// - All validations are performed BEFORE any storage modifications (atomic operation)
		/// - The `on_flight_cap` is initialized to 0 and increases/decreases with Fast transfers
		/// - Asset indexes enable CCCP protocol to resolve cross-chain transfer requests
		/// - Once registered, use `update_asset` to modify capacity or indexes
		#[pallet::call_index(3)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn add_asset(
			origin: OriginFor<T>,
			asset_id: AssetId,
			max_on_flight_cap: BalanceOf<T>,
			asset_indexes: Vec<AssetIndexInfo>,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;

			// Validate asset_indexes is not empty
			ensure!(!asset_indexes.is_empty(), Error::<T>::EmptyAssetIndexes);

			ensure!(
				asset_indexes.len() <= crate::MAX_ASSET_INDEXES_PER_CALL,
				Error::<T>::TooManyAssetIndexes
			);

			// Validate max_on_flight_cap > 0
			ensure!(max_on_flight_cap > Default::default(), Error::<T>::InvalidMaxCap);

			// Upper bound validation (100M cap limit)
			let max_cap_u128: u128 =
				max_on_flight_cap.try_into().map_err(|_| Error::<T>::OutOfRange)?;
			ensure!(max_cap_u128 <= crate::MAX_ON_FLIGHT_CAP, Error::<T>::CapTooLarge);

			// Check for duplicates within same call
			let mut seen = sp_std::collections::btree_set::BTreeSet::new();
			for asset_index in &asset_indexes {
				ensure!(seen.insert(asset_index.hash), Error::<T>::DuplicateAssetIndex);
			}

			// Validate asset doesn't already exist
			ensure!(!AssetCaps::<T>::contains_key(asset_id), Error::<T>::AssetAlreadyExists);

			// Validate ALL indexes before ANY storage modifications (atomicity)
			for asset_index in &asset_indexes {
				ensure!(
					!AssetIndexes::<T>::contains_key(asset_index.hash),
					Error::<T>::AssetIndexAlreadyExists
				);
			}

			// All validations passed - now safe to insert
			AssetCaps::<T>::insert(
				asset_id,
				AssetCapInfo { max_on_flight_cap, on_flight_cap: Default::default() },
			);
			for asset_index in &asset_indexes {
				AssetIndexes::<T>::insert(asset_index.hash, asset_id);
			}
			for asset_index in &asset_indexes {
				AssetIndexesHookState::<T>::insert(asset_index.hash, asset_index.is_hookable);
			}

			Self::deposit_event(Event::AssetAdded {
				asset_id,
				max_on_flight_cap,
				asset_indexes: asset_indexes.iter().map(|index| index.hash).collect(),
			});

			Ok(().into())
		}

		/// Unregister an asset and its associated CCCP asset indexes.
		///
		/// This extrinsic removes an asset from the cross-chain transfer registry. It also
		/// removes all CCCP asset indexes that were mapped to this asset.
		///
		/// # Parameters
		/// * `origin` - Must be `Root` (sudo access required)
		/// * `asset_id` - The EVM-compatible asset contract address (H160) to remove
		///
		/// # Errors
		/// * `AssetDNE` - If the asset is not registered
		/// * `AssetHasActiveTransfers` - If any of the associated asset indexes have active on-flight transfers
		///
		/// # Events
		/// * `AssetRemoved { asset_id, asset_indexes }` - Emitted on successful removal
		///
		/// # Storage Modifications
		/// - `AssetCaps`: Entry for `asset_id` is removed
		/// - `AssetIndexes`: All mappings for the associated asset indexes are removed
		///
		/// # Important
		/// - An asset cannot be removed if it has any active on-flight transfers. This ensures
		///   system consistency and prevents orphaned transfers.
		#[pallet::call_index(4)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn remove_asset(origin: OriginFor<T>, asset_id: AssetId) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;

			ensure!(AssetCaps::<T>::contains_key(asset_id), Error::<T>::AssetDNE);

			// Collect all asset indexes for this asset
			let asset_indexes: Vec<AssetIndexHash> = AssetIndexes::<T>::iter()
				.filter(|(_, v)| *v == asset_id)
				.map(|(k, _)| k)
				.collect();

			// Check if any of the asset indexes have active on-flight transfers
			// Pending transfers are intentionally not blocked here:
			// if the asset is de-registered before consensus, the transfer can still
			// complete later, but it will fall back to Standard because Fast mode
			// requires the asset to remain registered in storage.
			let asset_index_set: sp_std::collections::btree_set::BTreeSet<_> =
				asset_indexes.iter().collect();
			let has_active_transfers = OnFlightTransfers::<T>::iter().any(|(_, transfer_info)| {
				asset_index_set.contains(&transfer_info.asset_index_hash)
			});
			ensure!(!has_active_transfers, Error::<T>::AssetHasActiveTransfers);

			// Safe to remove asset and its indexes
			AssetCaps::<T>::remove(asset_id);
			for asset_index in &asset_indexes {
				AssetIndexes::<T>::remove(asset_index);
				AssetIndexesHookState::<T>::remove(asset_index);
			}

			Self::deposit_event(Event::AssetRemoved { asset_id, asset_indexes });

			Ok(().into())
		}

		/// Update an existing asset's configuration and CCCP asset indexes.
		///
		/// This extrinsic allows modifying the maximum on-flight capacity of a registered
		/// asset and managing its associated CCCP asset indexes by adding or removing them.
		///
		/// # Parameters
		/// * `origin` - Must be `Root` (sudo access required)
		/// * `asset_id` - The EVM-compatible asset contract address (H160) to update
		/// * `new_max_on_flight_cap` - (Optional) New maximum total amount allowed in Fast transfers
		///   - Must be > 0
		///   - Must be ≤ 100M cap limit
		///   - Cannot be less than the current `on_flight_cap`
		/// * `add_asset_indexes` - (Optional) Vector of new asset indexes to associate with the asset
		///   - Maximum 100 indexes per call
		///   - No duplicates allowed within the call
		///   - Each index must not already be registered
		/// * `update_asset_indexes` - (Optional) Vector of existing asset indexes to update
		///   - Maximum 100 indexes per call
		///   - No duplicates allowed within the call
		///   - Each index must already be registered
		///   - Updates the `is_hookable` flag of the index
		/// * `remove_asset_indexes` - (Optional) Vector of CCCP asset index hashes to disassociate
		///   - Maximum 100 indexes per call
		///   - Each index must belong to the specified `asset_id`
		///   - Index must not have active on-flight transfers
		///
		/// # Errors
		/// * `EmptySubmission` - If no update fields are provided
		/// * `AssetDNE` - If the asset is not registered
		/// * `TooManyAssetIndexes` - If more than 100 asset indexes provided in any list
		/// * `DuplicateAssetIndex` - If duplicate indexes exist within any list
		/// * `ConflictingAssetIndexOperation` - If an index is present in both add and remove lists
		/// * `NoWritingSameValue` - If the new cap is identical to the current cap
		/// * `InvalidMaxCap` - If the new cap is zero
		/// * `CapTooLarge` - If the new cap exceeds 100M limit
		/// * `CapReductionBelowCurrentUsage` - If the new cap is less than the current on-flight usage
		/// * `AssetIndexAlreadyExists` - If a new index to add is already registered
		/// * `AssetIndexDNE` - If an index to update or remove is not registered
		/// * `AssetIndexNotBelongToAsset` - If an index to remove belongs to a different asset
		/// * `AssetHasActiveTransfers` - If an index to remove has active on-flight transfers
		///
		/// # Events
		/// * `AssetUpdated { asset_id, new_max_on_flight_cap, add_asset_indexes, update_asset_indexes, remove_asset_indexes }`
		///
		/// # Storage Modifications
		/// - `AssetCaps`: Updates `max_on_flight_cap` for the asset if provided
		/// - `AssetIndexes`: Inserts new mappings and removes specified mappings
		/// - `AssetIndexesHookState`: Inserts, updates, and removes hookable state alongside `AssetIndexes`
		#[pallet::call_index(5)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn update_asset(
			origin: OriginFor<T>,
			asset_id: AssetId,
			new_max_on_flight_cap: Option<BalanceOf<T>>,
			add_asset_indexes: Option<Vec<AssetIndexInfo>>,
			update_asset_indexes: Option<Vec<AssetIndexInfo>>,
			remove_asset_indexes: Option<Vec<AssetIndexHash>>,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;

			// Ensure at least one of the fields is provided
			ensure!(
				new_max_on_flight_cap.is_some()
					|| add_asset_indexes.is_some()
					|| update_asset_indexes.is_some()
					|| remove_asset_indexes.is_some(),
				Error::<T>::EmptySubmission
			);

			// Bounded vector sizes (DoS protection)
			if let Some(ref indexes) = add_asset_indexes {
				ensure!(
					indexes.len() <= crate::MAX_ASSET_INDEXES_PER_CALL,
					Error::<T>::TooManyAssetIndexes
				);
			}
			if let Some(ref indexes) = update_asset_indexes {
				ensure!(
					indexes.len() <= crate::MAX_ASSET_INDEXES_PER_CALL,
					Error::<T>::TooManyAssetIndexes
				);
			}
			if let Some(ref indexes) = remove_asset_indexes {
				ensure!(
					indexes.len() <= crate::MAX_ASSET_INDEXES_PER_CALL,
					Error::<T>::TooManyAssetIndexes
				);
			}

			// Check for duplicates within add_asset_indexes
			if let Some(ref indexes) = add_asset_indexes {
				let mut seen = sp_std::collections::btree_set::BTreeSet::new();
				for asset_index in indexes {
					ensure!(seen.insert(asset_index.hash), Error::<T>::DuplicateAssetIndex);
				}
			}
			if let Some(ref indexes) = update_asset_indexes {
				let mut seen = sp_std::collections::btree_set::BTreeSet::new();
				for asset_index in indexes {
					ensure!(seen.insert(asset_index.hash), Error::<T>::DuplicateAssetIndex);
				}
			}
			// Check for duplicates within remove_asset_indexes
			if let Some(ref indexes) = remove_asset_indexes {
				let mut seen = sp_std::collections::btree_set::BTreeSet::new();
				for asset_index in indexes {
					ensure!(seen.insert(asset_index), Error::<T>::DuplicateAssetIndex);
				}
			}

			// Check for conflicts between add and remove
			if let (Some(ref add), Some(ref remove)) = (&add_asset_indexes, &remove_asset_indexes) {
				for add_idx in add {
					ensure!(
						!remove.contains(&add_idx.hash),
						Error::<T>::ConflictingAssetIndexOperation
					);
				}
			}
			// Check for conflicts between update and remove
			if let (Some(ref update), Some(ref remove)) =
				(&update_asset_indexes, &remove_asset_indexes)
			{
				for upd_idx in update {
					ensure!(
						!remove.contains(&upd_idx.hash),
						Error::<T>::ConflictingAssetIndexOperation
					);
				}
			}

			let mut asset_cap = AssetCaps::<T>::get(asset_id).ok_or(Error::<T>::AssetDNE)?;

			if let Some(new_max_on_flight_cap) = new_max_on_flight_cap {
				if asset_cap.max_on_flight_cap == new_max_on_flight_cap {
					return Err(Error::<T>::NoWritingSameValue.into());
				}
				// Validate max_on_flight_cap > 0
				ensure!(new_max_on_flight_cap > Default::default(), Error::<T>::InvalidMaxCap);

				// Upper bound validation (100M cap limit)
				let max_cap_u128: u128 =
					new_max_on_flight_cap.try_into().map_err(|_| Error::<T>::OutOfRange)?;
				ensure!(max_cap_u128 <= crate::MAX_ON_FLIGHT_CAP, Error::<T>::CapTooLarge);

				// Prevent reducing cap below current usage
				ensure!(
					new_max_on_flight_cap >= asset_cap.on_flight_cap,
					Error::<T>::CapReductionBelowCurrentUsage
				);
				asset_cap.max_on_flight_cap = new_max_on_flight_cap;
				AssetCaps::<T>::insert(asset_id, asset_cap);
			}
			if let Some(add_asset_indexes) = &add_asset_indexes {
				for asset_index in add_asset_indexes {
					ensure!(
						!AssetIndexes::<T>::contains_key(asset_index.hash),
						Error::<T>::AssetIndexAlreadyExists
					);
					ensure!(
						!AssetIndexesHookState::<T>::contains_key(asset_index.hash),
						Error::<T>::AssetIndexAlreadyExists
					);
					AssetIndexes::<T>::insert(asset_index.hash, asset_id);
					AssetIndexesHookState::<T>::insert(asset_index.hash, asset_index.is_hookable);
				}
			}
			if let Some(update_asset_indexes) = &update_asset_indexes {
				for asset_index in update_asset_indexes {
					ensure!(
						AssetIndexes::<T>::contains_key(asset_index.hash),
						Error::<T>::AssetIndexDNE
					);
					ensure!(
						AssetIndexesHookState::<T>::contains_key(asset_index.hash),
						Error::<T>::AssetIndexDNE
					);
					AssetIndexesHookState::<T>::insert(asset_index.hash, asset_index.is_hookable);
				}
			}
			if let Some(remove_asset_indexes) = &remove_asset_indexes {
				for asset_index in remove_asset_indexes {
					let found_asset_id =
						AssetIndexes::<T>::get(asset_index).ok_or(Error::<T>::AssetIndexDNE)?;
					ensure!(found_asset_id == asset_id, Error::<T>::AssetIndexNotBelongToAsset);
				}

				// Prevent removing asset indexes with active transfers
				// Only on-flight transfers are blocked on purpose. Pending votes can
				// still complete after the asset is removed, but they will no longer
				// qualify for Fast mode because get_asset_info() will return None.
				let remove_index_set: sp_std::collections::btree_set::BTreeSet<_> =
					remove_asset_indexes.iter().collect();
				let has_active_transfers =
					OnFlightTransfers::<T>::iter().any(|(_, transfer_info)| {
						remove_index_set.contains(&transfer_info.asset_index_hash)
					});
				ensure!(!has_active_transfers, Error::<T>::AssetHasActiveTransfers);

				for asset_index in remove_asset_indexes {
					AssetIndexes::<T>::remove(asset_index);
					AssetIndexesHookState::<T>::remove(asset_index);
				}
			}

			Self::deposit_event(Event::AssetUpdated {
				asset_id,
				new_max_on_flight_cap,
				add_asset_indexes: add_asset_indexes
					.map(|indexes| indexes.iter().map(|index| index.hash).collect()),
				update_asset_indexes: update_asset_indexes
					.map(|indexes| indexes.iter().map(|index| index.hash).collect()),
				remove_asset_indexes,
			});

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
				Call::on_flight_poll { on_flight_poll_submission, signature } => {
					let OnFlightPollSubmission { authority_id, msg, msg_hash, src_tx_id } =
						on_flight_poll_submission;
					Self::verify_authority(authority_id)?;

					// verify if the signature was originated from the authority_id.
					let message = [
						keccak_256("OnFlightPoll".as_bytes()).as_slice(),
						Encode::encode(&(msg, msg_hash, src_tx_id)).as_slice(),
					]
					.concat();
					if !signature.verify(&*message, authority_id) {
						return InvalidTransaction::BadProof.into();
					}

					ValidTransaction::with_tag_prefix("OnFlightPoll")
						.priority(TransactionPriority::MAX)
						.and_provides((authority_id, signature))
						.propagate(true)
						.build()
				},
				Call::finalize_poll { finalize_poll_submission, signature } => {
					let FinalizePollSubmission { authority_id, msg } = finalize_poll_submission;
					Self::verify_authority(authority_id)?;

					// verify if the signature was originated from the authority_id.
					let message =
						[keccak_256("FinalizePoll".as_bytes()).as_slice(), msg.as_slice()].concat();
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
