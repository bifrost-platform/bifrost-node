#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]

mod pallet;
use ethabi_decode::Token;
pub use pallet::pallet::*;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[cfg(test)]
mod mock;

pub mod migrations;
pub mod weights;
use weights::WeightInfo;

use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode};
use scale_info::TypeInfo;

use bp_btc_relay::{BoundedBitcoinAddress, UnboundedBytes, MULTI_SIG_MAX_ACCOUNTS};
use bp_cccp::RequestID;
use bp_staking::MAX_AUTHORITIES;
use sp_core::{ConstU32, RuntimeDebug, H160, H256, U256};
use sp_runtime::BoundedBTreeMap;
use sp_std::{vec, vec::Vec};

/// The gas limit used for contract function calls.
const CALL_GAS_LIMIT: u64 = 1_000_000;

/// The function selector of `BitcoinSocket::txs()`.
const BITCOIN_SOCKET_TXS_FUNCTION_SELECTOR: &str = "986ba392";

pub(crate) const LOG_TARGET: &'static str = "runtime::socket-queue";

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

#[derive(Decode, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
/// The submitted PSBT information for a rollback request.
pub struct RollbackRequest<AccountId> {
	/// The origin unsigned PSBT (in bytes).
	pub unsigned_psbt: UnboundedBytes,
	/// The registered user's Bifrost address.
	pub who: AccountId,
	/// The hash of the transaction that contains the output (that should be rollbacked. to: `vault`)
	pub txid: H256,
	/// The output index of the transaction.
	pub vout: U256,
	/// The `to` address of the output. (= `vault`)
	pub to: BoundedBitcoinAddress,
	/// The `amount` of the output.
	pub amount: U256,
	/// The current votes submitted by relayers.
	/// key: The relayer address.
	/// value: The voting side. Approved or not.
	pub votes: BoundedBTreeMap<AccountId, bool, ConstU32<MAX_AUTHORITIES>>,
	/// The current approval of the request.
	/// It'll only be approved when the majority of relayers voted for the request.
	pub is_approved: bool,
}

impl<AccountId: PartialEq + Clone + Ord + sp_std::fmt::Debug> RollbackRequest<AccountId> {
	pub fn new(
		unsigned_psbt: UnboundedBytes,
		who: AccountId,
		txid: H256,
		vout: U256,
		to: BoundedBitcoinAddress,
		amount: U256,
	) -> Self {
		Self {
			unsigned_psbt,
			who,
			txid,
			vout,
			to,
			amount,
			votes: Default::default(),
			is_approved: false,
		}
	}

	/// Replace the authority's vote.
	pub fn replace_authority(&mut self, old: &AccountId, new: &AccountId) {
		if let Some(vote) = self.votes.remove(old) {
			self.votes
				.try_insert(new.clone(), vote)
				.expect("Should not fail as we just removed an element");
		}
	}
}

#[derive(Decode, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
/// The type of the PSBT request.
pub enum RequestType {
	/// PSBT for normal requests.
	Normal,
	/// PSBT for rollback requests.
	Rollback,
	/// PSBT for vault migration requests.
	Migration,
}

#[derive(Decode, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
/// The submitted PSBT information for outbound request(s).
pub struct PsbtRequest<AccountId> {
	/// The submitted origin unsigned PSBT (in bytes).
	pub unsigned_psbt: UnboundedBytes,
	/// The latest combined PSBT with the given signed PSBT's (in bytes).
	pub combined_psbt: UnboundedBytes,
	/// The finalized PSBT with the current combined PSBT (in bytes).
	pub finalized_psbt: UnboundedBytes,
	/// The submitted signed PSBT's (in bytes).
	pub signed_psbts: BoundedBTreeMap<AccountId, UnboundedBytes, ConstU32<MULTI_SIG_MAX_ACCOUNTS>>,
	/// The submitted `SocketMessage`'s of this request. It is ordered by the PSBT's tx outputs.
	/// This will be empty for rollback/migration requests.
	pub socket_messages: Vec<UnboundedBytes>,
	/// The request type of the PSBT.
	pub request_type: RequestType,
}

impl<AccountId: PartialEq + Clone + Ord + sp_std::fmt::Debug> PsbtRequest<AccountId> {
	/// Instantiates a new `PsbtRequest` instance.
	pub fn new(
		unsigned_psbt: UnboundedBytes,
		socket_messages: Vec<UnboundedBytes>,
		request_type: RequestType,
	) -> Self {
		Self {
			combined_psbt: unsigned_psbt.clone(),
			unsigned_psbt,
			finalized_psbt: UnboundedBytes::default(),
			signed_psbts: BoundedBTreeMap::default(),
			socket_messages,
			request_type,
		}
	}

	/// Check if the given authority has already submitted a signed PSBT.
	pub fn is_authority_submitted(&self, authority_id: &AccountId) -> bool {
		self.signed_psbts.contains_key(authority_id)
	}

	/// Check if the given signed PSBT is already submitted by an authority.
	pub fn is_signed_psbt_submitted(&self, psbt: &UnboundedBytes) -> bool {
		self.signed_psbts.values().into_iter().any(|x| x == psbt)
	}

