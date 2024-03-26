use ethabi_decode::{ParamKind, Token};

use bp_multi_sig::{traits::PoolManager, Address, BoundedBitcoinAddress, Psbt};
use sp_core::{H160, H256, U256};
use sp_io::hashing::keccak_256;
use sp_runtime::{traits::IdentifyAccount, DispatchError};
use sp_std::{boxed::Box, prelude::ToOwned, str, str::FromStr, vec, vec::Vec};

use pallet_evm::Runner;
use scale_info::prelude::string::String;

use crate::{PsbtOutput, RequestInfo, SocketMessage};

use super::pallet::*;

impl<T> Pallet<T>
where
	T: Config,
	T::AccountId: Into<H160>,
	H160: Into<T::AccountId>,
{
	/// Try to deserialize the given bytes to a `PSBT` instance.
	pub fn try_get_checked_psbt(psbt: &Vec<u8>) -> Result<Psbt, DispatchError> {
		Ok(Psbt::deserialize(psbt).map_err(|_| Error::<T>::InvalidPsbt)?)
	}

	/// Try to combine the signed PSBT with the origin. If fails, the given PSBT is considered as invalid.
	pub fn try_signed_psbt_verification(
		origin: &Vec<u8>,
		signed: &Vec<u8>,
	) -> Result<(), DispatchError> {
		let mut origin = Self::try_get_checked_psbt(origin)?;
		let s = Self::try_get_checked_psbt(signed)?;
		Ok(origin.combine(s).map_err(|_| Error::<T>::InvalidPsbt)?)
	}

	pub fn try_psbt_output_verification(
		psbt: &Vec<u8>,
		outputs: Vec<PsbtOutput>,
	) -> Result<(), DispatchError> {
		let origin = Self::try_get_checked_psbt(&psbt)?.unsigned_tx.output;
		if origin.len() != outputs.len() {
			return Err(Error::<T>::InvalidPsbt.into());
		}
		// at least 2 outputs required. one for refund and one for system vault.
		if origin.len() < 2 {
			return Err(Error::<T>::InvalidPsbt.into());
		}
		let system_vault = Address::from_script(
			origin[0].script_pubkey.as_script(),
			T::RegistrationPool::get_bitcoin_network(),
		)
		.map_err(|_| Error::<T>::InvalidPsbt)?;
		if system_vault != outputs[0].to {
			return Err(Error::<T>::InvalidPsbt.into());
		}
		for i in 1..origin.len() {
			let to = Address::from_script(
				origin[i].script_pubkey.as_script(),
				T::RegistrationPool::get_bitcoin_network(),
			)
			.map_err(|_| Error::<T>::InvalidPsbt)?;

			let amount = U256::from(origin[i].value.to_sat());

			if to != outputs[i].to {
				return Err(Error::<T>::InvalidPsbt.into());
			}
			if amount != outputs[i].amount {
				return Err(Error::<T>::InvalidPsbt.into());
			}
		}
		Ok(())
	}

	pub fn try_build_psbt_outputs(
		socket_messages: Vec<Vec<u8>>,
	) -> Result<Vec<PsbtOutput>, DispatchError> {
		let system_vault =
			T::RegistrationPool::get_system_vault().ok_or(Error::<T>::SystemVaultDne)?;

		// TODO: check length bound
		if socket_messages.is_empty() {
			return Err(Error::<T>::InvalidSocketMessage.into());
		}
		let mut outputs = vec![];

		let to_address = |addr: BoundedBitcoinAddress| {
			// we assume all the registered addresses are valid and checked.
			let addr = str::from_utf8(&addr).expect("Must be valid");
			Address::from_str(addr).expect("Must be valid").assume_checked()
		};

		// we assume the first output to be the utxo repayment
		outputs.push(PsbtOutput { to: to_address(system_vault), amount: Default::default() });
		for msg in socket_messages {
			let msg_hash = Self::hash_bytes(&msg);
			let msg = Self::try_decode_socket_message(&msg)
				.map_err(|_| Error::<T>::InvalidSocketMessage)?;
			let request_info = Self::try_get_request(&msg.encode_req_id())?;

			// the socket message should be valid
			if !request_info.is_msg_hash(msg_hash) {
				return Err(Error::<T>::InvalidSocketMessage.into());
			}
			if !msg.is_accepted() {
				return Err(Error::<T>::InvalidSocketMessage.into());
			}

			// the user must exist in the pool
			let to = T::RegistrationPool::get_refund_address(&msg.params.to.into())
				.ok_or(Error::<T>::UserDNE)?;
			outputs.push(PsbtOutput { to: to_address(to), amount: msg.params.amount });
		}
		Ok(outputs)
	}

	pub fn hash_bytes(bytes: &Vec<u8>) -> H256 {
		H256(keccak_256(bytes))
	}

	/// Try to get the `RequestInfo` by the given `req_id`.
	pub fn try_get_request(req_id: &Vec<u8>) -> Result<RequestInfo, DispatchError> {
		let caller = <UnsignedPsbtSubmitter<T>>::get().ok_or(Error::<T>::SubmitterDNE)?;
		let socket = <Socket<T>>::get().ok_or(Error::<T>::SocketDNE)?;

		let mut calldata: String = "8dac2204".to_owned(); // get_request()
		calldata.push_str(&array_bytes::bytes2hex("", req_id));

		let info = <T as pallet_evm::Config>::Runner::call(
			caller.into_account().into(),
			socket.into(),
			hex::decode(&calldata).map_err(|_| Error::<T>::InvalidCalldata)?,
			U256::zero(),
			1_000_000,
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

	pub fn try_decode_request_info(info: &Vec<u8>) -> Result<RequestInfo, ()> {
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

	pub fn try_decode_socket_message(msg: &Vec<u8>) -> Result<SocketMessage, ()> {
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
				Token::Tuple(msg) => {
					let req_id = match &msg[0] {
						Token::Tuple(token) => token.clone().try_into()?,
						_ => return Err(()),
					};
					let status = msg[1].clone().to_uint().ok_or(())?;
					let ins_code = match &msg[2] {
						Token::Tuple(token) => token.clone().try_into()?,
						_ => return Err(()),
					};
					let params = match &msg[3] {
						Token::Tuple(token) => token.clone().try_into()?,
						_ => return Err(()),
					};
					return Ok(SocketMessage { req_id, status, ins_code, params });
				},
				_ => return Err(()),
			},
			Err(_) => return Err(()),
		};
	}
}
