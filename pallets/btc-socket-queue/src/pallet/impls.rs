use ethabi_decode::{ParamKind, Token};

use bp_multi_sig::Psbt;
use scale_info::prelude::string::String;
use sp_core::{H160, H256, U256};
use sp_io::hashing::keccak_256;
use sp_runtime::{traits::IdentifyAccount, DispatchError};
use sp_std::{boxed::Box, prelude::ToOwned, vec, vec::Vec};

use pallet_evm::Runner;

use crate::{RequestInfo, SocketMessage};

use super::pallet::*;

impl<T: Config> Pallet<T> {
	/// Try to deserialize the given bytes to a `PSBT` instance.
	pub fn try_get_checked_psbt(psbt: &Vec<u8>) -> Result<Psbt, DispatchError> {
		Ok(Psbt::deserialize(psbt).map_err(|_| Error::<T>::InvalidPsbt)?)
	}

	/// Try to combine the signed PSBT with the origin. If fails, the given PSBT is considered as invalid.
	pub fn verify_signed_psbt(origin: &Vec<u8>, signed: &Vec<u8>) -> Result<(), DispatchError> {
		let mut o = Self::try_get_checked_psbt(origin)?;
		let s = Self::try_get_checked_psbt(signed)?;
		Ok(o.combine(s).map_err(|_| Error::<T>::InvalidPsbt)?)
	}

	pub fn hash_bytes(bytes: &Vec<u8>) -> H256 {
		H256(keccak_256(bytes))
	}
}

impl<T> Pallet<T>
where
	T: Config,
	T::AccountId: Into<H160>,
{
	/// Try to get the `SocketMessage` by the given `req_id`.
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
