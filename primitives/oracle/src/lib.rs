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
