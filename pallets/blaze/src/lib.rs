#![cfg_attr(not(feature = "std"), no_std)]

mod pallet;
pub mod weights;

pub use pallet::pallet::*;
pub use weights::WeightInfo;

use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_core::{ConstU32, RuntimeDebug, H256, U256};
use sp_runtime::BoundedVec;
use sp_std::vec::Vec;

use bp_btc_relay::UnboundedBytes;
use bp_staking::MAX_AUTHORITIES;

#[derive(Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
pub enum UtxoStatus {
	Unconfirmed,
	Available,
	Locked,
	Spent,
}

#[derive(Decode, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct Utxo<AccountId> {
	pub inner: UtxoInfo,
	pub status: UtxoStatus,
	pub voters: BoundedVec<AccountId, ConstU32<MAX_AUTHORITIES>>,
}

#[derive(Decode, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct UtxoInfo {
	pub txid: H256,
	pub vout: U256,
	pub amount: U256,
}

#[derive(Decode, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
/// A bundle of UTXOs with their voters.
pub struct Txos<AccountId> {
	/// Bundled and sorted UTXO hashes.
	pub utxo_hashes: Vec<H256>,
	/// Voters of the UTXOs.
	pub voters: BoundedVec<AccountId, ConstU32<MAX_AUTHORITIES>>,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct UtxoSubmission<AccountId> {
	pub authority_id: AccountId,
	pub utxos: Vec<UtxoInfo>,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct SpendTxosSubmission<AccountId> {
	pub authority_id: AccountId,
	/// The txid of the PSBT
	pub txid: H256,
	/// The utxo hashes to spend.
	pub utxo_hashes: Vec<H256>,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct FeeRateSubmission<AccountId, BlockNumber> {
	pub authority_id: AccountId,
	pub fee_rate: u64,
	/// The deadline of the submission. Used to filter out expired signatures.
	pub deadline: BlockNumber,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct OutboundRequestSubmission<AccountId> {
	pub authority_id: AccountId,
	pub messages: Vec<UnboundedBytes>,
}
