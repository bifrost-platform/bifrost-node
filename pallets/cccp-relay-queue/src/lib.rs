#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]

mod pallet;
pub mod weights;

pub mod migrations;
pub use pallet::pallet::*;
pub use weights::WeightInfo;

use bp_cccp::UnboundedBytes;
use bp_staking::MAX_AUTHORITIES;
use frame_support::traits::Currency;
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode};
use scale_info::TypeInfo;
use sp_core::{ConstU32, RuntimeDebug, H160, H256, U256};
use sp_runtime::BoundedVec;

pub(crate) const LOG_TARGET: &'static str = "runtime::cccp-relay-queue";

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

/// Chain ID type.
pub type ChainId = u32;

/// Asset address type.
pub type AssetId = H160;

/// Asset oracle address type.
pub type AssetOracleId = H160;

/// Asset index hash type.
pub type AssetIndexHash = H256;

#[derive(
	Decode, Encode, TypeInfo, Clone, Copy, PartialEq, Eq, RuntimeDebug, DecodeWithMemTracking,
)]
pub enum TransferOption {
	Fast,
	Standard,
}

#[derive(
	Decode, Encode, TypeInfo, Clone, Copy, PartialEq, Eq, RuntimeDebug, DecodeWithMemTracking,
)]
pub enum TransferStatus {
	Pending,
	OnFlight,
	Finalized,
}

pub type BalanceOf<T> =
	<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

/// Maximum allowed on-flight cap per asset (100 million in base units).
/// This limit prevents excessive Fast transfer exposure and potential overflow issues.
/// Value: 100,000,000 * 10^18 (assuming 18 decimals like BFC)
pub const MAX_ON_FLIGHT_CAP: u128 = 100_000_000 * bifrost_common_constants::currency::BFC; // 100M with 18 decimals

/// Maximum number of asset indexes per call.
/// This limit prevents excessive asset index operations and DoS attacks.
pub const MAX_ASSET_INDEXES_PER_CALL: usize = 100;

/// Maximum native currency chains per asset (50).
/// Rationale: Realistically, no asset will be native on >50 chains.
/// This prevents storage bloat while allowing future chain growth.
pub const MAX_NATIVE_CURRENCY_CHAINS: usize = 50;

#[derive(Decode, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct AssetCapInfo<Balance> {
	/// The maximum on-flight cap of the asset.
	pub max_on_flight_cap: Balance,
	/// The current on-flight cap of the asset.
	pub on_flight_cap: Balance,
}

#[derive(Decode, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct TransferInfo<Balance, AccountId> {
	/// The amount of the transfer.
	pub amount: Balance,
	/// The sequence id. The sequence id which initiated the transfer.
	pub sequence_id: U256,
	/// The source transaction id.
	pub src_tx_id: H256,
	/// The source chain id.
	pub src_chain_id: ChainId,
	/// The destination chain id.
	pub dst_chain_id: ChainId,
	/// The asset index hash.
	pub asset_index_hash: AssetIndexHash,
	/// The option of the transfer.
	pub option: TransferOption,
	/// The status of the transfer.
	pub status: TransferStatus,
	/// The initial socket message of the transfer. (status: REQUESTED)
	pub socket_message: UnboundedBytes,
	/// Voters of the transfer.
	/// Voting is only required for inbound requests since the source chain are non-bifrost chains.
	/// Socket messages originated by outbound requests are internally validated by the pallet itself. (=immediately on-flight)
	pub on_flight_voters: BoundedVec<AccountId, ConstU32<MAX_AUTHORITIES>>,
	/// Voters of the finalization.
	/// Voting is only required for inbound requests since the source chain are non-bifrost chains.
	/// Socket messages originated by outbound requests are internally validated by the pallet itself. (=immediately finalized)
	pub finalization_voters: BoundedVec<AccountId, ConstU32<MAX_AUTHORITIES>>,
}

#[derive(Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
/// A submission of Socket message.
pub struct OnFlightPollSubmission<AccountId> {
	/// The authority id.
	pub authority_id: AccountId,
	/// The source transaction id.
	pub src_tx_id: H256,
	/// The source chain id.
	pub src_chain_id: ChainId,
	/// The sequence id.
	pub sequence_id: U256,
	/// The Socket message.
	pub message: UnboundedBytes,
}

#[derive(Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
/// A submission of Socket message.
pub struct FinalizePollSubmission<AccountId> {
	/// The authority id.
	pub authority_id: AccountId,
	/// The source chain id.
	pub src_chain_id: ChainId,
	/// The sequence id.
	pub sequence_id: U256,
	/// The Socket message.
	pub message: UnboundedBytes,
}
