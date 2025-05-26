use crate::BoundedBitcoinAddress;
use parity_scale_codec::{Decode, Encode};
use scale_info::{prelude::string::String, TypeInfo};
use sp_core::{RuntimeDebug, H256};

#[derive(Decode, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
/// The information of a UTXO.
pub struct UtxoInfo {
	/// The txid of the UTXO.
	pub txid: H256,
	/// The vout (output index) of the UTXO.
	pub vout: u32,
	/// The amount of the UTXO.
	pub amount: u64,
	/// owner of the UTXO.
	pub address: BoundedBitcoinAddress,
}

#[derive(Decode, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
/// The information of a UTXO. (with size & script)
pub struct UtxoInfoWithSize {
	/// The UTXO hash.
	pub hash: H256,
	/// The txid of the UTXO.
	pub txid: H256,
	/// The vout (output index) of the UTXO.
	pub vout: u32,
	/// The amount of the UTXO.
	pub amount: u64,
	/// owner of the UTXO.
	pub descriptor: String,
	/// The size of the UTXO.
	pub input_vbytes: u64,
}

#[derive(Clone)]
pub struct ScoredUtxo {
	pub utxo: UtxoInfoWithSize,
	pub fee: u64,
	pub long_term_fee: u64,
	pub effective_value: u64,
}

#[derive(PartialEq, Eq)]
pub enum SelectionStrategy {
	Bnb,
	Knapsack,
}

#[derive(Eq, PartialEq, Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
pub enum FailureReason {
	InsufficientFunds,
	CoinSelection,
	PsbtComposition,
}
