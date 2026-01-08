use frame_support::pallet_prelude::{InvalidTransaction, TransactionValidityError};
use pallet_evm::Runner;

use bp_cccp::{RequestInfo, UnboundedBytes, SOCKET_GET_REQUEST_FUNCTION_SELECTOR};
use bp_staking::traits::Authorities;
use sp_core::{H160, H256};
use sp_io::hashing::keccak_256;
use sp_runtime::DispatchError;

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
			target: "pallet-cccp-transfer-manager",
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
			100_000u64,    // gas_limit (enough for view call)
			T::config(),
		);

		let result = match result {
			Err(_) => {
				log::warn!(
					target: "pallet-cccp-transfer-manager",
					"Socket call failed: Runner::view_call returned error"
				);
				return Err(Error::<T>::InvalidRequestInfo.into());
			},
			Ok(r) => {
				log::debug!(
					target: "pallet-cccp-transfer-manager",
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
					target: "pallet-cccp-transfer-manager",
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
				target: "pallet-cccp-transfer-manager",
				"Socket call failed: return data too short, expected 1088 bytes, got {}",
				return_data.len()
			);
			return Err(Error::<T>::InvalidRequestInfo.into());
		}

		// Parse RequestInfo from return data
		let request_info =
			RequestInfo::try_from(return_data).map_err(|_| Error::<T>::InvalidRequestInfo)?;

		log::debug!(
			target: "pallet-cccp-transfer-manager",
			"Request info: msg_hash={:?}, registered_time={}",
			request_info.msg_hash, request_info.registered_time
		);

		Ok(request_info)
	}
}
