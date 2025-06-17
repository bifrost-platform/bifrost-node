#![cfg_attr(not(feature = "std"), no_std)]

pub mod migrations;
mod pallet;
pub mod weights;

pub use pallet::pallet::*;
pub use weights::WeightInfo;

use bp_btc_relay::{
	blaze::{UtxoInfo, UtxoInfoWithSize},
	UnboundedBytes,
};
use bp_staking::MAX_AUTHORITIES;
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_core::{ConstU32, RuntimeDebug, H256};
use sp_runtime::BoundedVec;
use sp_std::vec::Vec;

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

#[derive(Eq, PartialEq, Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
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

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
/// A submission of UTXOs.
pub struct UtxoSubmission<AccountId> {
	/// The authority id.
	pub authority_id: AccountId,
	/// The UTXOs to submit.
	pub utxos: Vec<UtxoInfo>,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
/// A submission of txid which is broadcasted.
pub struct BroadcastSubmission<AccountId> {
	/// The authority id.
	pub authority_id: AccountId,
	/// The txid of the PSBT.
	pub txid: H256,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
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

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
/// A submission of Socket messages.
pub struct SocketMessagesSubmission<AccountId> {
	/// The authority id.
	pub authority_id: AccountId,
	/// The Socket messages.
	pub messages: Vec<UnboundedBytes>,
}
