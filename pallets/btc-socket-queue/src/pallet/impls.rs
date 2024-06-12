use ethabi_decode::{ParamKind, Token};

use bp_multi_sig::{
	traits::PoolManager, Address, BoundedBitcoinAddress, Hash, Psbt, PsbtExt, Script, Secp256k1,
	Txid, UnboundedBytes,
};
use pallet_evm::Runner;
use scale_info::prelude::{format, string::ToString};
use sp_core::{Get, H160, H256, U256};
use sp_io::hashing::keccak_256;
use sp_runtime::{
	transaction_validity::{InvalidTransaction, TransactionValidityError},
	BoundedVec, DispatchError,
};
use sp_std::{boxed::Box, collections::btree_map::BTreeMap, str, str::FromStr, vec, vec::Vec};

use crate::{
	HashKeyRequest, RequestInfo, SocketMessage, TxInfo, UserRequest,
	BITCOIN_SOCKET_TXS_FUNCTION_SELECTOR, CALL_GAS_LIMIT, SOCKET_GET_REQUEST_FUNCTION_SELECTOR,
};

use super::pallet::*;

impl<T> Pallet<T>
where
	T: Config,
	T::AccountId: Into<H160>,
	H160: Into<T::AccountId>,
{
	/// Verify if the authority_id is valid
	pub fn verify_authority(authority_id: &T::AccountId) -> Result<(), TransactionValidityError> {
		if let Some(a) = <Authority<T>>::get() {
			if a != *authority_id {
				return Err(InvalidTransaction::BadSigner.into());
			}
			return Ok(());
		} else {
			return Err(InvalidTransaction::BadSigner.into());
		}
	}

	/// Try to finalize the latest combined PSBT.
	pub fn try_psbt_finalization(combined: Psbt) -> Result<Psbt, DispatchError> {
		let secp = Secp256k1::new();
		let finalized = combined.finalize(&secp).map_err(|_| Error::<T>::CannotFinalizePsbt)?;
		Ok(finalized)
	}

	/// Try to combine the signed PSBT with the latest combined PSBT. If fails, the given PSBT is considered as invalid.
	pub fn try_psbt_combination(combined: &mut Psbt, signed: &Psbt) -> Result<Psbt, DispatchError> {
		combined.combine(signed.clone()).map_err(|_| Error::<T>::InvalidPsbt)?;
		Ok(combined.clone())
	}

	/// Try to deserialize the given bytes to a `PSBT` instance.
	pub fn try_get_checked_psbt(psbt: &UnboundedBytes) -> Result<Psbt, DispatchError> {
		Ok(Psbt::deserialize(psbt).map_err(|_| Error::<T>::InvalidPsbt)?)
	}

	/// Try to convert a script to a Bitcoin address.
	pub fn try_convert_to_address_from_script(script: &Script) -> Result<Address, DispatchError> {
		Ok(Address::from_script(script, T::RegistrationPool::get_bitcoin_network())
			.map_err(|_| Error::<T>::InvalidBitcoinAddress)?)
	}

	/// Try to convert bytes to a Bitcoin address.
	pub fn try_convert_to_address_from_vec(
		addr: BoundedBitcoinAddress,
	) -> Result<Address, DispatchError> {
		let addr = str::from_utf8(&addr).map_err(|_| Error::<T>::InvalidBitcoinAddress)?;
		Ok(Address::from_str(addr)
			.map_err(|_| Error::<T>::InvalidBitcoinAddress)?
			.assume_checked())
	}

	/// Try to verify PSBT outputs with the given `SocketMessage`'s.
	pub fn try_psbt_output_verification(
		psbt: &Psbt,
		unchecked_outputs: BTreeMap<BoundedBitcoinAddress, Vec<UnboundedBytes>>,
	) -> Result<(Vec<SocketMessage>, Vec<UnboundedBytes>), DispatchError> {
		let psbt_outputs = &psbt.unsigned_tx.output;
		// output length must match.
		if psbt_outputs.len() != unchecked_outputs.len() {
			return Err(Error::<T>::InvalidPsbt.into());
		}
		// at least 2 outputs required. one for refund and one for system vault.
		if psbt_outputs.len() < 2 {
			return Err(Error::<T>::InvalidPsbt.into());
		}
		let system_vault =
			T::RegistrationPool::get_system_vault().ok_or(Error::<T>::SystemVaultDNE)?;

		let mut deserialized_msgs = vec![];
		let mut serialized_msgs = vec![];
		let mut msg_hashes = vec![];
		for output in psbt_outputs {
			let to: BoundedBitcoinAddress = BoundedVec::try_from(
				Self::try_convert_to_address_from_script(output.script_pubkey.as_script())?
					.to_string()
					.as_bytes()
					.to_vec(),
			)
			.map_err(|_| Error::<T>::InvalidBitcoinAddress)?;

			// verify socket messages
			if let Some(socket_messages) = unchecked_outputs.get(&to) {
				// output for system vault must contain zero messages.
				if to == system_vault && socket_messages.is_empty() {
					return Err(Error::<T>::InvalidUncheckedOutput.into());
				}

				let mut amount = U256::default();
				for serialized_msg in socket_messages {
					let msg = Self::try_decode_socket_message(serialized_msg)
						.map_err(|_| Error::<T>::InvalidSocketMessage)?;
					let msg_hash = Self::hash_bytes(
						&UserRequest::new(msg.ins_code.clone(), msg.params.clone()).encode(),
					);
					if msg_hashes.contains(&msg_hash) {
						return Err(Error::<T>::InvalidSocketMessage.into());
					}
					let request_info = Self::try_get_request(&msg.encode_req_id())?;

					// the socket message should be valid
					if !request_info.is_msg_hash(msg_hash) {
						return Err(Error::<T>::InvalidSocketMessage.into());
					}
					if !request_info.is_accepted() || !msg.is_accepted() {
						return Err(Error::<T>::InvalidSocketMessage.into());
					}
					if !msg.is_outbound(
						<T as pallet_evm::Config>::ChainId::get() as u32,
						T::RegistrationPool::get_bitcoin_chain_id(),
					) {
						return Err(Error::<T>::InvalidSocketMessage.into());
					}
					if Self::socket_messages(&msg.req_id.sequence).is_some() {
						return Err(Error::<T>::SocketMessageAlreadySubmitted.into());
					}

					// user must be registered
					if T::RegistrationPool::get_refund_address(&msg.params.to.into()).is_none() {
						return Err(Error::<T>::UserDNE.into());
					}

					deserialized_msgs.push(msg.clone());
					serialized_msgs.push(serialized_msg.clone());
					msg_hashes.push(msg_hash);
					amount = amount.checked_add(msg.params.amount).unwrap();
				}

				// verify psbt output (refund addresses only)
				let psbt_amount = U256::from(output.value.to_sat());
				if to != system_vault && psbt_amount != amount {
					return Err(Error::<T>::InvalidPsbt.into());
				}
			} else {
				return Err(Error::<T>::InvalidUncheckedOutput.into());
			}
		}
		Ok((deserialized_msgs, serialized_msgs))
	}

	/// Hash the given bytes.
	pub fn hash_bytes(bytes: &UnboundedBytes) -> H256 {
		H256::from(keccak_256(bytes))
	}

	/// Convert txid from big endian to little endian.
	pub fn convert_txid(txid: Txid) -> H256 {
		let mut txid = txid.to_byte_array();
		txid.reverse();
		H256::from(txid)
	}

	/// Try Pallet EVM contract call.
	pub fn try_evm_call(
		source: T::AccountId,
		target: T::AccountId,
		calldata: &str,
	) -> Result<UnboundedBytes, DispatchError> {
		let info = <T as pallet_evm::Config>::Runner::call(
			source.into(),
			target.into(),
			hex::decode(calldata).map_err(|_| Error::<T>::InvalidCalldata)?,
			U256::zero(),
			CALL_GAS_LIMIT,
			None,
			None,
			None,
			vec![],
			false,
			true,
			None,
			None,
			<T as pallet_evm::Config>::config(),
		)
		.map_err(|_| Error::<T>::InvalidCalldata)?;

		Ok(info.value)
	}

	/// Try to get the `RequestInfo` by the given `req_id`.
	pub fn try_get_request(req_id: &UnboundedBytes) -> Result<RequestInfo, DispatchError> {
		let caller = <Authority<T>>::get().ok_or(Error::<T>::AuthorityDNE)?;
		let socket = <Socket<T>>::get().ok_or(Error::<T>::SocketDNE)?;
		let calldata = format!(
			"{}{}",
			SOCKET_GET_REQUEST_FUNCTION_SELECTOR,
			array_bytes::bytes2hex("", req_id)
		);

		Ok(Self::try_decode_request_info(&Self::try_evm_call(caller, socket, &calldata)?)
			.map_err(|_| Error::<T>::InvalidRequestInfo)?)
	}

	/// Generate a hash key.
	pub fn generate_hash_key(txid: H256, vout: U256, who: T::AccountId, amount: U256) -> H256 {
		let hash_key_req = HashKeyRequest::new(txid.0.to_vec(), vout, who.into(), amount);
		Self::hash_bytes(&hash_key_req.encode())
	}

	/// Try to get the `TxInfo` by the given `hash_key`.
	pub fn try_get_tx_info(hash_key: H256) -> Result<TxInfo, DispatchError> {
		let caller = <Authority<T>>::get().ok_or(Error::<T>::AuthorityDNE)?;
		let bitcoin_socket = <BitcoinSocket<T>>::get().ok_or(Error::<T>::SocketDNE)?;
		let calldata = format!(
			"{}{}",
			BITCOIN_SOCKET_TXS_FUNCTION_SELECTOR,
			array_bytes::bytes2hex("", hash_key.as_bytes())
		);

		Ok(Self::try_decode_tx_info(&Self::try_evm_call(caller, bitcoin_socket, &calldata)?)
			.map_err(|_| Error::<T>::InvalidTxInfo)?)
	}

	/// Try to decode the given `TxInfo`.
	pub fn try_decode_tx_info(info: &UnboundedBytes) -> Result<TxInfo, ()> {
		match ethabi_decode::decode(
			&[
				ParamKind::Address,
				ParamKind::Uint(256),
				ParamKind::Uint(256),
				ParamKind::Tuple(vec![
					Box::new(ParamKind::FixedBytes(4)),
					Box::new(ParamKind::Uint(64)),
					Box::new(ParamKind::Uint(128)),
				]),
			],
			info,
		) {
			Ok(token) => Ok(token.clone().try_into()?),
			Err(_) => return Err(()),
		}
	}

	/// Try to decode the given `RequestInfo`.
	pub fn try_decode_request_info(info: &UnboundedBytes) -> Result<RequestInfo, ()> {
		match ethabi_decode::decode(
			&[
				ParamKind::FixedArray(Box::new(ParamKind::Uint(8)), 32),
				ParamKind::FixedBytes(32),
				ParamKind::Uint(256),
			],
			info,
		) {
			Ok(token) => Ok(token.clone().try_into()?),
			Err(_) => return Err(()),
		}
	}

	/// Try to decode the given `SocketMessage`.
	pub fn try_decode_socket_message(msg: &UnboundedBytes) -> Result<SocketMessage, ()> {
		match ethabi_decode::decode(
			&[ParamKind::Tuple(vec![
				Box::new(ParamKind::Tuple(vec![
					Box::new(ParamKind::FixedBytes(4)),
					Box::new(ParamKind::Uint(64)),
					Box::new(ParamKind::Uint(128)),
				])),
				Box::new(ParamKind::Uint(8)),
				Box::new(ParamKind::Tuple(vec![
					Box::new(ParamKind::FixedBytes(4)),
					Box::new(ParamKind::FixedBytes(16)),
				])),
				Box::new(ParamKind::Tuple(vec![
					Box::new(ParamKind::FixedBytes(32)),
					Box::new(ParamKind::FixedBytes(32)),
					Box::new(ParamKind::Address),
					Box::new(ParamKind::Address),
					Box::new(ParamKind::Uint(256)),
					Box::new(ParamKind::Bytes),
				])),
			])],
			msg,
		) {
			Ok(socket) => match &socket[0] {
				Token::Tuple(msg) => Ok(msg.clone().try_into()?),
				_ => return Err(()),
			},
			Err(_) => return Err(()),
		}
	}
}
