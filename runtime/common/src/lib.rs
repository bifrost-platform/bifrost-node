#![cfg_attr(not(feature = "std"), no_std)]

mod apis;
mod precompiles;
mod self_contained_call;

pub mod extensions;

use frame_support::traits::Get;
use pallet_bifrost_evm_tx_payment::{AcceptedFeeTokens, LastFeeTokenUpdate, Pallet, UserFeeToken};
use sp_core::{H160, U256};
use sp_runtime::traits::{Saturating, Zero};
use sp_std::marker::PhantomData;

/// Filter for feeless EVM calls.
///
/// **IMPORTANT**: This filter has two different effects depending on where it's used:
///
/// 1. **Pool validation** (`validate_transaction_in_pool`):
///    - `is_zero_balance_callable`: Skips native balance check for gas fees
///    - Used for: ERC20 fee users (they pay in ERC20, not native) and fee setup calls
///    - For ERC20 fee users, validates they have sufficient token balance to cover fees
///
/// 2. **Runner execution** (`call`/`create`):
///    - `is_feeless`: Sets `effective_max_fee_per_gas = 0` (truly free call)
///    - Used for: Only fee setup calls (`setUserFeeToken`, `clearUserFeeToken`)
///
/// # Security
/// - Rate limiting prevents spam on fee token setup calls
/// - ERC20 fee payment validation happens both here (pool) and in `BifrostFeeAdapter::withdraw_fee()`
/// - DoS is prevented by checking token balance at pool validation time
pub struct BifrostFeelessCalls<T>(PhantomData<T>);

impl<T> pallet_evm::FeelessCallFilter for BifrostFeelessCalls<T>
where
	T: frame_system::Config + pallet_bifrost_evm_tx_payment::Config,
{
	/// Returns `true` if the call can be submitted with zero native balance.
	///
	/// This includes:
	/// - Users with ERC20 fee token set who have sufficient token balance
	/// - Fee token setup calls (truly feeless, rate-limited)
	fn is_zero_balance_callable(
		caller: H160,
		target: Option<H160>,
		input: &[u8],
		gas_limit: U256,
		base_fee: U256,
	) -> bool {
		// Case 1: Check if user has ERC20 fee token set
		if let Some(token) = UserFeeToken::<T>::get(caller) {
			return Self::validate_erc20_fee_user(caller, token, gas_limit, base_fee);
		}

		// Case 2: Fee token setup calls (truly feeless)
		Self::is_feeless_internal(caller, target, input)
	}

	/// Returns `true` if the call should have zero gas fee (truly free).
	///
	/// Only fee token setup calls are truly feeless. ERC20 fee users
	/// will pay fees via BifrostFeeAdapter, so they are NOT truly feeless.
	fn is_feeless(caller: H160, target: Option<H160>, input: &[u8]) -> bool {
		Self::is_feeless_internal(caller, target, input)
	}
}

