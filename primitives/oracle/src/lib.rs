#![cfg_attr(not(feature = "std"), no_std)]

pub mod traits;

use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::{H160, H256, U256};

/// Chain ID type.
pub type ChainId = u64;

/// Asset address type (EVM-compatible contract address).
pub type AssetId = H160;

/// Asset oracle ID type (oracle identifier used to fetch the asset price).
pub type AssetOracleId = H256;

/// Oracle info returned by `last_oracle_info(bytes32)`.
///
/// Maps to the Solidity struct `Oracle_Manager_Source`:
/// ```solidity
/// struct Oracle_Manager_Source {
///     bytes32 data;
///     uint64  block;
///     uint64  time;
///     uint64  authority_round;
///     uint64  _reserved;
/// }
/// ```
#[derive(Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo)]
#[cfg_attr(feature = "std", derive(Debug))]
pub struct OracleInfo {
	/// The uint256 round returned as the first output.
	pub round: sp_core::U256,
	/// The raw oracle data (`bytes32`).
	pub data: H256,
	/// The block number at which the data was recorded.
	pub block: u64,
	/// The timestamp at which the data was recorded.
	pub time: u64,
	/// The authority round in which the data was recorded.
	pub authority_round: u64,
	/// Reserved field.
	pub reserved: u64,
}

/// Unified key for oracle registry lookups.
///
/// Encodes both EVM asset contract addresses and chain IDs into a single
/// type, allowing them to share a single [`StorageMap`].
#[derive(Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo)]
#[cfg_attr(feature = "std", derive(Debug))]
pub enum OracleKey {
	/// Oracle key for an EVM-compatible asset contract address.
	Asset(AssetId),
	/// Oracle key for the native currency of a chain.
	NativeCurrency(ChainId),
}

#[derive(Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo)]
#[cfg_attr(feature = "std", derive(Debug))]
pub struct AggregatorInfo {
	/// The aggregator contract address.
	pub address: H160,
	/// The decimal places of the asset.
	pub decimal: u8,
}

/// Round data returned by `latestRoundData()` on a Chainlink-compatible aggregator contract.
///
/// Maps to the Solidity function return:
/// ```solidity
/// function latestRoundData() external view
///   returns (
///     uint80 roundId,
///     int256 answer,
///     uint256 startedAt,
///     uint256 updatedAt,
///     uint80 answeredInRound
///   );
/// ```
///
/// All fields are stored as `U256` (raw big-endian 256-bit words as decoded from ABI).
/// `answer` carries the raw two's-complement bits of the `int256` return value.
#[derive(Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo)]
#[cfg_attr(feature = "std", derive(Debug))]
pub struct AggregatorRoundData {
	/// Round ID (`uint80`, padded to 32 bytes).
	pub round_id: U256,
	/// Price answer (`int256`, stored as raw two's-complement big-endian bits).
	pub answer: U256,
	/// Timestamp when the round started (`uint256`).
	pub started_at: U256,
	/// Timestamp when the round was last updated (`uint256`).
	pub updated_at: U256,
	/// Round ID in which the answer was computed (`uint80`, padded to 32 bytes).
	pub answered_in_round: U256,
}
