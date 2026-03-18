use frame_support::{
	ensure,
	pallet_prelude::{InvalidTransaction, TransactionValidityError},
};
use pallet_evm::Runner;

use bp_cccp::{
	traits::RelayQueueManager, RequestInfo, SocketMessage, UnboundedBytes, UserRequest,
	SOCKET_GET_REQUEST_FUNCTION_SELECTOR,
};
use bp_staking::{traits::Authorities, MAX_AUTHORITIES};
use scale_info::prelude::format;
use sp_core::{ConstU32, H160, H256, U256};
use sp_io::hashing::keccak_256;
use sp_runtime::{BoundedVec, DispatchError};

use crate::{
	AssetCapInfo, AssetId, AssetIndexHash, BalanceOf, TransferInfo, TransferInfoWithTxId,
	TransferOption,
};

use super::pallet::*;

impl<T: Config> Pallet<T> {
	pub fn verify_authority(authority_id: &T::AccountId) -> Result<(), TransactionValidityError> {
		if !T::Relayers::is_authority(authority_id) {
			return Err(InvalidTransaction::BadSigner.into());
		}
		Ok(())
	}

	/// Hash the given bytes.
	pub fn hash_bytes(bytes: &UnboundedBytes) -> H256 {
		H256::from(keccak_256(bytes))
	}

	/// Try to get the `RequestInfo` by the given `req_id`.
	pub fn try_get_request(req_id: &UnboundedBytes) -> Result<RequestInfo, DispatchError>
	where
		T::AccountId: Into<H160>,
	{
		let socket = <Socket<T>>::get().ok_or(Error::<T>::SocketDNE)?;
		let calldata_hex = format!(
			"{}{}",
			SOCKET_GET_REQUEST_FUNCTION_SELECTOR,
			array_bytes::bytes2hex("", req_id)
		);
		let calldata =
			array_bytes::hex2bytes(&calldata_hex).map_err(|_| Error::<T>::InvalidRequestInfo)?;

		log::debug!(
			target: "pallet-cccp-relay-queue",
			"Socket call: calling get_request() on {:?}",
			socket
		);

		// Execute call via pallet-evm Runner using view_call to avoid state changes.
		// view_call wraps execution in a storage transaction that gets rolled back,
		// ensuring no state changes persist (including nonce increments).
		let result = T::Runner::view_call(
			H160::zero(),  // source (context only, no state changes)
			socket.into(), // target (socket contract)
			calldata,      // input (function selector + req_id)
			1_000_000u64,  // gas_limit (1M for view call)
			T::config(),
		);

		let result = match result {
			Err(_) => {
				log::warn!(
					target: "pallet-cccp-relay-queue",
					"Socket call failed: Runner::view_call returned error"
				);
				return Err(Error::<T>::InvalidRequestInfo.into());
			},
			Ok(r) => {
				log::debug!(
					target: "pallet-cccp-relay-queue",
					"Socket call result: exit_reason={:?}, return_data_len={}",
					r.exit_reason, r.value.len()
				);
				r
			},
		};

		// Check for successful execution
		match result.exit_reason {
			pallet_evm::ExitReason::Succeed(_) => {},
			ref reason => {
				log::warn!(
					target: "pallet-cccp-relay-queue",
					"Socket call reverted: {:?}",
					reason
				);
				return Err(Error::<T>::InvalidRequestInfo.into());
			},
		}

		// Parse return data
		// get_request returns: (uint8[32], bytes32, uint256)
		// Total: 32 * 32 + 32 + 32 = 1088 bytes
		let return_data = result.value;
		if return_data.len() < 1088 {
			log::warn!(
				target: "pallet-cccp-relay-queue",
				"Socket call failed: return data too short, expected 1088 bytes, got {}",
				return_data.len()
			);
			return Err(Error::<T>::InvalidRequestInfo.into());
		}

		// Parse RequestInfo from return data
		let request_info =
			RequestInfo::try_from(return_data).map_err(|_| Error::<T>::InvalidRequestInfo)?;

		log::debug!(
			target: "pallet-cccp-relay-queue",
			"Request info: msg_hash={:?}, registered_time={}",
			request_info.msg_hash, request_info.registered_time
		);

		Ok(request_info)
	}

	/// Parse and validate a socket message from raw bytes.
	///
	/// # Arguments
	/// * `message` - Raw message bytes to parse
	/// * `expected_status_check` - Optional closure to validate the message status
	///
	/// # Returns
	/// Parsed `SocketMessage` if valid, otherwise an error
	pub fn validate_and_parse_socket_message(
		message: &UnboundedBytes,
		expected_status_check: impl FnOnce(&SocketMessage) -> bool,
	) -> Result<SocketMessage, DispatchError> {
		ensure!(!message.is_empty(), Error::<T>::EmptySubmission);

		let msg = SocketMessage::try_from(message.clone())
			.map_err(|_| Error::<T>::InvalidSocketMessage)?;

		ensure!(expected_status_check(&msg), Error::<T>::MessageStatusMismatch);

		Ok(msg)
	}

	/// Get asset information.
	///
	/// # Arguments
	/// * `asset_index_hash` - The asset index hash to look up
	///
	/// # Returns
	/// `Some((asset_id, asset_cap_info))` if both asset ID and cap are registered,
	/// `None` if either is missing (causes transfer to use Standard mode)
	///
	/// # Note
	/// When this returns `None`, the transfer will always use Standard mode regardless
	/// of amount, since Fast transfers are only supported for registered assets with
	/// configured capacity limits.
	pub fn get_asset_info(
		asset_index_hash: AssetIndexHash,
	) -> Option<(AssetId, AssetCapInfo<BalanceOf<T>>)> {
		let asset_id = AssetIndexes::<T>::get(asset_index_hash)?;
		let asset_cap = AssetCaps::<T>::get(asset_id)?;

		Some((asset_id, asset_cap))
	}

