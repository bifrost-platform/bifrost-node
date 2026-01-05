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

	/// Maximum allowed staleness for oracle price data in seconds.
	/// If the oracle's `updated_at` timestamp is older than `now - max_staleness_seconds`,
	/// the price is considered stale and will be rejected.
	/// Set to 0 to disable staleness check (not recommended for production).
	pub max_staleness_seconds: u64,
}

impl Default for FeeTokenConfig {
	fn default() -> Self {
		Self {
			enabled: true,
			oracle_address: H160::zero(),
			decimals: 18,
			oracle_decimals: 8,          // Chainlink standard
			max_staleness_seconds: 3600, // 1 hour default
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

/// Reason why ERC20 fee payment fell back to native token.
#[derive(
	Clone, Copy, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen, Debug, PartialEq, Eq,
)]
pub enum FallbackReason {
	/// Token is not registered in accepted fee tokens.
	TokenNotRegistered,
	/// Token is disabled.
	TokenDisabled,
	/// Oracle price conversion failed.
	PriceConversionFailed,
	/// ERC20 transfer failed (e.g., insufficient balance).
	TransferFailed,
	/// Oracle price data is stale (updated_at too old).
	OraclePriceStale,
}

/// Fee payment information for a transaction.
///
/// This is stored per-transaction to allow Ethereum RPC to expose
/// ERC20 fee payment details in transaction receipts.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen, Debug, PartialEq, Eq,
)]
pub struct FeePaymentInfo {
	/// The ERC20 token used for fee payment.
	pub token: H160,
	/// Amount of ERC20 tokens paid as fee.
	pub amount: U256,
	/// Equivalent amount in native token (BFC).
	pub native_equivalent: U256,
	/// Oracle price of the ERC20 token (Token/USD) at the time of payment.
	/// The number of decimals is defined in the token's oracle configuration.
	pub token_price: U256,
	/// Oracle price of the native token (Native/USD) at the time of payment.
	/// The number of decimals is defined in the native oracle configuration.
	pub native_price: U256,
}
