#![cfg_attr(not(feature = "std"), no_std)]

pub mod traits;

use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::{H160, H256};

/// Chain ID type.
pub type ChainId = u32;

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
