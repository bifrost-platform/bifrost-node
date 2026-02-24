#![cfg_attr(not(feature = "std"), no_std)]

pub mod traits;

use sp_core::{H160, H256};

/// Chain ID type.
pub type ChainId = u32;

/// Asset address type (EVM-compatible contract address).
pub type AssetId = H160;

/// Asset oracle ID type (oracle identifier used to fetch the asset price).
pub type AssetOracleId = H256;
