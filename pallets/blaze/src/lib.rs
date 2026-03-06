#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]

#[cfg(any(test, feature = "runtime-benchmarks"))]
mod benchmarking;
pub mod migrations;
#[cfg(test)]
mod mock;

mod pallet;
pub mod weights;

pub use pallet::pallet::*;
pub use weights::WeightInfo;

use bp_btc_relay::{
	blaze::{UtxoInfo, UtxoInfoWithSize},
	UnboundedBytes,
};
use bp_staking::MAX_AUTHORITIES;
use ethabi_decode::{ParamKind, Token};
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode};
use scale_info::TypeInfo;
use sp_core::{ConstU32, RuntimeDebug, H160, H256, U256};
use sp_runtime::BoundedVec;
use sp_std::{boxed::Box, vec, vec::Vec};

pub(crate) const LOG_TARGET: &'static str = "runtime::blaze";

// syntactic sugar for logging.
#[macro_export]
macro_rules! log {
	($level:tt, $patter:expr $(, $values:expr)* $(,)?) => {
		log::$level!(
			target: crate::LOG_TARGET,
			concat!("[{:?}] 💸 ", $patter), <frame_system::Pallet<T>>::block_number() $(, $values)*
		)
	};
}

#[derive(Eq, PartialEq, Clone, Encode, Decode, DecodeWithMemTracking, RuntimeDebug, TypeInfo)]
/// The status of a UTXO.
pub enum UtxoStatus {
	/// The UTXO is not confirmed.
	Unconfirmed,
	/// The UTXO is available.
	Available,
	/// The UTXO is locked to a PSBT.
	Locked,
	/// The UTXO is used.
	Used,
}

#[derive(Decode, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
/// A UTXO with its status and voters.
pub struct Utxo<AccountId> {
	/// The UTXO information.
	pub inner: UtxoInfoWithSize,
	/// The status of the UTXO.
	pub status: UtxoStatus,
	/// The voters of the UTXO.
	pub voters: BoundedVec<AccountId, ConstU32<MAX_AUTHORITIES>>,
}

#[derive(Decode, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
/// A bundle of TXOs with their voters.
pub struct BTCTransaction<AccountId> {
	/// Bundled and sorted UTXO hashes.
	pub inputs: Vec<UtxoInfoWithSize>,
	/// Voters of the UTXOs.
	pub voters: BoundedVec<AccountId, ConstU32<MAX_AUTHORITIES>>,
}

#[derive(Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
/// A submission of UTXOs.
pub struct UtxoSubmission<AccountId> {
	/// The authority id.
	pub authority_id: AccountId,
	/// The UTXOs to submit.
	pub utxos: Vec<UtxoInfo>,
}

#[derive(Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
/// A submission of txid which is broadcasted.
pub struct BroadcastSubmission<AccountId> {
	/// The authority id.
	pub authority_id: AccountId,
	/// The txid of the PSBT.
	pub txid: H256,
}

#[derive(Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
/// A submission of a fee rate.
pub struct FeeRateSubmission<AccountId, BlockNumber> {
	/// The authority id.
	pub authority_id: AccountId,
	/// The long term fee rate (sat/vb).
	pub lt_fee_rate: u64,
	/// The fee rate (sat/vb).
	pub fee_rate: u64,
	/// The deadline of the submission. Used to filter out expired signatures.
	pub deadline: BlockNumber,
}

#[derive(Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
/// A submission of Socket messages.
pub struct SocketMessagesSubmission<AccountId> {
	/// The authority id.
	pub authority_id: AccountId,
	/// The Socket messages.
	pub messages: Vec<UnboundedBytes>,
}

// TODO: remove later
/// The `SocketMessage`.
#[derive(Decode, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct SocketMessage {
	/// The request ID.
	pub req_id: RequestID,
	/// The status of the message.
	pub status: U256,
	/// The instruction code.
	pub ins_code: Instruction,
	/// The task parameters.
	pub params: TaskParams,
}

impl TryFrom<UnboundedBytes> for SocketMessage {
	type Error = ();

	fn try_from(bytes: UnboundedBytes) -> Result<Self, Self::Error> {
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
			&bytes,
		) {
			Ok(socket) => match &socket[0] {
				Token::Tuple(msg) => Ok(msg.clone().try_into()?),
				_ => Err(()),
			},
			Err(_) => Err(()),
		}
	}
}

impl TryFrom<Vec<Token>> for SocketMessage {
	type Error = ();

	fn try_from(token: Vec<Token>) -> Result<Self, Self::Error> {
		if token.len() != 4 {
			return Err(());
		}

		let req_id = match &token[0] {
			Token::Tuple(token) => token.clone().try_into()?,
			_ => return Err(()),
		};
		let status = token[1].clone().to_uint().ok_or(())?;
		let ins_code = match &token[2] {
			Token::Tuple(token) => token.clone().try_into()?,
			_ => return Err(()),
		};
		let params = match &token[3] {
			Token::Tuple(token) => token.clone().try_into()?,
			_ => return Err(()),
		};
		Ok(SocketMessage { req_id, status, ins_code, params })
	}
}

/// The `SocketMessage`'s request ID.
#[derive(Decode, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct RequestID {
	/// The source chain.
	pub chain: UnboundedBytes,
	/// The round ID.
	pub round_id: U256,
	/// The sequence ID.
	pub sequence: U256,
}

impl TryFrom<Vec<Token>> for RequestID {
	type Error = ();

	fn try_from(token: Vec<Token>) -> Result<Self, Self::Error> {
		if token.len() != 3 {
			return Err(());
		}
		Ok(Self {
			chain: token[0].clone().to_fixed_bytes().ok_or(())?,
			round_id: token[1].clone().to_uint().ok_or(())?,
			sequence: token[2].clone().to_uint().ok_or(())?,
		})
	}
}

/// The `SocketMessage`'s instruction.
#[derive(Decode, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct Instruction {
	/// The destination chain.
	pub chain: UnboundedBytes,
	/// The method information.
	pub method: UnboundedBytes,
}

impl TryFrom<Vec<Token>> for Instruction {
	type Error = ();

	fn try_from(token: Vec<Token>) -> Result<Self, Self::Error> {
		if token.len() != 2 {
			return Err(());
		}
		Ok(Self {
			chain: token[0].clone().to_fixed_bytes().ok_or(())?,
			method: token[1].clone().to_fixed_bytes().ok_or(())?,
		})
	}
}

/// The `SocketMessage`'s params.
#[derive(Decode, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct TaskParams {
	/// The source chain token index.
	pub token_idx0: UnboundedBytes,
	/// The destination chain token index.
	pub token_idx1: UnboundedBytes,
	/// The user's refund address.
	pub refund: H160,
	/// The user's address.
	pub to: H160,
	/// The bridge amount.
	pub amount: U256,
	/// Extra variants.
	pub variants: UnboundedBytes,
}

impl TryFrom<Vec<Token>> for TaskParams {
	type Error = ();

	fn try_from(token: Vec<Token>) -> Result<Self, Self::Error> {
		if token.len() != 6 {
			return Err(());
		}
		Ok(TaskParams {
			token_idx0: token[0].clone().to_fixed_bytes().ok_or(())?,
			token_idx1: token[1].clone().to_fixed_bytes().ok_or(())?,
			refund: token[2].clone().to_address().ok_or(())?,
			to: token[3].clone().to_address().ok_or(())?,
			amount: token[4].clone().to_uint().ok_or(())?,
			variants: token[5].clone().to_bytes().ok_or(())?,
		})
	}
}
