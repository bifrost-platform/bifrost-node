#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]

pub mod traits;

use ethabi_decode::{ParamKind, Token};
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_core::{RuntimeDebug, H160, H256, U256};
use sp_std::{boxed::Box, vec, vec::Vec};

/// Length unbounded bytes type.
pub type UnboundedBytes = Vec<u8>;

/// The function selector of `Socket::get_request()`.
pub const SOCKET_GET_REQUEST_FUNCTION_SELECTOR: &str = "8dac2204";

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

/// The `UserRequest`.
#[derive(Decode, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct UserRequest {
	/// The instruction code.
	pub ins_code: Instruction,
	/// The task parameters.
	pub params: TaskParams,
}

impl UserRequest {
	pub fn new(ins_code: Instruction, params: TaskParams) -> Self {
		Self { ins_code, params }
	}

	/// Encodes into bytes.
	pub fn encode(&self) -> UnboundedBytes {
		ethabi_decode::encode(&[Token::Tuple(vec![
			Token::Tuple(vec![
				Token::FixedBytes(self.ins_code.chain.clone()),
				Token::FixedBytes(self.ins_code.method.clone()),
			]),
			Token::Tuple(vec![
				Token::FixedBytes(self.params.token_idx0.clone()),
				Token::FixedBytes(self.params.token_idx1.clone()),
				Token::Address(self.params.refund),
				Token::Address(self.params.to),
				Token::Uint(self.params.amount),
				Token::Bytes(self.params.variants.clone()),
			]),
		])])
	}
}

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

impl SocketMessage {
	/// Encodes the full socket message into ABI-encoded bytes.
	/// This matches the Socket event ABI structure for proper hash computation.
	pub fn encode(&self) -> UnboundedBytes {
		ethabi_decode::encode(&[Token::Tuple(vec![
			Token::Tuple(vec![
				Token::FixedBytes(self.req_id.chain.clone()),
				Token::Uint(self.req_id.round_id),
				Token::Uint(self.req_id.sequence),
			]),
			Token::Uint(self.status),
			Token::Tuple(vec![
				Token::FixedBytes(self.ins_code.chain.clone()),
				Token::FixedBytes(self.ins_code.method.clone()),
			]),
			Token::Tuple(vec![
				Token::FixedBytes(self.params.token_idx0.clone()),
				Token::FixedBytes(self.params.token_idx1.clone()),
				Token::Address(self.params.refund),
				Token::Address(self.params.to),
				Token::Uint(self.params.amount),
				Token::Bytes(self.params.variants.clone()),
			]),
		])])
	}

	/// Encodes the request ID into bytes.
	pub fn encode_req_id(&self) -> UnboundedBytes {
		ethabi_decode::encode(&[
			Token::FixedBytes(self.req_id.chain.clone()),
			Token::Uint(self.req_id.round_id),
			Token::Uint(self.req_id.sequence),
		])
	}

	/// Check if the message status is in `Requested`.
	pub fn is_requested(&self) -> bool {
		self.status == U256::from(1)
	}

	/// Check if the message status is in `Accepted`.
	pub fn is_accepted(&self) -> bool {
		self.status == U256::from(5)
	}

	/// Check if the message status is in `Rejected`.
	pub fn is_rejected(&self) -> bool {
		self.status == U256::from(6)
	}

	/// Check if the message status is in `Committed`.
	pub fn is_committed(&self) -> bool {
		self.status == U256::from(7)
	}

	/// Check if the message status is in `Rollbacked`.
	pub fn is_rollbacked(&self) -> bool {
		self.status == U256::from(8)
	}

	/// Check if the message is an outbound request.
	pub fn is_outbound(&self, bifrost_chain_id: u32) -> bool {
		if self.req_id.chain.clone().as_slice() != bifrost_chain_id.to_be_bytes() {
			return false;
		}
		true
	}

	/// Check if the message is a Bifrost to Bitcoin outbound request.
	pub fn is_bitcoin_outbound(&self, bifrost_chain_id: u32, bitcoin_chain_id: u32) -> bool {
		if self.req_id.chain.clone().as_slice() != bifrost_chain_id.to_be_bytes() {
			return false;
		}
		if self.ins_code.chain.clone().as_slice() != bitcoin_chain_id.to_be_bytes() {
			return false;
		}
		true
	}
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

#[derive(Debug)]
/// The request information. (Response for the `get_request()` function)
pub struct RequestInfo {
	/// The first element represents the current message status.
	pub field: Vec<U256>,
	/// The hash of the message.
	pub msg_hash: H256,
	/// Emitted time.
	pub registered_time: U256,
}

impl RequestInfo {
	/// Check if the given hash matches with `msg_hash`.
	pub fn is_msg_hash(&self, hash: H256) -> bool {
		self.msg_hash == hash
	}

	/// Check if the status is in `Requested`.
	pub fn is_requested(&self) -> bool {
		self.field[0] == U256::from(1)
	}

	/// Check if the status is in `Accepted`.
	pub fn is_accepted(&self) -> bool {
		self.field[0] == U256::from(5)
	}

	/// Check if the status is in `Rejected`.
	pub fn is_rejected(&self) -> bool {
		self.field[0] == U256::from(6)
	}

	/// Check if the status is in `Committed`.
	pub fn is_committed(&self) -> bool {
		self.field[0] == U256::from(7)
	}

	/// Check if the status is in `Rollbacked`.
	pub fn is_rollbacked(&self) -> bool {
		self.field[0] == U256::from(8)
	}
}

impl TryFrom<UnboundedBytes> for RequestInfo {
	type Error = ();

	fn try_from(bytes: UnboundedBytes) -> Result<Self, Self::Error> {
		match ethabi_decode::decode(
			&[
				ParamKind::FixedArray(Box::new(ParamKind::Uint(8)), 32),
				ParamKind::FixedBytes(32),
				ParamKind::Uint(256),
			],
			&bytes,
		) {
			Ok(token) => Ok(token.clone().try_into()?),
			Err(_) => Err(()),
		}
	}
}

impl TryFrom<Vec<Token>> for RequestInfo {
	type Error = ();

	fn try_from(token: Vec<Token>) -> Result<Self, Self::Error> {
		if token.len() != 3 {
			return Err(());
		}
		let tokenized_field = token[0].clone().to_fixed_array().ok_or(())?;
		let mut field = vec![];
		for token in tokenized_field {
			field.push(token.to_uint().ok_or(())?);
		}
		Ok(RequestInfo {
			field,
			msg_hash: H256::from_slice(&token[1].clone().to_fixed_bytes().ok_or(())?),
			registered_time: token[2].clone().to_uint().ok_or(())?,
		})
	}
}
