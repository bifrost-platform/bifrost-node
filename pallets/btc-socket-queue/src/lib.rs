#![cfg_attr(not(feature = "std"), no_std)]

mod pallet;
use ethabi_decode::Token;
pub use pallet::pallet::*;

pub mod weights;
use weights::WeightInfo;

use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

use bp_multi_sig::{Address, MULTI_SIG_MAX_ACCOUNTS};
use sp_core::{ConstU32, RuntimeDebug, H160, H256, U256};
use sp_runtime::BoundedBTreeMap;
use sp_std::{vec, vec::Vec};

#[derive(Decode, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
/// The submitted PSBT information of a outbound request(s).
pub struct PsbtRequest<AccountId> {
	/// The submitted initial unsigned PSBT (in bytes).
	pub unsigned_psbt: Vec<u8>,
	/// The submitted signed PSBT's (in bytes).
	pub signed_psbts: BoundedBTreeMap<AccountId, Vec<u8>, ConstU32<MULTI_SIG_MAX_ACCOUNTS>>,
	/// The submitted `SocketMessage`'s of this request. It is ordered by the PSBT's tx outputs.
	pub socket_messages: Vec<Vec<u8>>,
}

impl<AccountId: PartialEq + Clone + Ord> PsbtRequest<AccountId> {
	/// Instantiates a new `OutboundRequest` instance.
	pub fn new(unsigned_psbt: Vec<u8>, socket_messages: Vec<Vec<u8>>) -> Self {
		Self { unsigned_psbt, signed_psbts: BoundedBTreeMap::default(), socket_messages }
	}

	/// Check if the given authority has already submitted a signed PSBT.
	pub fn is_authority_submitted(&self, authority_id: &AccountId) -> bool {
		self.signed_psbts.contains_key(authority_id)
	}

	/// Check if the given signed PSBT is already submitted by an authority.
	pub fn is_signed_psbt_submitted(&self, psbt: &Vec<u8>) -> bool {
		self.signed_psbts.values().cloned().collect::<Vec<Vec<u8>>>().contains(psbt)
	}

	/// Check if the given PSBT matches with the initial unsigned PSBT.
	pub fn is_unsigned_psbt(&self, psbt: &Vec<u8>) -> bool {
		self.unsigned_psbt.eq(psbt)
	}

	/// Insert the signed PSBT submitted by the authority.
	pub fn insert_signed_psbt(
		&mut self,
		authority_id: AccountId,
		psbt: Vec<u8>,
	) -> Result<Option<Vec<u8>>, (AccountId, Vec<u8>)> {
		self.signed_psbts.try_insert(authority_id, psbt)
	}
}

#[derive(Decode, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
/// The message payload for unsigned PSBT submission.
pub struct UnsignedPsbtMessage<AccountId> {
	/// The authority's account address.
	pub authority_id: AccountId,
	/// The emitted `SocketMessage`'s (in bytes).
	pub socket_messages: Vec<Vec<u8>>,
	/// The unsigned PSBT (in bytes).
	pub psbt: Vec<u8>,
}

#[derive(Decode, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
/// The message payload for signed PSBT submission.
pub struct SignedPsbtMessage<AccountId> {
	/// The authority's account address.
	pub authority_id: AccountId,
	/// The unsigned PSBT (in bytes).
	pub unsigned_psbt: Vec<u8>,
	/// The signed PSBT (in bytes).
	pub signed_psbt: Vec<u8>,
}

#[derive(Decode, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
/// The message payload for PSBT finalization.
pub struct FinalizePsbtMessage<AccountId> {
	/// The authority's account address.
	pub authority_id: AccountId,
	/// The unsigned PSBT hash.
	pub psbt_hash: H256,
}

/// The `SocketMessage`'s request ID.
pub struct RequestID {
	/// The source chain.
	pub chain: Vec<u8>,
	/// The round ID.
	pub round_id: U256,
	/// The sequence ID.
	pub sequence: U256,
}

impl TryFrom<Vec<Token>> for RequestID {
	type Error = ();

	fn try_from(token: Vec<Token>) -> Result<Self, Self::Error> {
		Ok(Self {
			chain: token[0].clone().to_fixed_bytes().ok_or(())?,
			round_id: token[1].clone().to_uint().ok_or(())?,
			sequence: token[2].clone().to_uint().ok_or(())?,
		})
	}
}

/// The `SocketMessage`'s instruction.
pub struct Instruction {
	/// The destination chain.
	pub chain: Vec<u8>,
	/// The method information.
	pub method: Vec<u8>,
}

impl TryFrom<Vec<Token>> for Instruction {
	type Error = ();

	fn try_from(token: Vec<Token>) -> Result<Self, Self::Error> {
		Ok(Self {
			chain: token[0].clone().to_fixed_bytes().ok_or(())?,
			method: token[1].clone().to_fixed_bytes().ok_or(())?,
		})
	}
}

/// The `SocketMessage`'s params.
pub struct TaskParams {
	/// The source chain token index.
	pub token_idx0: Vec<u8>,
	/// The destination chain token index.
	pub token_idx1: Vec<u8>,
	/// The user's refund address.
	pub refund: H160,
	/// The user's address.
	pub to: H160,
	/// The bridge amount.
	pub amount: U256,
	/// Extra variants.
	pub variants: Vec<u8>,
}

impl TryFrom<Vec<Token>> for TaskParams {
	type Error = ();

	fn try_from(token: Vec<Token>) -> Result<Self, Self::Error> {
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

/// The `SocketMessage`.
pub struct SocketMessage {
	pub req_id: RequestID,
	pub status: U256,
	pub ins_code: Instruction,
	pub params: TaskParams,
}

impl SocketMessage {
	/// Encodes the request ID into bytes.
	pub fn encode_req_id(&self) -> Vec<u8> {
		ethabi_decode::encode(&[
			Token::FixedBytes(self.req_id.chain.clone()),
			Token::Uint(self.req_id.round_id),
			Token::Uint(self.req_id.sequence),
		])
	}

	/// Check if the message status is in `Accepted`.
	pub fn is_accepted(&self) -> bool {
		self.status == U256::from(5)
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

	/// Check if the status is in `Accepted`.
	pub fn is_accepted(&self) -> bool {
		self.field[0] == U256::from(5)
	}
}

impl TryFrom<Vec<Token>> for RequestInfo {
	type Error = ();

	fn try_from(token: Vec<Token>) -> Result<Self, Self::Error> {
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

/// Unchecked PSBT transaction outputs derived by the submitted socket messages.
pub struct UncheckedOutput {
	/// The target Bitcoin address.
	pub to: Address,
	/// The amount to be transferred.
	pub amount: U256, // TODO: 단위 체크 필요 (sat? wei?)
}
