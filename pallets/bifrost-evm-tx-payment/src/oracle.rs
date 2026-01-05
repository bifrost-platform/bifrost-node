//! Oracle integration for price feeds.
//!
//! This module implements Chainlink-style oracle integration using the
//! `latestRoundData()` interface.

use crate::types::OraclePriceData;
use frame_support::traits::Time;
use pallet_evm::{ExitReason, Runner};
use sp_core::{H160, U256};
use sp_runtime::traits::UniqueSaturatedInto;

/// Chainlink AggregatorV3 function selectors.
mod selectors {
	/// `latestRoundData()` selector: 0xfeaf968c
	pub const LATEST_ROUND_DATA: [u8; 4] = [0xfe, 0xaf, 0x96, 0x8c];
}

/// Error types for oracle operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OracleError {
	/// Oracle call failed (EVM execution error).
	CallFailed,
	/// Invalid return data from oracle.
	InvalidData,
	/// Price is zero or negative.
	InvalidPrice,
	/// Price data is stale (updated_at too old).
	StalePrice,
}

/// Get price from Chainlink-style oracle with staleness check.
///
/// Calls `latestRoundData()` on the oracle contract and validates the response,
/// including checking that the price data is not stale.
///
/// # Arguments
/// * `oracle_address` - Address of the oracle contract
/// * `max_staleness_seconds` - Maximum allowed age of price data in seconds (0 to disable)
///
/// # Returns
/// * `Ok(U256)` - Price as U256 (with oracle decimals)
/// * `Err(OracleError)` - If oracle call fails, returns invalid data, or price is stale
pub fn get_oracle_price_with_staleness<T: crate::Config>(
	oracle_address: H160,
	max_staleness_seconds: u64,
) -> Result<U256, OracleError> {
	let price_data = call_latest_round_data::<T>(oracle_address)?;

	// Validate price data - must be positive
	if price_data.answer <= 0 {
		log::warn!(
			target: "bifrost-tx-payment",
			"Oracle price invalid: answer={} (must be positive)",
			price_data.answer
		);
		return Err(OracleError::InvalidPrice);
	}

	// Check staleness if enabled (max_staleness_seconds > 0)
	if max_staleness_seconds > 0 {
		// Get current timestamp from pallet-evm's Timestamp (in milliseconds)
		// Convert to seconds for comparison with oracle's updated_at
		let now_ms: u128 =
			<T as pallet_evm::Config>::Timestamp::now().unique_saturated_into();
		let current_timestamp = (now_ms / 1000) as u64;

		if price_data.updated_at == 0 {
			log::warn!(
				target: "bifrost-tx-payment",
				"Oracle price has zero updated_at timestamp"
			);
			return Err(OracleError::StalePrice);
		}

		let age = current_timestamp.saturating_sub(price_data.updated_at);
		if age > max_staleness_seconds {
			log::warn!(
				target: "bifrost-tx-payment",
				"Oracle price stale: updated_at={}, current={}, age={}s, max_staleness={}s",
				price_data.updated_at, current_timestamp, age, max_staleness_seconds
			);
			return Err(OracleError::StalePrice);
		}

		log::debug!(
			target: "bifrost-tx-payment",
			"Oracle price freshness OK: age={}s, max_staleness={}s",
			age, max_staleness_seconds
		);
	}

	Ok(price_data.price_u256())
}

/// Get price from Chainlink-style oracle (legacy, no staleness check).
///
/// Calls `latestRoundData()` on the oracle contract and parses the response.
///
/// # Arguments
/// * `oracle_address` - Address of the oracle contract
///
/// # Returns
/// * `Ok(U256)` - Price as U256 (with oracle decimals)
/// * `Err(())` - If oracle call fails or returns invalid data
pub fn get_oracle_price<T: crate::Config>(oracle_address: H160) -> Result<U256, ()> {
	get_oracle_price_with_staleness::<T>(oracle_address, 0).map_err(|_| ())
}

