//! Type definitions for the EVM Fee Token pallet.

use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::{H160, U256};

/// Configuration for an accepted fee token.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen, Debug, PartialEq, Eq,
)]
pub struct FeeTokenConfig {
	/// Whether the token is currently enabled for fee payment.
	pub enabled: bool,

	/// Oracle contract address (Chainlink-style latestRoundData interface).
	pub oracle_address: H160,

	/// Token decimals (e.g., 6 for USDC, 18 for most tokens).
	pub decimals: u8,

	/// Oracle price decimals (e.g., 8 for Chainlink standard).
	/// Used in price conversion calculation.
	pub oracle_decimals: u8,
}

impl Default for FeeTokenConfig {
	fn default() -> Self {
		Self {
			enabled: true,
			oracle_address: H160::zero(),
			decimals: 18,
			oracle_decimals: 8, // Chainlink standard
		}
	}
}

/// Oracle price data from Chainlink-style oracle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OraclePriceData {
	/// Round ID from oracle.
	pub round_id: u128,
	/// Price answer (typically with 8 or 18 decimals).
	pub answer: i128,
	/// Timestamp when the round started.
	pub started_at: u64,
	/// Timestamp when the answer was computed.
	pub updated_at: u64,
	/// Round ID in which the answer was computed.
	pub answered_in_round: u128,
}

impl OraclePriceData {
	/// Get the price as U256 (absolute value).
	pub fn price_u256(&self) -> U256 {
		U256::from(self.answer.unsigned_abs())
	}
}
