use crate::{AssetId, AssetOracleId, ChainId, OracleInfo, OracleKey};
use sp_core::{H160, H256, U256};

/// ABI encoding/decoding helpers for the oracle manager contract.
///
/// These utilities encode calldata for and decode return data from the
/// `last_oracle_data(bytes32)` function on the oracle manager contract.
/// They are provided here for use by pallet implementations of
/// [`OracleRegistryManager::get_latest_oracle_data`].
pub mod oracle_manager_abi {
	use super::*;

	/// `last_oracle_data(bytes32)` function selector.
	///
	/// Computed as `keccak256("last_oracle_data(bytes32)")[0..4]`.
	pub const LAST_ORACLE_DATA_SELECTOR: [u8; 4] = [0xbe, 0x27, 0x46, 0x72];

	/// Encodes the calldata for `last_oracle_data(bytes32 oid)`.
	///
	/// Returns a 36-byte buffer: 4-byte selector followed by the ABI-encoded
	/// `oracle_id` (left-padded to 32 bytes as required by the ABI spec).
	pub fn encode_calldata(oracle_id: AssetOracleId) -> [u8; 36] {
		let mut calldata = [0u8; 36];
		calldata[..4].copy_from_slice(&LAST_ORACLE_DATA_SELECTOR);
		calldata[4..].copy_from_slice(oracle_id.as_bytes());
		calldata
	}

	/// Decodes the return data from `last_oracle_data(bytes32)`.
	///
	/// Expects at least 32 bytes and returns the first 32 bytes as `H256`,
	/// or `None` if the slice is too short.
	pub fn decode_return(data: &[u8]) -> Option<H256> {
		if data.len() < 32 {
			return None;
		}
		Some(H256::from_slice(&data[..32]))
	}
}

/// ABI encoding/decoding helpers for the `last_oracle_info(bytes32)` function.
///
/// The function returns `(uint256, (bytes32, uint64, uint64, uint64, uint64))`,
/// which is ABI-encoded as 6 consecutive 32-byte words (192 bytes total).
pub mod oracle_info_abi {
	use super::*;

	/// `last_oracle_info(bytes32)` function selector.
	///
	/// Computed as `keccak256("last_oracle_info(bytes32)")[0..4]`.
	pub const LAST_ORACLE_INFO_SELECTOR: [u8; 4] = [0xa1, 0x6d, 0x4d, 0x76];

	/// Encodes the calldata for `last_oracle_info(bytes32 oid)`.
	///
	/// Returns a 36-byte buffer: 4-byte selector followed by the ABI-encoded
	/// `oracle_id` (left-padded to 32 bytes as required by the ABI spec).
	pub fn encode_calldata(oracle_id: AssetOracleId) -> [u8; 36] {
		let mut calldata = [0u8; 36];
		calldata[..4].copy_from_slice(&LAST_ORACLE_INFO_SELECTOR);
		calldata[4..].copy_from_slice(oracle_id.as_bytes());
		calldata
	}

	/// Decodes the return data from `last_oracle_info(bytes32)`.
	///
	/// Expects 192 bytes (6 × 32-byte ABI words):
	/// - Word 0: `uint256` value
	/// - Word 1: `bytes32` data
	/// - Word 2: `uint64`  block   (left-padded to 32 bytes)
	/// - Word 3: `uint64`  time    (left-padded to 32 bytes)
	/// - Word 4: `uint64`  authority_round (left-padded to 32 bytes)
	/// - Word 5: `uint64`  _reserved (left-padded to 32 bytes)
	pub fn decode_return(data: &[u8]) -> Option<OracleInfo> {
		if data.len() < 192 {
			return None;
		}

		let round = U256::from_big_endian(&data[0..32]);
		let oracle_data = H256::from_slice(&data[32..64]);
		let block = U256::from_big_endian(&data[64..96]).low_u64();
		let time = U256::from_big_endian(&data[96..128]).low_u64();
		let authority_round = U256::from_big_endian(&data[128..160]).low_u64();
		let reserved = U256::from_big_endian(&data[160..192]).low_u64();

		Some(OracleInfo { round, data: oracle_data, block, time, authority_round, reserved })
	}
}

/// Cross-pallet interface for the Oracle Registry.
///
/// Implement this trait on the oracle registry pallet and use it as a bound in
/// other pallets' `Config` to give them typed, read-only access to the oracle
/// registry without creating a hard dependency on the pallet itself.
pub trait OracleRegistryManager {
	/// Returns the oracle ID registered for the given key, or `None` if not
	/// registered.
	fn get_oracle(key: OracleKey) -> Option<AssetOracleId>;

	/// Returns the oracle ID registered for the given EVM asset contract
	/// address, or `None` if not registered.
	fn get_asset_oracle(asset: &AssetId) -> Option<AssetOracleId> {
		Self::get_oracle(OracleKey::Asset(*asset))
	}

	/// Returns the oracle ID registered for the native currency of the given
	/// chain, or `None` if not registered.
	fn get_native_currency_oracle(chain_id: ChainId) -> Option<AssetOracleId> {
		Self::get_oracle(OracleKey::NativeCurrency(chain_id))
	}

	/// Returns the oracle manager contract address, or `None` if not set.
	///
	/// Other pallets (e.g., precompiles) can use this to verify whether a
	/// calling EVM contract is authorised to manage the oracle registry.
	fn get_oracle_manager_contract() -> Option<H160>;

	/// Calls `last_oracle_data(bytes32)` on the oracle manager contract and
	/// returns the result, or `None` if the contract is not configured or the
	/// call fails.
	///
	/// Implementations should:
	/// 1. Retrieve the contract address via [`Self::get_oracle_manager_contract`].
	/// 2. Encode the calldata using [`oracle_manager_abi::encode_calldata`].
	/// 3. Execute a read-only EVM call (e.g. `Runner::view_call`) with the
	///    encoded input.
	/// 4. Decode the return bytes via [`oracle_manager_abi::decode_return`].
	///
	/// # Arguments
	/// * `oracle_id` - The oracle ID (`bytes32`) to query on the contract.
	///
	/// # Returns
	/// * `Some(H256)` - The raw 32-byte oracle data returned by the contract.
	/// * `None` - If the contract is not set, the EVM call fails, or the
	///   return data cannot be decoded.
	fn get_latest_oracle_data(oracle_id: AssetOracleId) -> Option<H256>;

	/// Calls `last_oracle_info(bytes32)` on the oracle manager contract and
	/// returns the full oracle info, or `None` if the contract is not configured
	/// or the call fails.
	///
	/// # Arguments
	/// * `oracle_id` - The oracle ID (`bytes32`) to query on the contract.
	///
	/// # Returns
	/// * `Some(OracleInfo)` - The decoded oracle info containing value, data,
	///   block, time, authority_round, and reserved fields.
	/// * `None` - If the contract is not set, the EVM call fails, or the
	///   return data cannot be decoded.
	fn get_latest_oracle_info(oracle_id: AssetOracleId) -> Option<OracleInfo>;
}
