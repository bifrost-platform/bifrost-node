//! Oracle integration for price feeds via oracle-registry.
//!
//! This module queries token prices through the `OracleRegistryManager` trait,
//! which delegates to the oracle-registry pallet. All oracle prices use 18 decimals.

use bp_oracle::{traits::OracleRegistryManager, AssetOracleId};
use frame_support::traits::Time;
use sp_core::{H160, H256, U256};
use sp_runtime::traits::UniqueSaturatedInto;

/// Error types for oracle operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OracleError {
	/// Oracle call failed (contract not configured or EVM execution error).
	CallFailed,
	/// Price is zero.
	InvalidPrice,
	/// Price data is stale (timestamp too old).
	StalePrice,
	/// No oracle ID registered for the given token.
	OracleNotRegistered,
}

/// Get price from oracle-registry by oracle ID with staleness check.
///
/// Calls `last_oracle_info(bytes32)` on the oracle manager contract via
/// `OracleRegistryManager` and validates the response.
///
/// # Arguments
/// * `oracle_id` - The oracle ID to query
/// * `max_staleness_seconds` - Maximum allowed age of price data in seconds (0 to disable)
///
/// # Returns
/// * `Ok(U256)` - Price as U256 (18 decimals)
/// * `Err(OracleError)` - If oracle call fails, returns invalid data, or price is stale
pub fn get_oracle_price_from_registry<T: crate::Config>(
	oracle_id: AssetOracleId,
	max_staleness_seconds: u64,
) -> Result<U256, OracleError> {
	let info =
		T::OracleRegistry::get_latest_oracle_info(oracle_id).ok_or(OracleError::CallFailed)?;

	// data → U256 (big-endian)
	let price = U256::from_big_endian(info.data.as_bytes());
	if price.is_zero() {
		log::warn!(
			target: "bifrost-tx-payment",
			"Oracle price invalid: zero price for oracle_id {:?}",
			oracle_id
		);
		return Err(OracleError::InvalidPrice);
	}

	// Staleness check using info.time
	if max_staleness_seconds > 0 {
		let now_ms: u128 = <T as pallet_evm::Config>::Timestamp::now().unique_saturated_into();
		let current_timestamp = (now_ms / 1000) as u64;

		if info.time == 0 {
			log::warn!(
				target: "bifrost-tx-payment",
				"Oracle price has zero timestamp for oracle_id {:?}",
				oracle_id
			);
			return Err(OracleError::StalePrice);
		}

		let age = current_timestamp.saturating_sub(info.time);
		if age > max_staleness_seconds {
			log::warn!(
				target: "bifrost-tx-payment",
				"Oracle price stale: time={}, current={}, age={}s, max_staleness={}s",
				info.time, current_timestamp, age, max_staleness_seconds
			);
			return Err(OracleError::StalePrice);
		}

		log::debug!(
			target: "bifrost-tx-payment",
			"Oracle price freshness OK: age={}s, max_staleness={}s",
			age, max_staleness_seconds
		);
	}

	Ok(price)
}

/// Get price from a Chainlink-compatible aggregator contract for an asset.
///
/// Calls `latestRoundData()` on the registered aggregator contract, normalises
/// the returned answer to 18 decimals using the aggregator's configured decimal
/// count, and validates staleness via the `updatedAt` timestamp.
///
/// # Arguments
/// * `token` - ERC20 token contract address whose aggregator to query
/// * `max_staleness_seconds` - Maximum allowed age of price data in seconds (0 to disable)
///
/// # Returns
/// * `Ok(U256)` - Price as U256 (18 decimals)
/// * `Err(OracleError)` - If no aggregator registered, call fails, price is zero/stale
pub fn get_aggregator_price_from_registry<T: crate::Config>(
	token: &H160,
	max_staleness_seconds: u64,
) -> Result<U256, OracleError> {
	let aggregator_info =
		T::OracleRegistry::get_aggregator_info(token).ok_or(OracleError::CallFailed)?;

	let round_data =
		T::OracleRegistry::get_latest_round_data(token).ok_or(OracleError::CallFailed)?;

	// `answer` is int256 in two's-complement. Bit 255 being set means negative.
	// Reject anything <= 0 (negative or zero).
	let answer = round_data.answer;
	if answer.is_zero() || answer.bit(255) {
		log::warn!(
			target: "bifrost-tx-payment",
			"Aggregator price invalid: answer is zero or negative for token {:?}",
			token
		);
		return Err(OracleError::InvalidPrice);
	}

	// Normalise to 18 decimals.
	let decimal = aggregator_info.decimal;
	let price = if decimal <= 18 {
		answer.saturating_mul(U256::from(10u64).pow(U256::from(18u8 - decimal)))
	} else {
		answer / U256::from(10u64).pow(U256::from(decimal - 18))
	};

	if price.is_zero() {
		return Err(OracleError::InvalidPrice);
	}

	// Staleness check using `updatedAt`.
	if max_staleness_seconds > 0 {
		let updated_at = round_data.updated_at.low_u64();

		if updated_at == 0 {
			log::warn!(
				target: "bifrost-tx-payment",
				"Aggregator price has zero updatedAt for token {:?}",
				token
			);
			return Err(OracleError::StalePrice);
		}

		let now_ms: u128 = <T as pallet_evm::Config>::Timestamp::now().unique_saturated_into();
		let current_timestamp = (now_ms / 1000) as u64;
		let age = current_timestamp.saturating_sub(updated_at);

		if age > max_staleness_seconds {
			log::warn!(
				target: "bifrost-tx-payment",
				"Aggregator price stale: updatedAt={}, current={}, age={}s, max_staleness={}s",
				updated_at, current_timestamp, age, max_staleness_seconds
			);
			return Err(OracleError::StalePrice);
		}

		log::debug!(
			target: "bifrost-tx-payment",
			"Aggregator price freshness OK: age={}s, max_staleness={}s",
			age, max_staleness_seconds
		);
	}

	Ok(price)
}

/// Get price for a token via oracle-registry.
///
/// Looks up the oracle ID for the given token address in the oracle-registry.
/// If the oracle ID is zero the price is sourced from the Chainlink-compatible
/// aggregator contract registered for that asset; otherwise the oracle manager
/// contract is queried directly.
///
/// # Arguments
/// * `token` - ERC20 token contract address
/// * `max_staleness_seconds` - Maximum allowed age of price data in seconds (0 to disable)
///
/// # Returns
/// * `Ok(U256)` - Price as U256 (18 decimals)
/// * `Err(OracleError)` - If no oracle registered for token, or oracle query fails
pub fn get_token_price_via_registry<T: crate::Config>(
	token: H160,
	max_staleness_seconds: u64,
) -> Result<U256, OracleError> {
	let oracle_id =
		T::OracleRegistry::get_asset_oracle(&token).ok_or(OracleError::OracleNotRegistered)?;

	if oracle_id == H256::zero() {
		// Zero oracle ID signals that pricing comes from the aggregator contract.
		get_aggregator_price_from_registry::<T>(&token, max_staleness_seconds)
	} else {
		get_oracle_price_from_registry::<T>(oracle_id, max_staleness_seconds)
	}
}
