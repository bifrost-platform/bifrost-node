mod impls;

use crate::{
	weights::WeightInfo, AssetCapInfo, AssetId, AssetIndexHash, AssetOracleId, BalanceOf, ChainId,
	SocketMessageSubmission, TransferInfo, TransferOption, TransferStatus,
};

use frame_support::{
	pallet_prelude::*,
	traits::{Currency, ReservableCurrency, StorageVersion},
};
use frame_system::pallet_prelude::*;

use bp_cccp::SocketMessage;
use bp_staking::traits::Authorities;
use sp_core::{H160, H256, U256};
use sp_io::hashing::keccak_256;
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
		/// Native currency chain already exists.
		NativeCurrencyChainAlreadyExists,
		/// Native currency chain does not exist.
		NativeCurrencyChainDNE,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A transfer has been polled.
		TransferPolled {
			asset_index_hash: AssetIndexHash,
			sequence_id: U256,
			src_tx_id: H256,
			src_chain_id: ChainId,
			dst_chain_id: ChainId,
			authority_id: T::AccountId,
			option: TransferOption,
			amount: BalanceOf<T>,
			status: TransferStatus,
		},
		/// A finalization has been polled.
		FinalizationPolled {
			asset_index_hash: AssetIndexHash,
			sequence_id: U256,
			src_tx_id: H256,
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
			asset_oracle_id: AssetOracleId,
			max_on_flight_cap: BalanceOf<T>,
			asset_indexes: Vec<AssetIndexHash>,
		},
		/// An asset has been removed.
		AssetRemoved { asset_id: AssetId, asset_indexes: Vec<AssetIndexHash> },
		/// An asset has been updated.
		AssetUpdated {
			asset_id: AssetId,
			new_asset_oracle_id: Option<AssetOracleId>,
			new_max_on_flight_cap: Option<BalanceOf<T>>,
			add_asset_indexes: Option<Vec<AssetIndexHash>>,
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
	/// Mapping from asset addresses to their oracle addresses.
	///
	/// This storage maps EVM-compatible asset contract addresses to their corresponding
	/// oracle addresses. Oracle addresses are used to fetch the price of the asset from the
	/// price oracle.
	///
	/// - **Key**: `AssetId` (H160) - The EVM-compatible asset contract address
	/// - **Value**: `AssetOracleId` (H160) - The oracle address
	pub type AssetOracles<T: Config> = StorageMap<_, Twox64Concat, AssetId, AssetOracleId>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// Mapping from chain IDs to their native currency oracle addresses.
	///
	/// This storage maps chain IDs to their corresponding native currency oracle addresses.
	/// Native currency oracle addresses are used to fetch the price of the native currency from the
	/// price oracle.
	///
	/// - **Key**: `ChainId` (u32) - The chain ID
	/// - **Value**: `AssetOracleId` (H160) - The oracle address
	pub type NativeCurrencyOracles<T: Config> = StorageMap<_, Twox64Concat, ChainId, AssetOracleId>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// Active cross-chain transfers awaiting finalization.
	///
	/// This storage tracks all transfers that have been approved for execution but not yet
	/// finalized (committed or rolled back). Transfers remain in this storage from approval
	/// until finalization, when they are moved to `FinalizedTransfers`.
	///
	/// - **Key 1**: `ChainId` (u32) - The source chain ID where the transfer originated
	/// - **Key 2**: `H256` - The source transaction ID uniquely identifying this transfer
	/// - **Value**: `TransferInfo<Balance, AccountId>` containing:
	///   - `amount`: Transfer amount in the asset's units
	///   - `asset_index_hash`: The CCCP asset index identifying the transferred asset
	///   - `option`: Fast or Standard transfer mode
	///   - `status`: Current status (Pending → OnFlight → Finalized)
	///   - `socket_message`: Original CCCP message from the transfer request
	///   - `on_flight_voters`: Relayers who voted to approve the transfer (inbound only)
	///   - `finalization_voters`: Relayers who voted to finalize the transfer (inbound only)
	///
	/// **Transfer Lifecycle**:
	/// 1. **Outbound** (Bifrost → External): Immediately OnFlight upon first submission
	/// 2. **Inbound** (External → Bifrost): Pending → OnFlight when majority consensus reached
	/// 3. Both paths: Removed from this storage and moved to `FinalizedTransfers` upon finalization
	pub type OnFlightTransfers<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		ChainId,
		Twox64Concat,
		H256,
		TransferInfo<BalanceOf<T>, T::AccountId>,
	>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// Historical record of completed cross-chain transfers.
	///
	/// This storage maintains a permanent record of all transfers that have been finalized,
	/// either committed (successfully executed) or rolled back (cancelled). Transfers are
	/// moved here from `OnFlightTransfers` upon reaching final consensus.
	///
	/// - **Key 1**: `ChainId` (u32) - The source chain ID where the transfer originated
	/// - **Key 2**: `H256` - The source transaction ID uniquely identifying this transfer
	/// - **Value**: `TransferInfo<Balance, AccountId>` with `status = Finalized` and final state:
	///   - `amount`: Transfer amount in the asset's units
	///   - `asset_index_hash`: The CCCP asset index identifying the transferred asset
	///   - `option`: Fast or Standard transfer mode used
	///   - `status`: Always `Finalized` in this storage
	///   - `socket_message`: Final CCCP message showing committed or rolled back status
	///   - `on_flight_voters`: Complete list of relayers who approved the transfer
	///   - `finalization_voters`: Complete list of relayers who confirmed finalization
	///
	/// **Purpose**:
	/// - Prevents duplicate transfer processing by checking if a source tx ID already exists
	/// - Provides historical audit trail for cross-chain transfer operations
	/// - Enables queries for transfer outcomes (committed vs rolled back)
	/// - Never removed (unbounded storage for permanent record-keeping)
	pub type FinalizedTransfers<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		ChainId,
		Twox64Concat,
		H256,
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

			let SocketMessageSubmission { authority_id, src_tx_id, message } =
				socket_message_submission;

			// Parse and validate socket message (must be in REQUESTED status)
			let msg = Self::validate_and_parse_socket_message(&message, |m| m.is_requested())?;
			let amount = msg.params.amount.try_into().map_err(|_| Error::<T>::OutOfRange)?;
			let src_chain_id: ChainId = u32::from_be_bytes(
				msg.req_id.chain.as_slice().try_into().map_err(|_| Error::<T>::OutOfRange)?,
			);
			let dst_chain_id: ChainId = u32::from_be_bytes(
				msg.ins_code.chain.as_slice().try_into().map_err(|_| Error::<T>::OutOfRange)?,
			);

			// Get asset information (if registered)
			let asset_index_hash = AssetIndexHash::from_slice(&msg.params.token_idx0);
			let asset_info = Self::get_asset_info(asset_index_hash);

			// Ensure transfer hasn't been finalized already
			ensure!(
				!FinalizedTransfers::<T>::contains_key(src_chain_id, src_tx_id),
				Error::<T>::TransferAlreadyFinalized
			);
			let on_flight_transfer = OnFlightTransfers::<T>::get(src_chain_id, src_tx_id);

			// Determine transfer option based on current cap:
			// - If asset is registered with cap: Fast if cap allows, otherwise Standard
			// - If asset is not registered: Always Standard (no Fast transfer support)
			let transfer_option = if let Some((_, ref asset_cap)) = asset_info {
				Self::determine_transfer_option(asset_cap, msg.params.amount)?
			} else {
				TransferOption::Standard
			};

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
					src_chain_id,
					src_tx_id,
					TransferInfo {
						amount,
						src_tx_id,
						src_chain_id,
						dst_chain_id,
						asset_index_hash,
						option: transfer_option,
						status: TransferStatus::OnFlight,
						socket_message: message.clone(),
						on_flight_voters: BoundedVec::try_from(vec![authority_id.clone()])
							.map_err(|_| Error::<T>::OutOfRange)?,
						finalization_voters: BoundedVec::new(),
					},
				);

				// Update cap for Fast transfers (only if asset is registered)
				if transfer_option == TransferOption::Fast {
					if let Some((asset_id, asset_cap)) = asset_info {
						Self::update_fast_transfer_cap(
							asset_id,
							asset_cap,
							msg.params.amount,
							true,
						)?;
					}
				}

				Self::deposit_event(Event::TransferPolled {
					asset_index_hash,
					sequence_id: msg.req_id.sequence,
					src_tx_id,
					src_chain_id,
					dst_chain_id,
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
						// If asset is not registered, always use Standard mode
						let actual_transfer_option = if let Some((asset_id, current_asset_cap)) =
							Self::get_asset_info(asset_index_hash)
						{
							let option = Self::determine_transfer_option(
								&current_asset_cap,
								msg.params.amount,
							)?;

							// Update cap for Fast transfers
							if option == TransferOption::Fast {
								Self::update_fast_transfer_cap(
									asset_id,
									current_asset_cap,
									msg.params.amount,
									true,
								)?;
							}

							option
						} else {
							TransferOption::Standard
						};

						// Update transfer option if cap availability changed during voting
						on_flight_transfer.option = actual_transfer_option;
						on_flight_transfer.status = TransferStatus::OnFlight;
					}

					Self::deposit_event(Event::TransferPolled {
						asset_index_hash,
						sequence_id: msg.req_id.sequence,
						src_tx_id,
						src_chain_id,
						dst_chain_id,
						authority_id,
						option: on_flight_transfer.option,
						amount: on_flight_transfer.amount,
						status: on_flight_transfer.status,
					});

					OnFlightTransfers::<T>::insert(src_chain_id, src_tx_id, on_flight_transfer);
				} else {
					// First vote: create transfer with Pending status
					OnFlightTransfers::<T>::insert(
						src_chain_id,
						src_tx_id,
						TransferInfo {
							amount,
							src_tx_id,
							src_chain_id,
							dst_chain_id,
							asset_index_hash,
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
						src_tx_id,
						src_chain_id,
						dst_chain_id,
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

			let SocketMessageSubmission { authority_id, src_tx_id, message } =
				socket_message_submission;

			// Parse and validate socket message (must be COMMITTED or ROLLBACKED)
			let msg = Self::validate_and_parse_socket_message(&message, |msg| {
				msg.is_committed() || msg.is_rollbacked()
			})?;

			let src_chain_id: ChainId = u32::from_be_bytes(
				msg.req_id.chain.as_slice().try_into().map_err(|_| Error::<T>::OutOfRange)?,
			);
			let dst_chain_id: ChainId = u32::from_be_bytes(
				msg.ins_code.chain.as_slice().try_into().map_err(|_| Error::<T>::OutOfRange)?,
			);

			// Get asset information (if registered)
			let asset_index_hash = AssetIndexHash::from_slice(&msg.params.token_idx0);
			let asset_info = Self::get_asset_info(asset_index_hash);

			// Ensure transfer is not already finalized
			ensure!(
				!FinalizedTransfers::<T>::contains_key(src_chain_id, src_tx_id),
				Error::<T>::TransferAlreadyFinalized
			);

			// Get transfer and ensure it's in OnFlight status
			let mut on_flight_transfer = OnFlightTransfers::<T>::get(src_chain_id, src_tx_id)
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
					src_chain_id,
					src_tx_id,
					on_flight_transfer.clone(),
				);
				OnFlightTransfers::<T>::remove(src_chain_id, src_tx_id);

				// Update cap for Fast transfers (only if asset is registered)
				if is_fast_transfer {
					if let Some((asset_id, asset_cap)) = asset_info {
						Self::update_fast_transfer_cap(
							asset_id,
							asset_cap,
							msg.params.amount,
							false, // subtract from cap
						)?;
					}
				}

				Self::deposit_event(Event::FinalizationPolled {
					asset_index_hash,
					sequence_id: msg.req_id.sequence,
					src_tx_id,
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
					src_chain_id,
					src_tx_id,
					on_flight_transfer.clone(),
				);
				OnFlightTransfers::<T>::remove(src_chain_id, src_tx_id);

				// Update cap for Fast transfers (only if asset is registered)
				if is_fast_transfer {
					if let Some((asset_id, asset_cap)) = asset_info {
						Self::update_fast_transfer_cap(
							asset_id,
							asset_cap,
							msg.params.amount,
							false, // subtract from cap
						)?;
					}
				}

				Self::deposit_event(Event::FinalizationPolled {
					asset_index_hash,
					sequence_id: msg.req_id.sequence,
					src_tx_id,
					src_chain_id,
					dst_chain_id,
					authority_id,
					option: on_flight_transfer.option,
					amount: on_flight_transfer.amount,
					is_finalized: true,
				});
			} else {
				// Majority not yet reached - persist vote and wait for more voters
				OnFlightTransfers::<T>::insert(src_chain_id, src_tx_id, on_flight_transfer.clone());

				Self::deposit_event(Event::FinalizationPolled {
					asset_index_hash,
					sequence_id: msg.req_id.sequence,
					src_tx_id,
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
		/// * `asset_oracle_id` - The oracle address (H160) for price feed of this asset
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
		/// * `AssetAdded { asset_id, asset_oracle_id, max_on_flight_cap, asset_indexes }` - Emitted on successful registration
		///
		/// # Storage Modifications
		/// - `AssetCaps`: Inserts new entry with `max_on_flight_cap` and `on_flight_cap = 0`
		/// - `AssetOracles`: Inserts mapping from asset_id to asset_oracle_id
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
			asset_oracle_id: AssetOracleId,
			max_on_flight_cap: BalanceOf<T>,
			asset_indexes: Vec<AssetIndexHash>,
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
				ensure!(seen.insert(asset_index), Error::<T>::DuplicateAssetIndex);
			}

			// Validate asset doesn't already exist
			ensure!(!AssetCaps::<T>::contains_key(asset_id), Error::<T>::AssetAlreadyExists);

			// Validate ALL indexes before ANY storage modifications (atomicity)
			for asset_index in &asset_indexes {
				ensure!(
					!AssetIndexes::<T>::contains_key(asset_index),
					Error::<T>::AssetIndexAlreadyExists
				);
			}

			// All validations passed - now safe to insert
			AssetCaps::<T>::insert(
				asset_id,
				AssetCapInfo { max_on_flight_cap, on_flight_cap: Default::default() },
			);
			AssetOracles::<T>::insert(asset_id, asset_oracle_id);
			for asset_index in &asset_indexes {
				AssetIndexes::<T>::insert(asset_index, asset_id);
			}

			Self::deposit_event(Event::AssetAdded {
				asset_id,
				asset_oracle_id,
				max_on_flight_cap,
				asset_indexes,
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
		/// - `AssetOracles`: Entry for `asset_id` is removed
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
			let asset_index_set: sp_std::collections::btree_set::BTreeSet<_> =
				asset_indexes.iter().collect();
			let has_active_transfers =
				OnFlightTransfers::<T>::iter().any(|(_, _, transfer_info)| {
					asset_index_set.contains(&transfer_info.asset_index_hash)
				});
			ensure!(!has_active_transfers, Error::<T>::AssetHasActiveTransfers);

			// Safe to remove asset and its indexes
			AssetCaps::<T>::remove(asset_id);
			AssetOracles::<T>::remove(asset_id);
			for asset_index in &asset_indexes {
				AssetIndexes::<T>::remove(asset_index);
			}

			Self::deposit_event(Event::AssetRemoved { asset_id, asset_indexes });

			Ok(().into())
		}

		/// Update an existing asset's configuration and CCCP asset indexes.
		///
		/// This extrinsic allows modifying the oracle address, maximum on-flight capacity of a registered
		/// asset and managing its associated CCCP asset indexes by adding or removing them.
		///
		/// # Parameters
		/// * `origin` - Must be `Root` (sudo access required)
		/// * `asset_id` - The EVM-compatible asset contract address (H160) to update
		/// * `new_asset_oracle_id` - (Optional) New oracle address for price feed of this asset
		///   - Cannot be the same as the current oracle address
		/// * `new_max_on_flight_cap` - (Optional) New maximum total amount allowed in Fast transfers
		///   - Must be > 0
		///   - Must be ≤ 100M cap limit
		///   - Cannot be less than the current `on_flight_cap`
		/// * `add_asset_indexes` - (Optional) Vector of new CCCP asset index hashes to associate
		///   - Maximum 100 indexes per call
		///   - No duplicates allowed within the call
		///   - Each index must not already be registered
		/// * `remove_asset_indexes` - (Optional) Vector of CCCP asset index hashes to disassociate
		///   - Maximum 100 indexes per call
		///   - Each index must belong to the specified `asset_id`
		///   - Index must not have active on-flight transfers
		///
		/// # Errors
		/// * `EmptySubmission` - If no update fields are provided
		/// * `AssetDNE` - If the asset is not registered
		/// * `TooManyAssetIndexes` - If more than 100 asset indexes provided in add or remove lists
		/// * `DuplicateAssetIndex` - If duplicate indexes exist within the add or remove lists
		/// * `ConflictingAssetIndexOperation` - If an index is present in both add and remove lists
		/// * `NoWritingSameValue` - If the new cap is identical to the current cap
		/// * `InvalidMaxCap` - If the new cap is zero
		/// * `CapTooLarge` - If the new cap exceeds 100M limit
		/// * `CapReductionBelowCurrentUsage` - If the new cap is less than the current on-flight usage
		/// * `AssetIndexAlreadyExists` - If a new index to add is already registered
		/// * `AssetIndexDNE` - If an index to remove is not registered
		/// * `AssetIndexNotBelongToAsset` - If an index to remove belongs to a different asset
		/// * `AssetHasActiveTransfers` - If an index to remove has active on-flight transfers
		///
		/// # Events
		/// * `AssetUpdated { asset_id, new_asset_oracle_id, new_max_on_flight_cap, add_asset_indexes, remove_asset_indexes }`
		///
		/// # Storage Modifications
		/// - `AssetOracles`: Updates oracle address for the asset if provided
		/// - `AssetCaps`: Updates `max_on_flight_cap` for the asset if provided
		/// - `AssetIndexes`: Inserts new mappings and removes specified mappings
		#[pallet::call_index(5)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn update_asset(
			origin: OriginFor<T>,
			asset_id: AssetId,
			new_asset_oracle_id: Option<AssetOracleId>,
			new_max_on_flight_cap: Option<BalanceOf<T>>,
			add_asset_indexes: Option<Vec<AssetIndexHash>>,
			remove_asset_indexes: Option<Vec<AssetIndexHash>>,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;

			// Ensure at least one of the fields is provided
			ensure!(
				new_asset_oracle_id.is_some()
					|| new_max_on_flight_cap.is_some()
					|| add_asset_indexes.is_some()
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
					ensure!(seen.insert(asset_index), Error::<T>::DuplicateAssetIndex);
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
					ensure!(!remove.contains(add_idx), Error::<T>::ConflictingAssetIndexOperation);
				}
			}

			let mut asset_cap = AssetCaps::<T>::get(asset_id).ok_or(Error::<T>::AssetDNE)?;

			// Update oracle address if provided
			if let Some(new_asset_oracle_id) = new_asset_oracle_id {
				let current_oracle_id = AssetOracles::<T>::get(asset_id);
				if let Some(current_oracle_id) = current_oracle_id {
					ensure!(
						current_oracle_id != new_asset_oracle_id,
						Error::<T>::NoWritingSameValue
					);
				}
				AssetOracles::<T>::insert(asset_id, new_asset_oracle_id);
			}

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
						!AssetIndexes::<T>::contains_key(asset_index),
						Error::<T>::AssetIndexAlreadyExists
					);
					AssetIndexes::<T>::insert(asset_index, asset_id);
				}
			}
			if let Some(remove_asset_indexes) = &remove_asset_indexes {
				for asset_index in remove_asset_indexes {
					let found_asset_id =
						AssetIndexes::<T>::get(asset_index).ok_or(Error::<T>::AssetIndexDNE)?;
					ensure!(found_asset_id == asset_id, Error::<T>::AssetIndexNotBelongToAsset);
				}

				// Prevent removing asset indexes with active transfers
				let remove_index_set: sp_std::collections::btree_set::BTreeSet<_> =
					remove_asset_indexes.iter().collect();
				let has_active_transfers =
					OnFlightTransfers::<T>::iter().any(|(_, _, transfer_info)| {
						remove_index_set.contains(&transfer_info.asset_index_hash)
					});
				ensure!(!has_active_transfers, Error::<T>::AssetHasActiveTransfers);

				for asset_index in remove_asset_indexes {
					AssetIndexes::<T>::remove(asset_index);
				}
			}

			Self::deposit_event(Event::AssetUpdated {
				asset_id,
				new_asset_oracle_id,
				new_max_on_flight_cap,
				add_asset_indexes,
				remove_asset_indexes,
			});

			Ok(().into())
		}

		#[pallet::call_index(6)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn set_native_currency_oracle(
			origin: OriginFor<T>,
			chain_id: ChainId,
			native_currency_oracle_id: AssetOracleId,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;

			ensure!(
				!NativeCurrencyOracles::<T>::contains_key(chain_id),
				Error::<T>::NativeCurrencyChainAlreadyExists
			);
			NativeCurrencyOracles::<T>::insert(chain_id, native_currency_oracle_id);

			Ok(().into())
		}

		#[pallet::call_index(7)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn update_native_currency_oracle(
			origin: OriginFor<T>,
			chain_id: ChainId,
			native_currency_oracle_id: AssetOracleId,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;

			ensure!(
				NativeCurrencyOracles::<T>::contains_key(chain_id),
				Error::<T>::NativeCurrencyChainDNE
			);
			if let Some(current_oracle_id) = NativeCurrencyOracles::<T>::get(chain_id) {
				ensure!(
					current_oracle_id != native_currency_oracle_id,
					Error::<T>::NoWritingSameValue
				);
			}
			NativeCurrencyOracles::<T>::insert(chain_id, native_currency_oracle_id);

			Ok(().into())
		}

		#[pallet::call_index(8)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn remove_native_currency_oracle(
			origin: OriginFor<T>,
			chain_id: ChainId,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;

			ensure!(
				NativeCurrencyOracles::<T>::contains_key(chain_id),
				Error::<T>::NativeCurrencyChainDNE
			);
			NativeCurrencyOracles::<T>::remove(chain_id);

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
					let SocketMessageSubmission { authority_id, src_tx_id, message } =
						socket_message_submission;
					Self::verify_authority(authority_id)?;

					// verify if the signature was originated from the authority_id.
					let message = [
						keccak_256("OnFlightPoll".as_bytes()).as_slice(),
						src_tx_id.as_ref(),
						message,
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
				Call::finalize_poll { socket_message_submission, signature } => {
					let SocketMessageSubmission { authority_id, src_tx_id, message } =
						socket_message_submission;
					Self::verify_authority(authority_id)?;

					// verify if the signature was originated from the authority_id.
					let message = [
						keccak_256("FinalizePoll".as_bytes()).as_slice(),
						src_tx_id.as_ref(),
						message,
					]
					.concat();
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
