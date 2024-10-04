use crate::{
	HashKeyRequest, RequestInfo, RequestType, SocketMessage, TxInfo, UserRequest,
	BITCOIN_SOCKET_TXS_FUNCTION_SELECTOR, CALL_GAS_LIMIT, SOCKET_GET_REQUEST_FUNCTION_SELECTOR,
};
use bp_multi_sig::{
	traits::{PoolManager, SocketQueueManager},
	Address, BoundedBitcoinAddress, Hash, Psbt, PsbtExt, Script, Secp256k1, Txid, UnboundedBytes,
};
use ethabi_decode::{ParamKind, Token};
use frame_support::ensure;
use miniscript::bitcoin::{opcodes, psbt, script::Instruction, TxIn, Weight as BitcoinWeight};
use pallet_evm::Runner;
use scale_info::prelude::{format, string::ToString};
use sp_core::{Get, H160, H256, U256};
use sp_io::hashing::keccak_256;
use sp_runtime::{
	transaction_validity::{InvalidTransaction, TransactionValidityError},
	BoundedVec, DispatchError,
};
use sp_std::{boxed::Box, collections::btree_map::BTreeMap, str, str::FromStr, vec, vec::Vec};

use super::pallet::*;

impl<T: Config> SocketQueueManager<T::AccountId> for Pallet<T> {
	fn is_ready_for_migrate() -> bool {
		let is_pending_requests_empty = <PendingRequests<T>>::iter().next().is_none();
		let is_finalized_requests_empty = <FinalizedRequests<T>>::iter().next().is_none();
		let is_pending_rollback_requests_empty =
			<RollbackRequests<T>>::iter().all(|x| x.1.is_approved);

		// Return true only if all request storages are empty.
		is_pending_requests_empty
			&& is_finalized_requests_empty
			&& is_pending_rollback_requests_empty
	}

	fn verify_authority(authority_id: &T::AccountId) -> Result<(), TransactionValidityError> {
		if let Some(a) = <Authority<T>>::get() {
			if a != *authority_id {
				return Err(InvalidTransaction::BadSigner.into());
			}
			Ok(())
		} else {
			Err(InvalidTransaction::BadSigner.into())
		}
	}
}

