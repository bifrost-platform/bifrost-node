use super::pallet::*;
use crate::{
	HashKeyRequest, RequestInfo, RequestType, SocketMessage, TxInfo, UserRequest,
	BITCOIN_SOCKET_TXS_FUNCTION_SELECTOR, CALL_GAS_LIMIT, SOCKET_GET_REQUEST_FUNCTION_SELECTOR,
};
use bp_btc_relay::{
	blaze::{SelectionStrategy, UtxoInfoWithSize},
	traits::{BlazeManager, PoolManager, SocketQueueManager, SocketVerifier},
	utils::estimate_finalized_input_size,
	Address, BoundedBitcoinAddress, Hash, Psbt, PsbtExt, Script, Secp256k1, Txid, UnboundedBytes,
};
use bp_staking::traits::Authorities;
use ethabi_decode::{ParamKind, Token};
use frame_support::ensure;
use miniscript::{
	bitcoin::{
		absolute::LockTime,
		bip32::{DerivationPath, Fingerprint},
		psbt::{Input, Output},
		transaction::Version,
		Amount, OutPoint, PublicKey, ScriptBuf, Sequence, Transaction, TxIn, TxOut, Witness,
	},
	Descriptor, ForEachKey,
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

impl<T: Config> SocketVerifier<T::AccountId> for Pallet<T>
where
	T: Config,
	T::AccountId: Into<H160>,
	H160: Into<T::AccountId>,
{
	fn verify_socket_message(msg: &UnboundedBytes) -> Result<(), DispatchError> {
		// the bytes should be a valid socket message
		let msg =
			Self::try_decode_socket_message(msg).map_err(|_| Error::<T>::InvalidSocketMessage)?;
		// the socket message should be valid onchain
		let msg_hash =
			Self::hash_bytes(&UserRequest::new(msg.ins_code.clone(), msg.params.clone()).encode());
		let request_info = Self::try_get_request(&msg.encode_req_id())?;
		// the socket message should be valid
		if !request_info.is_msg_hash(msg_hash) {
			#[cfg(not(feature = "runtime-benchmarks"))]
			return Err(Error::<T>::InvalidSocketMessage.into());
		}
		// the socket message should be accepted
		if !request_info.is_accepted() || !msg.is_accepted() {
			#[cfg(not(feature = "runtime-benchmarks"))]
			return Err(Error::<T>::InvalidSocketMessage.into());
		}
		// the socket message should be outbound
		if !msg.is_outbound(
			<T as pallet_evm::Config>::ChainId::get() as u32,
			T::RegistrationPool::get_bitcoin_chain_id(),
		) {
			#[cfg(not(feature = "runtime-benchmarks"))]
			return Err(Error::<T>::InvalidSocketMessage.into());
		}
		// the socket message should not be submitted yet
		if SocketMessages::<T>::get(&msg.req_id.sequence).is_some() {
			#[cfg(not(feature = "runtime-benchmarks"))]
			return Err(Error::<T>::SocketMessageAlreadySubmitted.into());
		}
		Ok(())
	}
}

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
		if T::Blaze::is_activated() {
			if !T::Relayers::is_authority(authority_id) {
				return Err(InvalidTransaction::BadSigner.into());
			}
			Ok(())
		} else {
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

	fn replace_authority(old: &T::AccountId, new: &T::AccountId) {
		// replace authority in pending requests
		<PendingRequests<T>>::iter().for_each(|(_, mut request)| {
			request.replace_authority(old, new);
		});
		// replace authority in rollback requests (if not approved yet)
		<RollbackRequests<T>>::iter().for_each(|(_, mut request)| {
			if !request.is_approved {
				request.replace_authority(old, new);
			}
		});
	}

	fn get_max_fee_rate() -> u64 {
		<MaxFeeRate<T>>::get()
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_max_fee_rate(rate: u64) {
		<MaxFeeRate<T>>::put(rate);
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
		#[cfg(not(feature = "runtime-benchmarks"))]
		return Ok(Address::from_script(script, T::RegistrationPool::get_bitcoin_network())
			.map_err(|_| Error::<T>::InvalidBitcoinAddress)?);

		#[cfg(feature = "runtime-benchmarks")]
		{
			use bp_btc_relay::Network;
			Ok(Address::from_script(script, Network::Regtest)
				.map_err(|_| Error::<T>::InvalidBitcoinAddress)?)
		}
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

	/// Estimate the finalized vsize from the given PSBT.
	fn estimate_finalized_vb(psbt: &Psbt) -> Result<u64, DispatchError> {
		let mut total_vb = 10; // version(4) + locktime(4) + input_count(1) + output_count(1)

		for (i, input) in psbt.inputs.iter().enumerate() {
			let txin = psbt.unsigned_tx.input.get(i).ok_or(Error::<T>::InvalidPsbt)?;
			let input_vb = estimate_finalized_input_size(
				input.witness_script.as_ref().ok_or(Error::<T>::InvalidPsbt)?,
				Some(txin),
			)
			.ok_or(Error::<T>::InvalidPsbt)?;
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
				let fee_diff = U256::from(
					new_fee.checked_sub(old_fee).ok_or(Error::<T>::InvalidPsbt)?.to_sat(),
				);
				let amount_diff =
					old_amount.checked_sub(new_amount).ok_or(Error::<T>::InvalidPsbt)?;
				match request_type {
					RequestType::Migration => {
						// fees are subtracted from the system vault output
						if to == system_vault && fee_diff != amount_diff {
							return Err(Error::<T>::InvalidPsbt.into());
						}
					},
					RequestType::Rollback => {
						// fees are subtracted from the user output
						if to != system_vault && fee_diff != amount_diff {
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

						if msg_sequences.contains(&msg.req_id.sequence) {
							return Err(Error::<T>::InvalidSocketMessage.into());
						}
						Self::verify_socket_message(&serialized_msg)?;

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
						amount = amount
							.checked_add(msg.params.amount)
							.ok_or_else(|| <Error<T>>::U256OverFlowed)?;
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

	/// Filter unregistered outbounds & deserialize registered.
	pub fn filter_unregistered_outbounds(
		mut outbound_pool: Vec<UnboundedBytes>,
	) -> (Vec<UnboundedBytes>, Vec<(SocketMessage, ScriptBuf)>) {
		let mut unregistered = vec![];

		let outbound_requests = outbound_pool
			.iter()
			.filter_map(|x| match Self::try_decode_socket_message(x) {
				Ok(msg) => match T::RegistrationPool::get_refund_address(&msg.params.to.into()) {
					Some(refund) => {
						let script_pubkey =
							Self::try_convert_to_address_from_vec(refund).unwrap().script_pubkey();
						Some((msg, script_pubkey))
					},
					None => {
						unregistered.push(x.clone());
						None
					},
				},
				Err(_) => {
					unregistered.push(x.clone());
					None
				},
			})
			.collect::<Vec<_>>();

		// remove unregistered from outbound_pool
		outbound_pool.retain(|x| !unregistered.contains(x));

		(outbound_pool, outbound_requests)
	}

	/// Composite PSBT.
	pub fn composite_psbt(
		selected_utxos: &[UtxoInfoWithSize],
		outbound_requests: &[(SocketMessage, ScriptBuf)],
		target: u64,
		fee_rate: u64,
		selection_strategy: SelectionStrategy,
	) -> Option<Psbt> {
		let input = selected_utxos
			.iter()
			.map(|x| {
				let txid = {
					let mut slice: [u8; 32] = x.txid.0;
					slice.reverse();
					Txid::from_slice(&slice).unwrap()
				};
				TxIn {
					previous_output: OutPoint::new(txid, x.vout),
					script_sig: ScriptBuf::new(),
					sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
					witness: Witness::new(),
				}
			})
			.collect::<Vec<_>>();

		let mut merged_output = BTreeMap::default();
		for x in outbound_requests.iter() {
			let value = Amount::from_sat(x.0.params.amount.as_u64());
			let script_pubkey = x.1.clone();
			*merged_output.entry(script_pubkey).or_insert(Amount::ZERO) += value;
		}
		let mut output = merged_output
			.into_iter()
			.map(|(script_pubkey, value)| TxOut { value, script_pubkey })
			.collect::<Vec<_>>();
		if selection_strategy == SelectionStrategy::Knapsack {
			let input_sum = selected_utxos.iter().map(|x| x.amount).sum::<u64>();
			if input_sum - 546 > target {
				let input_size_sum = selected_utxos.iter().map(|x| x.input_vbytes).sum::<u64>();
				let output_size_sum = output.iter().map(|x| x.size() as u64).sum::<u64>() + 43;
				let estimated_size = 11 + input_size_sum + output_size_sum;
				let fee = fee_rate * estimated_size;

				let change_amount = input_sum - target - fee;
				let system_vault =
					T::RegistrationPool::get_system_vault(T::RegistrationPool::get_current_round())
						.unwrap();
				let system_vault = Self::try_convert_to_address_from_vec(system_vault).unwrap();
				output.push(TxOut {
					value: Amount::from_sat(change_amount),
					script_pubkey: system_vault.script_pubkey(),
				})
			}
		}

		let tx = Transaction {
			version: Version::TWO,
			lock_time: LockTime::ZERO,
			input,
			output: output.clone(),
		};

		match Psbt::from_unsigned_tx(tx) {
			Ok(mut psbt) => {
				let psbt_inputs = selected_utxos
					.iter()
					.map(|x| {
						let descriptor = Descriptor::<PublicKey>::from_str(&x.descriptor).unwrap();
						let mut psbt_input = Input::default();
						psbt_input.witness_utxo = Some(TxOut {
							value: Amount::from_sat(x.amount),
							script_pubkey: descriptor.script_pubkey(),
						});
						psbt_input.witness_script = Some(descriptor.script_code().unwrap());

						let mut derivation = BTreeMap::new();
						descriptor.for_each_key(|x| {
							derivation.insert(
								x.inner,
								(Fingerprint::default(), DerivationPath::default()),
							);
							true
						});
						psbt_input.bip32_derivation = derivation;
						psbt_input
					})
					.collect::<Vec<_>>();
				psbt.inputs = psbt_inputs;
				psbt.outputs = vec![Output::default(); output.len()];
				Some(psbt)
			},
			Err(_) => None,
		}
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

		#[cfg(not(feature = "runtime-benchmarks"))]
		{
			Ok(Self::try_decode_request_info(&Self::try_evm_call(caller, socket, &calldata)?)
				.map_err(|_| Error::<T>::InvalidRequestInfo)?)
		}

		#[cfg(feature = "runtime-benchmarks")]
		{
			let _ = Self::try_evm_call(caller, socket, &calldata);
			Ok(RequestInfo {
				field: vec![U256::from(5)],
				msg_hash: H256([0; 32]),
				registered_time: U256::from(0),
			})
		}
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

		#[cfg(not(feature = "runtime-benchmarks"))]
		return Ok(Self::try_decode_tx_info(&Self::try_evm_call(
			caller,
			bitcoin_socket,
			&calldata,
		)?)
		.map_err(|_| Error::<T>::InvalidTxInfo)?);

		#[cfg(feature = "runtime-benchmarks")]
		{
			use crate::RequestID;
			let _ =
				Self::try_decode_tx_info(&Self::try_evm_call(caller, bitcoin_socket, &calldata)?);
			Ok(TxInfo {
				to: Default::default(),
				amount: Default::default(),
				vote_count: Default::default(),
				request_id: RequestID {
					chain: vec![],
					round_id: Default::default(),
					sequence: Default::default(),
				},
			})
		}
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
