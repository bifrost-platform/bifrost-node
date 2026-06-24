//! Bifrost Transaction Payment Precompile
//!
//! This precompile provides a Solidity-accessible interface for:
//! - Setting user's preferred fee token
//! - Querying fee token information
//! - Estimating fees in different tokens
//!
//! ## Precompile Address
//! This precompile should be registered at address `0x0000000000000000000000000000000000000810`
//!
//! ## Solidity Interface
//! ```solidity
//! interface IBifrostTransactionPayment {
//!     function setUserFeeToken(address token) external;
//!     function clearUserFeeToken() external;
//!     function getUserFeeToken(address user) external view returns (address);
//!     function getUsersFeeToken() external view returns (address[] memory users, address[] memory tokens);
//!     function estimateFeeInToken(address token, uint256 gasAmount) external view returns (uint256);
//!     function isAcceptedToken(address token) external view returns (bool);
//!     function getTokenConfig(address token) external view returns (
//!         bool enabled,
//!         uint8 decimals,
//!         uint256 minBalance
//!     );
//!     function getTokenPrice(address token) external view returns (uint256);
//! }
//! ```

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]

use frame_support::{
	dispatch::{GetDispatchInfo, PostDispatchInfo},
	traits::Get,
};
use pallet_bifrost_evm_tx_payment::{AcceptedFeeTokens, LastFeeTokenUpdate, Pallet, UserFeeToken};
use pallet_evm::FeeCalculator;
use precompile_utils::prelude::*;
use sp_core::{H160, U256};
use sp_runtime::{traits::Dispatchable, Saturating};
use sp_std::{marker::PhantomData, vec::Vec};

/// The precompile for Bifrost transaction payment management.
///
/// Provides functions for users to manage their fee token preferences
/// and query fee-related information.
pub struct BifrostTransactionPaymentPrecompile<Runtime>(PhantomData<Runtime>);

