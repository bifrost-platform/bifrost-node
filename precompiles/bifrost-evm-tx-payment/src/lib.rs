//! EVM Fee Token Precompile
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
//! interface IEVMFeeToken {
//!     function setUserFeeToken(address token) external;
//!     function clearUserFeeToken() external;
//!     function getUserFeeToken(address user) external view returns (address);
//!     function estimateFeeInToken(address token, uint256 gasAmount) external view returns (uint256);
//!     function isAcceptedToken(address token) external view returns (bool);
//!     function getTokenConfig(address token) external view returns (
//!         bool enabled,
//!         address oracle,
//!         uint8 decimals,
//!         uint8 oracleDecimals
//!     );
//! }
//! ```

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]

use frame_support::dispatch::{GetDispatchInfo, PostDispatchInfo};
use pallet_bifrost_evm_tx_payment::{AcceptedFeeTokens, Pallet, UserFeeToken};
use pallet_evm::FeeCalculator;
use precompile_utils::prelude::*;
use sp_core::{H160, U256};
use sp_runtime::traits::Dispatchable;
use sp_std::marker::PhantomData;

/// The precompile for EVM fee token management.
///
/// Provides functions for users to manage their fee token preferences
/// and query fee-related information.
pub struct EVMFeeTokenPrecompile<Runtime>(PhantomData<Runtime>);

#[precompile_utils::precompile]
impl<Runtime> EVMFeeTokenPrecompile<Runtime>
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
	/// The user must have approved the treasury address to spend their tokens
	/// before setting a fee token.
	///
	/// Selector: `setUserFeeToken(address)`
	/// Signature: `0x8a5fb3e4`
	#[precompile::public("setUserFeeToken(address)")]
	fn set_user_fee_token(handle: &mut impl PrecompileHandle, token: Address) -> EvmResult {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		handle.record_cost(RuntimeHelper::<Runtime>::db_write_gas_cost())?;

		let caller = handle.context().caller;
		let token_h160: H160 = token.into();

		// Check if token is accepted
		let config = AcceptedFeeTokens::<Runtime>::get(token_h160)
			.ok_or(revert("Token not accepted for fee payment"))?;

		if !config.enabled {
			return Err(revert("Token is currently disabled"));
		}

		// Store user preference
		UserFeeToken::<Runtime>::insert(caller, token_h160);

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
	/// Selector: `clearUserFeeToken()`
	/// Signature: `0x3e6e0a18`
	#[precompile::public("clearUserFeeToken()")]
	fn clear_user_fee_token(handle: &mut impl PrecompileHandle) -> EvmResult {
		handle.record_cost(RuntimeHelper::<Runtime>::db_write_gas_cost())?;

		let caller = handle.context().caller;

		UserFeeToken::<Runtime>::remove(caller);

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
	/// Signature: `0x5c7a5562`
	#[precompile::public("getUserFeeToken(address)")]
	#[precompile::view]
	fn get_user_fee_token(handle: &mut impl PrecompileHandle, user: Address) -> EvmResult<Address> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let user_h160: H160 = user.into();
		let token = UserFeeToken::<Runtime>::get(user_h160).unwrap_or(H160::zero());

		Ok(Address(token))
	}

	/// Estimate the fee amount in a specific token.
	///
	/// Selector: `estimateFeeInToken(address,uint256)`
	/// Signature: `0x7e7c0a5e`
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
	/// Signature: `0x5186d86f`
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
	/// Signature: `0xc7e074c3`
	///
	/// Returns: (enabled, oracle, decimals, oracleDecimals)
	#[precompile::public("getTokenConfig(address)")]
	#[precompile::view]
	fn get_token_config(
		handle: &mut impl PrecompileHandle,
		token: Address,
	) -> EvmResult<(bool, Address, u8, u8)> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let token_h160: H160 = token.into();

		let config =
			AcceptedFeeTokens::<Runtime>::get(token_h160).ok_or(revert("Token not found"))?;

		Ok((config.enabled, Address(config.oracle_address), config.decimals, config.oracle_decimals))
	}

	/// Get the current oracle price for a token.
	///
	/// Selector: `getTokenPrice(address)`
	/// Signature: `0xc495f2ea`
	#[precompile::public("getTokenPrice(address)")]
	#[precompile::view]
	fn get_token_price(handle: &mut impl PrecompileHandle, token: Address) -> EvmResult<U256> {
		// This call invokes the oracle, so charge more gas
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost() * 5)?;

		let token_h160: H160 = token.into();

		let config =
			AcceptedFeeTokens::<Runtime>::get(token_h160).ok_or(revert("Token not found"))?;

		let price =
			pallet_bifrost_evm_tx_payment::oracle::get_oracle_price::<Runtime>(config.oracle_address)
				.map_err(|_| revert("Failed to get oracle price"))?;

		Ok(price)
	}

	/// Get the native token fee for a given gas amount.
	///
	/// Selector: `getNativeFee(uint256)`
	/// Signature: `0x5a0a6e2d`
	#[precompile::public("getNativeFee(uint256)")]
	#[precompile::view]
	fn get_native_fee(handle: &mut impl PrecompileHandle, gas_amount: U256) -> EvmResult<U256> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let (base_fee, _) = <Runtime as pallet_evm::Config>::FeeCalculator::min_gas_price();

		let native_fee =
			base_fee.checked_mul(gas_amount).ok_or(revert("Fee calculation overflow"))?;

		Ok(native_fee)
	}
}

// ============================================================================
// Event Selectors
// ============================================================================

/// Event selector for `UserFeeTokenSet(address indexed user, address token)`
/// keccak256("UserFeeTokenSet(address,address)")
const SELECTOR_LOG_USER_FEE_TOKEN_SET: [u8; 32] = keccak256!("UserFeeTokenSet(address,address)");