impl<T> BifrostFeelessCalls<T>
where
	T: frame_system::Config + pallet_bifrost_evm_tx_payment::Config,
{
	/// Validate that an ERC20 fee token user has sufficient balance to pay for the transaction.
	///
	/// This is called during pool validation to prevent transactions from users who
	/// don't have enough tokens to cover the estimated fee.
	///
	/// # Arguments
	/// * `caller` - The user's address
	/// * `token` - The ERC20 token address they want to use for fees
	/// * `gas_limit` - The transaction's gas limit
	/// * `base_fee` - The current base fee per gas
	///
	/// # Returns
	/// `true` if the user has sufficient token balance to cover estimated fees
	fn validate_erc20_fee_user(caller: H160, token: H160, gas_limit: U256, base_fee: U256) -> bool {
		// Check if ERC20 fee payment is enabled (native oracle must be set)
		if !Pallet::<T>::is_erc20_fee_enabled() {
			// ERC20 fee payment not enabled, user will fall back to native
			// In this case, don't allow zero-balance callable for ERC20 users
			return false;
		}

		// Check if token is still accepted and enabled
		let config = match AcceptedFeeTokens::<T>::get(token) {
			Some(c) => c,
			None => return false,
		};

		if !config.enabled {
			return false;
		}

		// Calculate estimated native fee: gas_limit * base_fee
		let estimated_native_fee = match gas_limit.checked_mul(base_fee) {
			Some(fee) => fee,
			None => return false, // Overflow
		};

		// Convert native fee to token amount
		let required_token_amount = match Pallet::<T>::convert_native_to_token(estimated_native_fee, token)
		{
			Ok(amount) => amount,
			Err(_) => return false, // Oracle/conversion failure
		};

		// Get user's token balance
		let balance =
			match pallet_bifrost_evm_tx_payment::erc20::get_token_balance::<T>(caller, token) {
				Ok(b) => b,
				Err(_) => return false,
			};

		// Check if user has enough tokens
		balance >= required_token_amount
	}

	/// Returns `true` if this call should have zero gas fee (feeless).
	///
	/// Only the fee token setup calls are feeless:
	/// - `setUserFeeToken(address)` - to set up ERC20 fee payment
	/// - `clearUserFeeToken()` - to clear fee preference
	///
	/// This is called internally and by the trait implementation.
	fn is_feeless_internal(caller: H160, target: Option<H160>, input: &[u8]) -> bool {
		// BifrostTransactionPayment precompile address: 0x0000000000000000000000000000000000000810
		const TX_PAYMENT_PRECOMPILE: H160 =
			H160([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x08, 0x10]);

		// Function selectors for feeless calls (keccak256 of function signature, first 4 bytes)
		// setUserFeeToken(address) => 0x47dee8ee
		const SET_USER_FEE_TOKEN: [u8; 4] = [0x47, 0xde, 0xe8, 0xee];
		// clearUserFeeToken() => 0xc1e1da08
		const CLEAR_USER_FEE_TOKEN: [u8; 4] = [0xc1, 0xe1, 0xda, 0x08];

		if let Some(target) = target {
			if target == TX_PAYMENT_PRECOMPILE && input.len() >= 4 {
				let selector: [u8; 4] = [input[0], input[1], input[2], input[3]];
				return match selector {
					SET_USER_FEE_TOKEN => {
						// Rate limit check first
						if Self::is_rate_limited(caller) {
							return false;
						}

						// Extract a token address from input and validate
						if let Some(token) = Self::extract_token_from_input(input) {
							// Check if the token is accepted and caller has minimum balance
							return Self::validate_feeless_set_token(caller, token);
						}

						// Invalid input format
						false
					},
					CLEAR_USER_FEE_TOKEN => !Self::is_rate_limited(caller),
					_ => false,
				};
			}
		}
		false
	}

	/// Extract token address from `setUserFeeToken(address)` input data.
	///
	/// Input format: 4 bytes selector + 32 bytes address (left-padded)
	fn extract_token_from_input(input: &[u8]) -> Option<H160> {
		// setUserFeeToken(address) requires 4 + 32 = 36 bytes
		if input.len() < 36 {
			return None;
		}

		// Address is in bytes 16..36 (after 4 bytes selector + 12 bytes padding)
		Some(H160::from_slice(&input[16..36]))
	}

	/// Validate that the caller can use feeless setUserFeeToken for the given token.
	///
	/// Checks:
	/// 1. Token is registered in AcceptedFeeTokens
	/// 2. Token is enabled
	/// 3. Caller holds at least `min_balance` of the token (if min_balance > 0)
	fn validate_feeless_set_token(caller: H160, token: H160) -> bool {
		// Get token configuration
		let config = match AcceptedFeeTokens::<T>::get(token) {
			Some(c) => c,
			None => return false,
		};

		// Check if token is enabled
		if !config.enabled {
			return false;
		}

		// Check minimum balance requirement
		if !config.min_balance.is_zero() {
			let balance =
				match pallet_bifrost_evm_tx_payment::erc20::get_token_balance::<T>(caller, token) {
					Ok(b) => b,
					Err(_) => return false,
				};

			if balance < config.min_balance {
				return false;
			}
		}

		true
	}

	/// Check if the caller is currently rate-limited.
	fn is_rate_limited(caller: H160) -> bool {
		let cooldown = T::FeeTokenUpdateCooldown::get();

		// If cooldown is 0, rate limiting is disabled
		if cooldown.is_zero() {
			return false;
		}

		let current_block = frame_system::Pallet::<T>::block_number();

		if let Some(last_update) = LastFeeTokenUpdate::<T>::get(caller) {
			let earliest_allowed = last_update.saturating_add(cooldown);
			return current_block < earliest_allowed;
		}

		false
	}
}