	/// Validate on-chain status of a transfer request.
	///
	/// # Arguments
	/// * `msg` - The socket message to validate
	///
	/// # Returns
	/// The validated `RequestInfo` from on-chain state
	pub fn validate_on_chain_existence(msg: &SocketMessage) -> Result<RequestInfo, DispatchError>
	where
		T::AccountId: Into<H160>,
	{
		let msg_hash =
			Self::hash_bytes(&UserRequest::new(msg.ins_code.clone(), msg.params.clone()).encode());
		let request_info = Self::try_get_request(&msg.encode_req_id())?;

		ensure!(request_info.is_msg_hash(msg_hash), Error::<T>::OnChainExistenceMismatch);

		Ok(request_info)
	}

	/// Add a voter to a voter list with double-vote prevention.
	///
	/// # Arguments
	/// * `voters` - The voter list to add to
	/// * `authority_id` - The authority to add
	///
	/// # Returns
	/// Ok if voter added successfully, error if already voted or list full
	pub fn add_voter_to_list(
		voters: &mut BoundedVec<T::AccountId, ConstU32<MAX_AUTHORITIES>>,
		authority_id: &T::AccountId,
	) -> Result<(), DispatchError> {
		ensure!(!voters.contains(authority_id), Error::<T>::AlreadyVoted);
		voters.try_push(authority_id.clone()).map_err(|_| Error::<T>::OutOfRange)?;
		Ok(())
	}

	/// Determine transfer option based on current asset cap availability.
	///
	/// # Arguments
	/// * `asset_cap` - Current asset cap info
	/// * `amount` - Transfer amount to check
	///
	/// # Returns
	/// TransferOption::Fast if cap allows, otherwise TransferOption::Standard
	///
	/// # Note
	/// This should be called with the LATEST asset cap from storage to ensure
	/// accurate determination, especially during voting when caps can change.
	pub fn determine_transfer_option(
		asset_cap: &AssetCapInfo<BalanceOf<T>>,
		amount: U256,
	) -> Result<TransferOption, DispatchError>
	where
		BalanceOf<T>: Into<U256>,
	{
		let cap_after = asset_cap
			.on_flight_cap
			.into()
			.checked_add(amount)
			.ok_or(Error::<T>::OutOfRange)?;

		if cap_after > asset_cap.max_on_flight_cap.into() {
			Ok(TransferOption::Standard)
		} else {
			Ok(TransferOption::Fast)
		}
	}

	/// Update asset cap for fast transfers.
	///
	/// # Arguments
	/// * `asset_id` - The asset to update
	/// * `asset_cap` - Current asset cap info
	/// * `amount` - Amount to add (positive) or subtract (negative as U256)
	/// * `is_addition` - true to add, false to subtract
	///
	/// # Returns
	/// Updated asset cap info
	pub fn update_fast_transfer_cap(
		asset_id: AssetId,
		mut asset_cap: AssetCapInfo<BalanceOf<T>>,
		amount: U256,
		is_addition: bool,
	) -> Result<AssetCapInfo<BalanceOf<T>>, DispatchError>
	where
		BalanceOf<T>: Into<U256> + TryFrom<U256>,
	{
		let current_cap: U256 = asset_cap.on_flight_cap.into();

		let new_cap = if is_addition {
			current_cap.checked_add(amount).ok_or(Error::<T>::OutOfRange)?
		} else {
			current_cap.checked_sub(amount).ok_or(Error::<T>::OutOfRange)?
		};

		asset_cap.on_flight_cap = new_cap.try_into().map_err(|_| Error::<T>::OutOfRange)?;
		AssetCaps::<T>::insert(asset_id, asset_cap.clone());

		Ok(asset_cap)
	}
}

impl<T: Config> RelayQueueManager<T::AccountId> for Pallet<T> {
	fn replace_authority(old: &T::AccountId, new: &T::AccountId) {
		// Replace authority in all pending transfers (only handle on-flight voters. since finalization voters are handled in OnFlightTransfers)
		PendingTransfers::<T>::translate::<TransferInfo<BalanceOf<T>, T::AccountId>, _>(
			|_msg_hash, _src_tx_id, mut transfer_info| {
				if let Some(pos) =
					transfer_info.on_flight_voters.iter().position(|voter| voter == old)
				{
					// Remove old authority and add new one at the same position
					transfer_info.on_flight_voters.remove(pos);
					if transfer_info.on_flight_voters.try_insert(pos, new.clone()).is_err() {
						log::warn!(
							target: "pallet-cccp-relay-queue",
							"Failed to replace authority in on_flight_voters: {:?} -> {:?}",
							old,
							new
						);
					}
				}
				Some(transfer_info)
			},
		);

		// Replace authority in all on-flight transfers (only handle finalization voters. since on-flight voters are handled in PendingTransfers)
		OnFlightTransfers::<T>::translate::<TransferInfoWithTxId<BalanceOf<T>, T::AccountId>, _>(
			|_msg_hash, mut transfer_info| {
				if let Some(pos) =
					transfer_info.finalization_voters.iter().position(|voter| voter == old)
				{
					// Remove old authority and add new one at the same position
					transfer_info.finalization_voters.remove(pos);
					if transfer_info.finalization_voters.try_insert(pos, new.clone()).is_err() {
						log::warn!(
							target: "pallet-cccp-relay-queue",
							"Failed to replace authority in finalization_voters: {:?} -> {:?}",
							old,
							new
						);
					}
				}

				Some(transfer_info)
			},
		);
	}
}