/// Call `latestRoundData()` on oracle contract.
///
/// Returns parsed OraclePriceData struct.
fn call_latest_round_data<T: crate::Config>(
	oracle_address: H160,
) -> Result<OraclePriceData, OracleError> {
	// Prepare calldata: just the function selector
	let calldata = selectors::LATEST_ROUND_DATA.to_vec();

	log::debug!(
		target: "bifrost-tx-payment",
		"Oracle call: calling latestRoundData() on {:?}",
		oracle_address
	);

	// Execute call via pallet-evm Runner using view_call to avoid state changes.
	// view_call wraps execution in a storage transaction that gets rolled back,
	// ensuring no state changes persist (including nonce increments).
	let result = T::Runner::view_call(
		H160::zero(),   // source (context only, no state changes)
		oracle_address, // target (oracle contract)
		calldata,       // input (function selector)
		100_000u64,     // gas_limit (enough for view call)
		T::config(),
	);

	let result = match result {
		Err(_) => {
			log::warn!(
				target: "bifrost-tx-payment",
				"Oracle call failed: Runner::call returned error"
			);
			return Err(OracleError::CallFailed);
		},
		Ok(r) => {
			log::debug!(
				target: "bifrost-tx-payment",
				"Oracle call result: exit_reason={:?}, return_data_len={}",
				r.exit_reason, r.value.len()
			);
			r
		},
	};

	// Check for successful execution
	match result.exit_reason {
		ExitReason::Succeed(_) => {},
		ref reason => {
			log::warn!(
				target: "bifrost-tx-payment",
				"Oracle call reverted: {:?}",
				reason
			);
			return Err(OracleError::CallFailed);
		},
	}

	// Parse return data
	// latestRoundData returns: (uint80, int256, uint256, uint256, uint80)
	// Total: 5 * 32 = 160 bytes
	let return_data = result.value;
	if return_data.len() < 160 {
		log::warn!(
			target: "bifrost-tx-payment",
			"Oracle call failed: return data too short, expected 160 bytes, got {}",
			return_data.len()
		);
		return Err(OracleError::InvalidData);
	}

	let price_data =
		parse_latest_round_data(&return_data).map_err(|_| OracleError::InvalidData)?;

	log::debug!(
		target: "bifrost-tx-payment",
		"Oracle price data: answer={}, updated_at={}",
		price_data.answer, price_data.updated_at
	);

	Ok(price_data)
}

/// Parse the return data from `latestRoundData()`.
///
/// Expected format (ABI encoded):
/// - uint80 roundId (32 bytes, right-aligned)
/// - int256 answer (32 bytes)
/// - uint256 startedAt (32 bytes)
/// - uint256 updatedAt (32 bytes)
/// - uint80 answeredInRound (32 bytes, right-aligned)
fn parse_latest_round_data(data: &[u8]) -> Result<OraclePriceData, ()> {
	if data.len() < 160 {
		return Err(());
	}

	// Parse roundId (uint80, bytes 0-32)
	let round_id = parse_uint80(&data[0..32])?;

	// Parse answer (int256, bytes 32-64)
	let answer = parse_int256(&data[32..64])?;

	// Parse startedAt (uint256, bytes 64-96)
	let started_at = parse_uint256_as_u64(&data[64..96])?;

	// Parse updatedAt (uint256, bytes 96-128)
	let updated_at = parse_uint256_as_u64(&data[96..128])?;

	// Parse answeredInRound (uint80, bytes 128-160)
	let answered_in_round = parse_uint80(&data[128..160])?;

	Ok(OraclePriceData { round_id, answer, started_at, updated_at, answered_in_round })
}

/// Parse uint80 from 32-byte ABI encoded data.
fn parse_uint80(data: &[u8]) -> Result<u128, ()> {
	if data.len() != 32 {
		return Err(());
	}

	// uint80 is right-aligned in 32 bytes
	// Take the last 10 bytes (80 bits)
	let mut bytes = [0u8; 16];
	bytes[6..16].copy_from_slice(&data[22..32]);

	Ok(u128::from_be_bytes(bytes))
}

/// Parse int256 from 32-byte ABI encoded data.
fn parse_int256(data: &[u8]) -> Result<i128, ()> {
	if data.len() != 32 {
		return Err(());
	}

	// For simplicity, we'll parse as i128 (sufficient for price data)
	// Check if negative (MSB set)
	let is_negative = data[0] & 0x80 != 0;

	if is_negative {
		// Handle negative numbers
		// For now, we'll return error as prices should be positive
		return Err(());
	}

	// Take lower 16 bytes for positive i128
	let mut bytes = [0u8; 16];
	bytes.copy_from_slice(&data[16..32]);

	Ok(i128::from_be_bytes(bytes))
}

/// Parse uint256 as u64 (for timestamps).
fn parse_uint256_as_u64(data: &[u8]) -> Result<u64, ()> {
	if data.len() != 32 {
		return Err(());
	}

	// Take last 8 bytes for u64
	let mut bytes = [0u8; 8];
	bytes.copy_from_slice(&data[24..32]);

	Ok(u64::from_be_bytes(bytes))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_parse_uint80() {
		// Test with value 1
		let mut data = [0u8; 32];
		data[31] = 1;
		assert_eq!(parse_uint80(&data).unwrap(), 1);

		// Test with max uint80
		let mut data = [0u8; 32];
		data[22..32].copy_from_slice(&[0xff; 10]);
		let max_uint80 = (1u128 << 80) - 1;
		assert_eq!(parse_uint80(&data).unwrap(), max_uint80);
	}

	#[test]
	fn test_parse_int256_positive() {
		// Test with value 100000000 (1 USD with 8 decimals)
		let mut data = [0u8; 32];
		data[28..32].copy_from_slice(&100000000u32.to_be_bytes());
		assert_eq!(parse_int256(&data).unwrap(), 100000000);
	}

	#[test]
	fn test_parse_uint256_as_u64() {
		// Test with timestamp
		let timestamp: u64 = 1700000000;
		let mut data = [0u8; 32];
		data[24..32].copy_from_slice(&timestamp.to_be_bytes());
		assert_eq!(parse_uint256_as_u64(&data).unwrap(), timestamp);
	}
}