impl<T> Pallet<T>
where
	T: Config,
	T::AccountId: Into<H160>,
	H160: Into<T::AccountId>,
{
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

	/// Parse the witness script for extract m and n for m-of-n multisig.
	/// return None if the script is not a valid multisig script.
	fn parse_multisig_script(script: &Script) -> Option<(usize, usize)> {
		let instructions = script.instructions().collect::<Vec<_>>();

		if instructions.len() < 3 {
			return None; // Not enough instructions for multisig
		}

		// First instruction should be the number of required signatures (m)
		let m = match instructions[0] {
			Ok(Instruction::Op(op)) if op.to_u8() >= 0x51 && op.to_u8() <= 0x60 => {
				(op.to_u8() - 0x51 + 1) as usize
			},
			_ => return None, // Not a valid multisig script
		};

		// Last instruction should be OP_CHECKMULTISIG opcode
		if let Ok(Instruction::Op(opcodes::all::OP_CHECKMULTISIG)) = instructions.last().unwrap() {
			// Second-to-last instruction should be the number of public keys (n)
			let n = match instructions[instructions.len() - 2] {
				Ok(Instruction::Op(op)) if op.to_u8() >= 0x51 && op.to_u8() <= 0x60 => {
					(op.to_u8() - 0x51 + 1) as usize
				},
				_ => return None, // Not a valid multisig script
			};

			Some((m, n))
		} else {
			None
		}
	}

	/// Estimate the finalized input vsize from unsigned.
	fn estimate_finalized_input_size(
		input: &psbt::Input,
		txin: &TxIn,
	) -> Result<u64, DispatchError> {
		let witness_script = input.witness_script.as_ref().ok_or(Error::<T>::InvalidPsbt)?;
		let (m, _) = Self::parse_multisig_script(witness_script).ok_or(Error::<T>::InvalidPsbt)?;

		let script_len = witness_script.len() + 1;

		// empty(1byte) + signatures(73 * m) + script_len
		let estimated_witness_size = 1 + 73 * m + script_len;

		let estimated_final_vsize =
			(BitcoinWeight::from_witness_data_size(estimated_witness_size as u64)
				+ BitcoinWeight::from_non_witness_data_size(txin.base_size() as u64))
			.to_vbytes_ceil();

		Ok(estimated_final_vsize)
	}

	/// Estimate the finalized vsize from the given PSBT.
	fn estimate_finalized_vb(psbt: &Psbt) -> Result<u64, DispatchError> {
		let mut total_vb = 10; // version(4) + locktime(4) + input_count(1) + output_count(1)

		for (i, input) in psbt.inputs.iter().enumerate() {
			let txin = psbt.unsigned_tx.input.get(i).ok_or(Error::<T>::InvalidPsbt)?;
			let input_vb = Self::estimate_finalized_input_size(input, txin)?;
			total_vb += input_vb;
		}
		total_vb +=
			psbt.unsigned_tx.output.iter().map(|x| x.weight().to_vbytes_ceil()).sum::<u64>();

		Ok(total_vb)
	}

	/// Try to verify fee was set properly in the PSBT.
	pub fn try_psbt_fee_verification(psbt: &Psbt) -> Result<(), DispatchError> {
		let fee = psbt.fee().map_err(|_| Error::<T>::InvalidPsbt)?;
		let estimated_vb = Self::estimate_finalized_vb(psbt)?;

		let fee_rate = (fee / estimated_vb).to_sat();
		ensure!(fee_rate <= <MaxFeeRate<T>>::get(), Error::<T>::InvalidFeeRate);

		Ok(())
	}

	/// Try to verify PSBT inputs/outputs for RBF.
	pub fn try_bump_fee_psbt_verification(
		old_psbt: &Psbt,
		new_psbt: &Psbt,
		request_type: &RequestType,
	) -> Result<(), DispatchError> {
		let old_psbt_inputs = &old_psbt.unsigned_tx.input;
		let old_psbt_outputs = &old_psbt.unsigned_tx.output;

		let new_psbt_inputs = &new_psbt.unsigned_tx.input;
		let new_psbt_outputs = &new_psbt.unsigned_tx.output;

		let current_round = T::RegistrationPool::get_current_round();
		let system_vault = T::RegistrationPool::get_system_vault(current_round)
			.ok_or(Error::<T>::SystemVaultDNE)?;

		// output length check.
		// the new output can possibly include/exclude an output for change.
		if (new_psbt_outputs.len() as isize - old_psbt_outputs.len() as isize).abs() > 1 {
			return Err(Error::<T>::InvalidPsbt.into());
		}

		// input must be identical (order doesn't matter here)
		// new input may contain extra utxo's (for increased fee payment)
		for input in old_psbt_inputs {
			if !new_psbt_inputs.contains(input) {
				return Err(Error::<T>::InvalidPsbt.into());
			}
		}

		// fee check
		let old_fee = old_psbt.fee().map_err(|_| Error::<T>::InvalidPsbt)?;
		let new_fee = new_psbt.fee().map_err(|_| Error::<T>::InvalidPsbt)?;
		if new_fee <= old_fee {
			return Err(Error::<T>::InvalidPsbt.into());
		}

		// output must be identical except change (order doesn't matter here)
		let old_outputs_map = old_psbt_outputs
			.into_iter()
			.map(|output| -> Result<(BoundedBitcoinAddress, U256), DispatchError> {
				Ok((
					BoundedVec::try_from(
						Self::try_convert_to_address_from_script(output.script_pubkey.as_script())?
							.to_string()
							.as_bytes()
							.to_vec(),
					)
					.map_err(|_| Error::<T>::InvalidBitcoinAddress)?,
					U256::from(output.value.to_sat()),
				))
			})
			.collect::<Result<BTreeMap<BoundedBitcoinAddress, U256>, DispatchError>>()?;

		for output in new_psbt_outputs {
			let to = BoundedVec::try_from(
				Self::try_convert_to_address_from_script(output.script_pubkey.as_script())?
					.to_string()
					.as_bytes()
					.to_vec(),
			)
			.map_err(|_| Error::<T>::InvalidBitcoinAddress)?;

			if let Some(old_amount) = old_outputs_map.get(&to) {
				let new_amount = U256::from(output.value.to_sat());
				match request_type {
					RequestType::Migration | RequestType::Rollback => {
						let fee_diff = U256::from(
							new_fee.checked_sub(old_fee).ok_or(Error::<T>::InvalidPsbt)?.to_sat(),
						);
						let amount_diff =
							old_amount.checked_sub(new_amount).ok_or(Error::<T>::InvalidPsbt)?;
						if fee_diff != amount_diff {
							return Err(Error::<T>::InvalidPsbt.into());
						}
					},
					_ => {
						// user output amount must be identical
						if to != system_vault && new_amount != *old_amount {
							return Err(Error::<T>::InvalidPsbt.into());
						}
					},
				}
			} else {
				// every single output should match and exist for migration requests
				if matches!(request_type, RequestType::Migration) {
					return Err(Error::<T>::InvalidPsbt.into());
				}
				// which means that a change position has been included.
				// the address must match with the system vault.
				if to != system_vault {
					return Err(Error::<T>::InvalidPsbt.into());
				}
			}
		}

		Ok(())
	}

	/// Try to verify PSBT outputs with the given `SocketMessage`'s.
	pub fn try_psbt_output_verification(
		psbt: &Psbt,
		unchecked_outputs: Vec<(BoundedBitcoinAddress, Vec<UnboundedBytes>)>,
	) -> Result<(Vec<SocketMessage>, Vec<UnboundedBytes>), DispatchError> {
		let psbt_outputs = &psbt.unsigned_tx.output;
		// output length must match.
		if psbt_outputs.len() != unchecked_outputs.len() {
			return Err(Error::<T>::InvalidPsbt.into());
		}
		// for normal requests, at least 1 output is required.
		// one or more for outbound refunds. change position may exist (=system vault)
		if psbt_outputs.len() < 1 {
			return Err(Error::<T>::InvalidPsbt.into());
		}
		let current_round = T::RegistrationPool::get_current_round();
		let system_vault = T::RegistrationPool::get_system_vault(current_round)
			.ok_or(Error::<T>::SystemVaultDNE)?;

		let mut deserialized_msgs = vec![];
		let mut serialized_msgs = vec![];
		let mut msg_sequences = vec![];

		let unchecked_outputs_map: BTreeMap<BoundedBitcoinAddress, Vec<UnboundedBytes>> =
			unchecked_outputs.into_iter().collect();

		for output in psbt_outputs {
			let to: BoundedBitcoinAddress = BoundedVec::try_from(
				Self::try_convert_to_address_from_script(output.script_pubkey.as_script())?
					.to_string()
					.as_bytes()
					.to_vec(),
			)
			.map_err(|_| Error::<T>::InvalidBitcoinAddress)?;

			if let Some(socket_messages) = unchecked_outputs_map.get(&to) {
				if to == system_vault {
					// Meaningless PSBT. No BRP event included.
					if psbt_outputs.len() == 1 {
						return Err(Error::<T>::InvalidPsbt.into());
					}

					if !socket_messages.is_empty() {
						return Err(Error::<T>::InvalidUncheckedOutput.into());
					}
				} else {
					// verify socket messages
					let mut amount = U256::default();
					for serialized_msg in socket_messages {
						let msg = Self::try_decode_socket_message(serialized_msg)
							.map_err(|_| Error::<T>::InvalidSocketMessage)?;
						let msg_hash = Self::hash_bytes(
							&UserRequest::new(msg.ins_code.clone(), msg.params.clone()).encode(),
						);
						if msg_sequences.contains(&msg.req_id.sequence) {
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
						if let Some(refund) =
							T::RegistrationPool::get_refund_address(&msg.params.to.into())
						{
							if to != refund {
								return Err(Error::<T>::InvalidSocketMessage.into());
							}
						} else {
							return Err(Error::<T>::UserDNE.into());
						}

						deserialized_msgs.push(msg.clone());
						serialized_msgs.push(serialized_msg.clone());
						msg_sequences.push(msg.req_id.sequence);
						amount = amount.checked_add(msg.params.amount).unwrap();
					}
					// verify psbt output
					let psbt_amount = U256::from(output.value.to_sat());
					if psbt_amount != amount {
						return Err(Error::<T>::InvalidPsbt.into());
					}
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
			Err(_) => Err(()),
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
			Err(_) => Err(()),
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
				_ => Err(()),
			},
			Err(_) => Err(()),
		}
	}
}
