#![cfg_attr(not(feature = "std"), no_std)]

mod pallet;
pub mod weights;

use bp_btc_relay::UnboundedBytes;
pub use pallet::pallet::*;
use sp_runtime::BoundedBTreeMap;
pub use weights::WeightInfo;

use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_core::{ConstU32, RuntimeDebug, H256, U256};
use sp_std::vec::Vec;

use bp_staking::MAX_AUTHORITIES;

/// The round of the registration pool.
pub type PoolRound = u32;

#[derive(Decode, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct Utxo<AccountId> {
	pub txid: H256,
	pub vout: U256,
	pub amount: U256,
	pub is_approved: bool,
	// TODO: need lock time
	pub votes: BoundedBTreeMap<AccountId, bool, ConstU32<MAX_AUTHORITIES>>,
}

#[derive(Decode, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct PendingFeeRate<AccountId> {
	pub is_approved: bool,
	pub votes: BoundedBTreeMap<AccountId, U256, ConstU32<MAX_AUTHORITIES>>,
}

impl<AccountId: Ord> Default for PendingFeeRate<AccountId> {
	fn default() -> Self {
		Self { is_approved: false, votes: Default::default() }
	}
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct UtxoSubmission<AccountId> {
	pub authority_id: AccountId,
	pub utxos: Vec<Utxo<AccountId>>,
	/// The pool round.
	pub pool_round: PoolRound,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct FeeRateSubmission<AccountId> {
	pub authority_id: AccountId,
	pub fee_rate: U256,
	/// The pool round.
	pub pool_round: PoolRound,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct OutboundRequestSubmission<AccountId> {
	pub authority_id: AccountId,
	pub messages: Vec<UnboundedBytes>,
	/// The pool round.
	pub pool_round: PoolRound,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct SpendTxosSubmission<AccountId> {
	pub authority_id: AccountId,
	/// key: txid
	/// value: vout
	pub locked_txos: Vec<(H256, U256)>,
	/// The pool round.
	pub pool_round: PoolRound,
}
