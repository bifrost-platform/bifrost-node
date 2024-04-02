use ethabi_decode::{ParamKind, Token};

use bp_multi_sig::{
	traits::PoolManager, Address, BoundedBitcoinAddress, Psbt, Script, UnboundedBytes,
};
use sp_core::{H160, H256, U256};
use sp_io::hashing::keccak_256;
use sp_runtime::DispatchError;
use sp_std::{boxed::Box, prelude::ToOwned, str, str::FromStr, vec, vec::Vec};

use pallet_evm::Runner;
use scale_info::prelude::string::String;

use crate::{RequestInfo, SocketMessage, UncheckedOutput, CALL_FUNCTION_SELECTOR, CALL_GAS_LIMIT};

use super::pallet::*;

impl<T> Pallet<T>
where
	T: Config,
	T::AccountId: Into<H160>,
	H160: Into<T::AccountId>,
{
	/// Try to deserialize the given bytes to a `PSBT` instance.
	pub fn try_get_checked_psbt(psbt: &UnboundedBytes) -> Result<Psbt, DispatchError> {
		Ok(Psbt::deserialize(psbt).map_err(|_| Error::<T>::InvalidPsbt)?)
	}

	/// Try to combine the signed PSBT with the origin. If fails, the given PSBT is considered as invalid.
	pub fn try_signed_psbt_verification(
		origin: &UnboundedBytes,
		signed: &UnboundedBytes,
	) -> Result<(), DispatchError> {
		let mut origin = Self::try_get_checked_psbt(origin)?;
		let s = Self::try_get_checked_psbt(signed)?;
		Ok(origin.combine(s).map_err(|_| Error::<T>::InvalidPsbt)?)
	}

	/// Try to verify the PSBT transaction outputs with the unchecked outputs derived from the submitted socket messages.
	pub fn try_psbt_output_verification(
		psbt: &UnboundedBytes,
		unchecked: Vec<UncheckedOutput>,
	) -> Result<(), DispatchError> {
		let origin = Self::try_get_checked_psbt(&psbt)?.unsigned_tx.output;
		if origin.len() != unchecked.len() {
			return Err(Error::<T>::InvalidPsbt.into());
		}
		// at least 2 outputs required. one for refund and one for system vault.
		if origin.len() < 2 {
			return Err(Error::<T>::InvalidPsbt.into());
		}

		let convert_to_address = |script: &Script| {
			Address::from_script(script, T::RegistrationPool::get_bitcoin_network())
		};

		let system_vault = convert_to_address(origin[0].script_pubkey.as_script())
			.map_err(|_| Error::<T>::InvalidPsbt)?;
		if system_vault != unchecked[0].to {
			return Err(Error::<T>::InvalidPsbt.into());
		}
		for i in 1..origin.len() {
			let to = convert_to_address(origin[i].script_pubkey.as_script())
				.map_err(|_| Error::<T>::InvalidPsbt)?;
			let amount = U256::from(origin[i].value.to_sat());

			if to != unchecked[i].to {
				return Err(Error::<T>::InvalidPsbt.into());
			}
			if amount != unchecked[i].amount {
				return Err(Error::<T>::InvalidPsbt.into());
			}
		}
		Ok(())
	}

	/// Try to verify the submitted socket messages and build unchecked outputs.
	pub fn try_build_unchecked_outputs(
		socket_messages: &Vec<UnboundedBytes>,
	) -> Result<(Vec<UncheckedOutput>, Vec<SocketMessage>), DispatchError> {
		let system_vault =
			T::RegistrationPool::get_system_vault().ok_or(Error::<T>::SystemVaultDNE)?;

		// TODO: check length bound
		if socket_messages.is_empty() {
			return Err(Error::<T>::InvalidSocketMessage.into());
		}
		let mut outputs = vec![];

		let convert_to_address = |addr: BoundedBitcoinAddress| {
			// we assume all the registered addresses are valid and checked.
			let addr = str::from_utf8(&addr).expect("Must be valid");
			Address::from_str(addr).expect("Must be valid").assume_checked()
		};

		// we assume the first output to be the utxo repayment
		outputs.push(UncheckedOutput {
			to: convert_to_address(system_vault),
			amount: Default::default(),
		});

		let mut msgs = vec![];
		let mut msg_hashes = vec![];
		for msg in socket_messages {
			let msg_hash = Self::hash_bytes(msg);
			if msg_hashes.contains(&msg_hash) {
				return Err(Error::<T>::InvalidSocketMessage.into());
			}

			let msg = Self::try_decode_socket_message(msg)
				.map_err(|_| Error::<T>::InvalidSocketMessage)?;
			let request_info = Self::try_get_request(&msg.encode_req_id())?;

			// the socket message should be valid
			if !request_info.is_msg_hash(msg_hash) {
				return Err(Error::<T>::InvalidSocketMessage.into());
			}
			if !request_info.is_accepted() || !msg.is_accepted() {
				return Err(Error::<T>::InvalidSocketMessage.into());
			}
			if Self::socket_messages(&msg.req_id.sequence).is_some() {
				return Err(Error::<T>::SocketMessageAlreadySubmitted.into());
			}
			// TODO: check if request is outbound sequence
			// TODO: check if asset is unified btc

			// the user must exist in the pool
			let to = T::RegistrationPool::get_refund_address(&msg.params.to.into())
				.ok_or(Error::<T>::UserDNE)?;
			outputs.push(UncheckedOutput { to: convert_to_address(to), amount: msg.params.amount });

			msgs.push(msg);
			msg_hashes.push(msg_hash);
		}
		Ok((outputs, msgs))
	}

	/// Hash the given bytes.
	pub fn hash_bytes(bytes: &UnboundedBytes) -> H256 {
		H256::from(keccak_256(bytes))
	}

	/// Try to get the `RequestInfo` by the given `req_id`.
	pub fn try_get_request(req_id: &UnboundedBytes) -> Result<RequestInfo, DispatchError> {
		let caller = <Authority<T>>::get().ok_or(Error::<T>::AuthorityDNE)?;
		let socket = <Socket<T>>::get().ok_or(Error::<T>::SocketDNE)?;

		let mut calldata: String = CALL_FUNCTION_SELECTOR.to_owned(); // get_request()
		calldata.push_str(&array_bytes::bytes2hex("", req_id));

		let info = <T as pallet_evm::Config>::Runner::call(
			caller.into(),
			socket.into(),
			hex::decode(&calldata).map_err(|_| Error::<T>::InvalidCalldata)?,
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
		.map_err(|_| Error::<T>::SocketMessageDNE)?;

		Ok(Self::try_decode_request_info(&info.value)
			.map_err(|_| Error::<T>::InvalidRequestInfo)?)
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
