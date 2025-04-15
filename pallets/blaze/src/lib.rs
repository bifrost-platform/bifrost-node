#![cfg_attr(not(feature = "std"), no_std)]

mod pallet;
pub mod weights;

pub use pallet::pallet::*;
pub use weights::WeightInfo;

use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_core::{ConstU32, RuntimeDebug, H256, U256};
use sp_runtime::{BoundedBTreeMap, BoundedVec};
use sp_std::vec::Vec;

use bp_btc_relay::UnboundedBytes;
use bp_staking::MAX_AUTHORITIES;

#[derive(Decode, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct Utxo<AccountId> {
	pub inner: UtxoInfo,
	pub is_approved: bool,
	pub voters: BoundedVec<AccountId, ConstU32<MAX_AUTHORITIES>>,
}

#[derive(Decode, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct UtxoInfo {
	pub txid: H256,
	pub vout: U256,
	pub amount: U256,
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
	pub utxos: Vec<UtxoInfo>,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct FeeRateSubmission<AccountId> {
	pub authority_id: AccountId,
	pub fee_rate: U256,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct OutboundRequestSubmission<AccountId> {
	pub authority_id: AccountId,
	pub messages: Vec<UnboundedBytes>,
}