#[precompile_utils::precompile]
impl<Runtime> BifrostTransactionPaymentPrecompile<Runtime>
where
	Runtime: pallet_bifrost_evm_tx_payment::Config + pallet_evm::Config + frame_system::Config,
	Runtime::RuntimeCall: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	<Runtime::RuntimeCall as Dispatchable>::RuntimeOrigin: From<Option<Runtime::AccountId>>,
	Runtime::RuntimeCall: From<pallet_bifrost_evm_tx_payment::Call<Runtime>>,
{
	// ========================================================================
	// User Functions
	// ========================================================================

	/// Set the caller's preferred fee token.
	///
	/// Rate limited: users must wait `FeeTokenUpdateCooldown` blocks between changes.
	///
	/// Selector: `setUserFeeToken(address)`
	/// Signature: `0x47dee8ee`
	#[precompile::public("setUserFeeToken(address)")]
	fn set_user_fee_token(handle: &mut impl PrecompileHandle, token: Address) -> EvmResult {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost() * 2)?; // token config + last update
		handle.record_cost(RuntimeHelper::<Runtime>::db_write_gas_cost() * 2)?; // user token + last update

		let caller = handle.context().caller;
		let token_h160: H160 = token.into();

		// Rate limit check
		Self::check_rate_limit(caller)?;

		// Check if token is accepted
		let config = AcceptedFeeTokens::<Runtime>::get(token_h160)
			.ok_or(revert("Token not accepted for fee payment"))?;

		if !config.enabled {
			return Err(revert("Token is currently disabled"));
		}

		// Store user preference
		UserFeeToken::<Runtime>::insert(caller, token_h160);

		// Update last change timestamp
		let current_block = frame_system::Pallet::<Runtime>::block_number();
		LastFeeTokenUpdate::<Runtime>::insert(caller, current_block);

		// Emit log event
		log1(
			handle.context().address,
			SELECTOR_LOG_USER_FEE_TOKEN_SET,
			solidity::encode_event_data((Address(caller), Address(token_h160))),
		)
		.record(handle)?;

		Ok(())
	}

	/// Clear the caller's fee token preference (use native token).
	///
	/// Rate limited: users must wait `FeeTokenUpdateCooldown` blocks between changes.
	///
	/// Selector: `clearUserFeeToken()`
	/// Signature: `0xc1e1da08`
	#[precompile::public("clearUserFeeToken()")]
	fn clear_user_fee_token(handle: &mut impl PrecompileHandle) -> EvmResult {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?; // last update check
		handle.record_cost(RuntimeHelper::<Runtime>::db_write_gas_cost() * 2)?; // user token + last update

		let caller = handle.context().caller;

		// Rate limit check
		Self::check_rate_limit(caller)?;

		UserFeeToken::<Runtime>::remove(caller);

		// Update last change timestamp
		let current_block = frame_system::Pallet::<Runtime>::block_number();
		LastFeeTokenUpdate::<Runtime>::insert(caller, current_block);

		log1(
			handle.context().address,
			SELECTOR_LOG_USER_FEE_TOKEN_SET,
			solidity::encode_event_data((Address(caller), Address(H160::zero()))),
		)
		.record(handle)?;

		Ok(())
	}

	// ========================================================================
	// View Functions
	// ========================================================================

	/// Get a user's current fee token preference.
	///
	/// Returns zero address if user uses native token.
	///
	/// Selector: `getUserFeeToken(address)`
	/// Signature: `0x37e8e5d1`
	#[precompile::public("getUserFeeToken(address)")]
	#[precompile::view]
	fn get_user_fee_token(handle: &mut impl PrecompileHandle, user: Address) -> EvmResult<Address> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let user_h160: H160 = user.into();
		let token = UserFeeToken::<Runtime>::get(user_h160).unwrap_or(H160::zero());

		Ok(Address(token))
	}

	/// Get all users who have set a fee token preference.
	///
	/// Returns two arrays: users and their corresponding tokens.
	///
	/// WARNING: This function iterates over all storage entries and may be expensive.
	///
	/// Selector: `getUsersFeeToken()`
	/// Signature: `0x56ebb34c`
	#[precompile::public("getUsersFeeToken()")]
	#[precompile::view]
	fn get_users_fee_token(
		handle: &mut impl PrecompileHandle,
	) -> EvmResult<(Vec<Address>, Vec<Address>)> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost() * 10)?;

		let mut users: Vec<Address> = Vec::new();
		let mut tokens: Vec<Address> = Vec::new();

		for (user, token) in UserFeeToken::<Runtime>::iter() {
			users.push(Address(user));
			tokens.push(Address(token));

			handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		}

		Ok((users, tokens))
	}

	/// Estimate the fee amount in a specific token.
	///
	/// Selector: `estimateFeeInToken(address,uint256)`
	/// Signature: `0x6da6a16e`
	#[precompile::public("estimateFeeInToken(address,uint256)")]
	#[precompile::view]
	fn estimate_fee_in_token(
		handle: &mut impl PrecompileHandle,
		token: Address,
		gas_amount: U256,
	) -> EvmResult<U256> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost() * 2)?;

		let token_h160: H160 = token.into();

		// Get base fee per gas
		let (base_fee, _) = <Runtime as pallet_evm::Config>::FeeCalculator::min_gas_price();

		// Calculate native fee
		let native_fee =
			base_fee.checked_mul(gas_amount).ok_or(revert("Fee calculation overflow"))?;

		// Convert to token amount
		let token_fee = Pallet::<Runtime>::convert_native_to_token(native_fee, token_h160)
			.map_err(|_| revert("Failed to convert fee to token amount"))?;

		Ok(token_fee)
	}

	/// Check if a token is accepted for fee payment.
	///
	/// Selector: `isAcceptedToken(address)`
	/// Signature: `0x3b6e750f`
	#[precompile::public("isAcceptedToken(address)")]
	#[precompile::view]
	fn is_accepted_token(handle: &mut impl PrecompileHandle, token: Address) -> EvmResult<bool> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let token_h160: H160 = token.into();
		let is_accepted = Pallet::<Runtime>::is_token_accepted(&token_h160);

		Ok(is_accepted)
	}

	/// Get the configuration for a fee token.
	///
	/// Selector: `getTokenConfig(address)`
	/// Signature: `0xcb67e3b1`
	///
	/// Returns: (enabled, decimals, minBalance)
	#[precompile::public("getTokenConfig(address)")]
	#[precompile::view]
	fn get_token_config(
		handle: &mut impl PrecompileHandle,
		token: Address,
	) -> EvmResult<(bool, u8, U256)> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let token_h160: H160 = token.into();

		let config =
			AcceptedFeeTokens::<Runtime>::get(token_h160).ok_or(revert("Token not found"))?;

		Ok((
			config.enabled,
			config.decimals,
			config.min_balance,
		))
	}

	/// Get the current oracle price for a token.
	///
	/// Queries the oracle-registry for the token's price.
	///
	/// Selector: `getTokenPrice(address)`
	/// Signature: `0xd02641a0`
	#[precompile::public("getTokenPrice(address)")]
	#[precompile::view]
	fn get_token_price(handle: &mut impl PrecompileHandle, token: Address) -> EvmResult<U256> {
		// This call invokes the oracle, so charge more gas
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost() * 5)?;

		let token_h160: H160 = token.into();

		let price =
			pallet_bifrost_evm_tx_payment::oracle::get_token_price_via_registry::<Runtime>(
				token_h160,
				0,
			)
			.map_err(|_| revert("Failed to get oracle price"))?;

		Ok(price)
	}

	/// Get the native token fee for a given gas amount.
	///
	/// Selector: `getNativeFee(uint256)`
	/// Signature: `0x06465844`
	#[precompile::public("getNativeFee(uint256)")]
	#[precompile::view]
	fn get_native_fee(handle: &mut impl PrecompileHandle, gas_amount: U256) -> EvmResult<U256> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let (base_fee, _) = <Runtime as pallet_evm::Config>::FeeCalculator::min_gas_price();

		let native_fee =
			base_fee.checked_mul(gas_amount).ok_or(revert("Fee calculation overflow"))?;

		Ok(native_fee)
	}

	// ========================================================================
	// Internal Helpers
	// ========================================================================

	/// Check if the caller is rate limited from changing their fee token.
	fn check_rate_limit(caller: H160) -> EvmResult {
		use sp_runtime::traits::Zero;

		let cooldown =
			<Runtime as pallet_bifrost_evm_tx_payment::Config>::FeeTokenUpdateCooldown::get();

		// If cooldown is 0, rate limiting is disabled
		if cooldown.is_zero() {
			return Ok(());
		}

		let current_block = frame_system::Pallet::<Runtime>::block_number();

		if let Some(last_update) = LastFeeTokenUpdate::<Runtime>::get(caller) {
			let earliest_allowed = last_update.saturating_add(cooldown);
			if current_block < earliest_allowed {
				return Err(revert("Rate limited: please wait before changing fee token"));
			}
		}

		Ok(())
	}
}

// ============================================================================
// Event Selectors
// ============================================================================

/// Event selector for `UserFeeTokenSet(address indexed user, address token)`
/// keccak256("UserFeeTokenSet(address,address)")
const SELECTOR_LOG_USER_FEE_TOKEN_SET: [u8; 32] = keccak256!("UserFeeTokenSet(address,address)");