	/// Check if the given PSBT matches with the initial unsigned PSBT.
	pub fn is_unsigned_psbt(&self, psbt: &UnboundedBytes) -> bool {
		self.unsigned_psbt.eq(psbt)
	}

	/// Update the latest combined PSBT.
	pub fn set_combined_psbt(&mut self, psbt: UnboundedBytes) {
		self.combined_psbt = psbt;
	}

	/// Update the finalized PSBT.
	pub fn set_finalized_psbt(&mut self, psbt: UnboundedBytes) {
		self.finalized_psbt = psbt;
	}

	/// Replace the authority's signed PSBT.
	pub fn replace_authority(&mut self, old: &AccountId, new: &AccountId) {
		if let Some(psbt) = self.signed_psbts.remove(old) {
			self.signed_psbts
				.try_insert(new.clone(), psbt)
				.expect("Should not fail as we just removed an element");
		}
	}
}

#[derive(Decode, DecodeWithMemTracking, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
/// The message payload for unsigned PSBT submission.
pub struct UnsignedPsbtMessage<AccountId> {
	/// The authority's account address.
	pub authority_id: AccountId,
	/// The PSBT output information.
	/// key: the output `to` address. (=refund / system vault)
	/// value: the `SocketMessage`'s related to the output. (system vault output will have empty socket messages)
	pub outputs: Vec<(BoundedBitcoinAddress, Vec<UnboundedBytes>)>,
	/// The unsigned PSBT (in bytes).
	pub psbt: UnboundedBytes,
}

#[derive(Decode, DecodeWithMemTracking, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
/// The message payload for signed PSBT submission.
pub struct SignedPsbtMessage<AccountId> {
	/// The authority's account address.
	pub authority_id: AccountId,
	/// The unsigned PSBT (in bytes).
	pub unsigned_psbt: UnboundedBytes,
	/// The signed PSBT (in bytes).
	pub signed_psbt: UnboundedBytes,
}

#[derive(Decode, DecodeWithMemTracking, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
/// The message payload for PSBT finalization.
pub struct ExecutedPsbtMessage<AccountId> {
	/// The authority's account address.
	pub authority_id: AccountId,
	/// The executed PSBT's txid.
	pub txid: H256,
}

#[derive(Decode, DecodeWithMemTracking, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
/// The message payload for rollback PSBT submission.
pub struct RollbackPsbtMessage<AccountId> {
	/// The registered user's Bifrost address.
	pub who: AccountId,
	/// The hash of the transaction that contains the output (that should be rollbacked. to: `vault`)
	pub txid: H256,
	/// The output index of the transaction.
	pub vout: U256,
	/// The `amount` of the output.
	pub amount: U256,
	/// The unsigned PSBT (in bytes).
	pub unsigned_psbt: UnboundedBytes,
}

#[derive(Decode, DecodeWithMemTracking, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
/// The message payload for rollback poll submission.
pub struct RollbackPollMessage<AccountId> {
	/// The authority's account address.
	pub authority_id: AccountId,
	/// The rollback PSBT's txid.
	pub txid: H256,
	/// The voting side. Approved or not.
	pub is_approved: bool,
}

#[derive(Decode, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
/// The hash key used to call `BitcoinSocket::txs()`.
pub struct HashKeyRequest {
	/// The Bitcoin transaction hash.
	pub tx_hash: UnboundedBytes,
	/// The output index.
	pub index: U256,
	/// The user's Bifrost address.
	pub to: H160,
	/// The `amount` of the output.
	pub amount: U256,
}

impl HashKeyRequest {
	pub fn new(tx_hash: UnboundedBytes, index: U256, to: H160, amount: U256) -> Self {
		Self { tx_hash, index, to, amount }
	}

	pub fn encode(&self) -> UnboundedBytes {
		ethabi_decode::encode(&vec![
			Token::FixedBytes(self.tx_hash.clone()),
			Token::Uint(self.index),
			Token::Address(self.to),
			Token::Uint(self.amount),
		])
	}
}

#[derive(Decode, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
/// The return type of `BitcoinSocket::txs()`.
pub struct TxInfo {
	/// The user's Bifrost address.
	pub to: H160,
	/// The `amount` of the output.
	pub amount: U256,
	/// The current vote status.
	pub vote_count: U256,
	/// The request id.
	pub request_id: RequestID,
}

impl TryFrom<Vec<Token>> for TxInfo {
	type Error = ();

	fn try_from(token: Vec<Token>) -> Result<Self, Self::Error> {
		if token.len() != 4 {
			return Err(());
		}

		let to = token[0].clone().to_address().ok_or(())?;
		let amount = token[1].clone().to_uint().ok_or(())?;
		let vote_count = token[2].clone().to_uint().ok_or(())?;
		let request_id = match &token[3] {
			Token::Tuple(token) => token.clone().try_into()?,
			_ => return Err(()),
		};
		Ok(TxInfo { to, amount, vote_count, request_id })
	}
}
