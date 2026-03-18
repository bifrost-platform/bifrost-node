//! Type definitions for the EVM Fee Token pallet.

use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::{H160, U256};

/// Configuration for an accepted fee token (V0 - Chainlink oracle).
///
/// This version includes oracle_address and oracle_decimals for direct
/// Chainlink-style oracle calls. Used only for storage migration to V1.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen, Debug, PartialEq, Eq,
)]
pub struct FeeTokenConfigV0 {
	pub enabled: bool,
	pub oracle_address: H160,
	pub decimals: u8,
	pub oracle_decimals: u8,
	pub max_staleness_seconds: u64,
	pub min_balance: U256,
}

/// Configuration for an accepted fee token (V1 - oracle-registry).
///
/// Oracle address and oracle decimals are no longer stored per-token.
/// The oracle-registry manages token → oracle ID mappings, and all
/// oracle prices use a fixed 18-decimal format.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen, Debug, PartialEq, Eq,
)]
pub struct FeeTokenConfig {
	/// Whether the token is currently enabled for fee payment.
	pub enabled: bool,

	/// Token decimals (e.g., 6 for USDC, 18 for most tokens).
	pub decimals: u8,

	/// Maximum allowed staleness for oracle price data in seconds.
	/// If the oracle's timestamp is older than `now - max_staleness_seconds`,
	/// the price is considered stale and will be rejected.
	/// Set to 0 to disable staleness check (not recommended for production).
	pub max_staleness_seconds: u64,

	/// Minimum token balance required for feeless `setUserFeeToken` calls.
	/// Users must hold at least this amount of the token to set it as their
	/// fee token without paying gas fees. This prevents spam attacks where
	/// attackers create many addresses and set fee tokens without cost.
	/// Set to 0 to disable the minimum balance check.
	pub min_balance: U256,
}

impl Default for FeeTokenConfig {
	fn default() -> Self {
		Self {
			enabled: true,
			decimals: 18,
			max_staleness_seconds: 3600, // 1 hour default
			min_balance: U256::zero(),   // No minimum by default
		}
	}
}

impl From<FeeTokenConfigV0> for FeeTokenConfig {
	fn from(v0: FeeTokenConfigV0) -> Self {
		Self {
			enabled: v0.enabled,
			decimals: v0.decimals,
			max_staleness_seconds: v0.max_staleness_seconds,
			min_balance: v0.min_balance,
		}
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
	/// All oracle prices use 18 decimals.
	pub token_price: U256,
	/// Oracle price of the native token (Native/USD) at the time of payment.
	/// All oracle prices use 18 decimals.
	pub native_price: U256,
}
