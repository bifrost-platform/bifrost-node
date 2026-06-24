//! ERC20 token interaction via pallet-evm Runner.
//!
//! This module provides functions to interact with ERC20 contracts
//! from within the Substrate runtime using pallet-evm's Runner::call.

use pallet_evm::{ExitReason, Runner};
use sp_core::{H160, U256};
use sp_std::vec::Vec;

/// Gas limit for ERC20 transfer calls.
/// 100,000 gas is sufficient for standard ERC20 transfers.
const ERC20_TRANSFER_GAS_LIMIT: u64 = 100_000;

/// Gas limit for ERC20 view calls (balanceOf, etc.).
/// 50,000 gas is sufficient for standard ERC20 view functions.
const ERC20_VIEW_GAS_LIMIT: u64 = 50_000;

/// ERC20 function selectors.
mod selectors {
	/// `transfer(address,uint256)` selector: 0xa9059cbb
	pub const TRANSFER: [u8; 4] = [0xa9, 0x05, 0x9c, 0xbb];
	/// `balanceOf(address)` selector: 0x70a08231
	pub const BALANCE_OF: [u8; 4] = [0x70, 0xa0, 0x82, 0x31];
}

/// Execute ERC20 `transfer(to, amount)` from user to fee collector (precompile address).
///
/// This is the primary function for fee payment - user transfers tokens
/// to the precompile address.
///
/// # Arguments
/// * `from` - Address to transfer tokens from (user, msg.sender)
/// * `token` - ERC20 token contract address
/// * `to` - Address to transfer tokens to (precompile address)
/// * `amount` - Amount of tokens to transfer
///
/// # Note
/// User is the msg.sender, so no approval is needed.
/// This uses `call_as_internal_call` to avoid incrementing the user's nonce,
/// as this transfer is part of the main transaction's fee payment.
pub fn transfer_from_user<T: crate::Config>(
	from: H160,
	token: H160,
	to: H160,
	amount: U256,
) -> Result<(), ()> {
	if amount.is_zero() {
		return Ok(());
	}

	// Encode calldata: transfer(address to, uint256 amount)
	let calldata = encode_transfer(to, amount);

	log::debug!(
		target: "bifrost-tx-payment",
		"transfer_from_user: from={:?}, to={:?}, token={:?}, amount={}",
		from, to, token, amount
	);

	// Use call_as_internal_call to execute the transfer without incrementing nonce.
	// This is an internal call that is part of the fee payment, not a separate transaction.
	// User is the msg.sender, transferring their own tokens.
	let result = T::Runner::call_as_internal_call(
		from,                     // source (msg.sender = user)
		token,                    // target (ERC20 contract)
		calldata,                 // input
		ERC20_TRANSFER_GAS_LIMIT, // gas_limit
		T::config(),
	);

	let result = match result {
		Ok(r) => {
			log::debug!(
				target: "bifrost-tx-payment",
				"transfer_from_user: Runner::call succeeded, exit_reason={:?}, return_data={:?}",
				r.exit_reason, r.value
			);
			r
		},
		Err(e) => {
			log::warn!(
				target: "bifrost-tx-payment",
				"transfer_from_user: Runner::call failed ({:?})",
				e.error.into()
			);
			return Err(());
		},
	};

	// Check execution result
	match result.exit_reason {
		ExitReason::Succeed(_) => {
			// Check return value - ERC20 returns true on success
			if !check_bool_return(&result.value) {
				log::warn!(
					target: "bifrost-tx-payment",
					"transfer_from_user: ERC20 returned false or invalid data: {:?}",
					result.value
				);
				return Err(());
			}
			log::debug!(target: "bifrost-tx-payment", "transfer_from_user: SUCCESS");
			Ok(())
		},
		ref reason => {
			log::warn!(
				target: "bifrost-tx-payment",
				"transfer_from_user: EVM execution failed with reason: {:?}",
				reason
			);
			Err(())
		},
	}
}

/// Execute ERC20 `transfer(to, amount)` from fee collector (precompile) to user.
///
/// Used for refunds and tip payments - transfers tokens from precompile to user/author.
///
/// # Arguments
/// * `from` - Address to transfer from (precompile address, msg.sender)
/// * `token` - ERC20 token contract address
/// * `to` - Address to transfer to (user or block author)
/// * `amount` - Amount of tokens to transfer
///
/// # Note
/// This uses `call_as_internal_call` to avoid incrementing the precompile's nonce,
/// as this transfer is part of the main transaction's fee correction/tip payment.
pub fn transfer_to_user<T: crate::Config>(
	from: H160,
	token: H160,
	to: H160,
	amount: U256,
) -> Result<(), ()> {
	if amount.is_zero() {
		return Ok(());
	}

	// Encode calldata: transfer(address to, uint256 amount)
	let calldata = encode_transfer(to, amount);

	log::debug!(
		target: "bifrost-tx-payment",
		"transfer_to_user: from={:?}, to={:?}, token={:?}, amount={}",
		from, to, token, amount
	);

	// Use call_as_internal_call to execute the transfer without incrementing nonce.
	// This is an internal call that is part of the fee refund/tip, not a separate transaction.
	let result = match T::Runner::call_as_internal_call(
		from,                     // source (precompile address)
		token,                    // target (ERC20 contract)
		calldata,                 // input
		ERC20_TRANSFER_GAS_LIMIT, // gas_limit
		T::config(),
	) {
		Ok(r) => r,
		Err(e) => {
			log::error!(
				target: "bifrost-tx-payment",
				"transfer_to_user: Runner::call failed ({:?})",
				e.error.into()
			);
			return Err(());
		},
	};

	match result.exit_reason {
		ExitReason::Succeed(_) => {
			if !check_bool_return(&result.value) {
				log::warn!(
					target: "bifrost-tx-payment",
					"transfer_to_user: ERC20 returned false or invalid data: {:?}",
					result.value
				);
				return Err(());
			}
			log::debug!(target: "bifrost-tx-payment", "transfer_to_user: SUCCESS");
			Ok(())
		},
		ref reason => {
			log::warn!(
				target: "bifrost-tx-payment",
				"transfer_to_user: EVM execution failed with reason: {:?}",
				reason
			);
			Err(())
		},
	}
}

/// Get ERC20 token balance for an account.
///
/// Calls the `balanceOf(address)` function on the token contract
/// using `Runner::view_call` which does not modify state.
///
/// # Arguments
/// * `account` - Address to check balance for
/// * `token` - ERC20 token contract address
///
/// # Returns
/// * `Ok(U256)` - The token balance
/// * `Err(())` - If the call fails or returns invalid data
pub fn get_token_balance<T: crate::Config>(account: H160, token: H160) -> Result<U256, ()> {
	// Encode calldata: balanceOf(address account)
	let calldata = encode_balance_of(account);

	log::debug!(
		target: "bifrost-tx-payment",
		"get_token_balance: account={:?}, token={:?}",
		account, token
	);

	// Use view_call for read-only operation (no state changes, no nonce increment)
	let result = match T::Runner::view_call(
		account,              // source (used for context only)
		token,                // target (ERC20 contract)
		calldata,             // input
		ERC20_VIEW_GAS_LIMIT, // gas_limit
		T::config(),
	) {
		Ok(r) => {
			log::debug!(
				target: "bifrost-tx-payment",
				"get_token_balance: view_call succeeded, exit_reason={:?}",
				r.exit_reason
			);
			r
		},
		Err(e) => {
			log::warn!(
				target: "bifrost-tx-payment",
				"get_token_balance: view_call failed ({:?})",
				e.error.into()
			);
			return Err(());
		},
	};

	// Check execution result
	match result.exit_reason {
		ExitReason::Succeed(_) => {
			// Decode U256 from return data
			decode_u256_return(&result.value).ok_or_else(|| {
				log::warn!(
					target: "bifrost-tx-payment",
					"get_token_balance: failed to decode balance from return data: {:?}",
					result.value
				);
			})
		},
		ref reason => {
			log::warn!(
				target: "bifrost-tx-payment",
				"get_token_balance: EVM execution failed with reason: {:?}",
				reason
			);
			Err(())
		},
	}
}

// ============================================================================
// ABI Encoding Functions
// ============================================================================

/// Encode `balanceOf(address account)` calldata.
fn encode_balance_of(account: H160) -> Vec<u8> {
	let mut calldata = Vec::with_capacity(4 + 32);

	// Function selector
	calldata.extend_from_slice(&selectors::BALANCE_OF);

	// account address (32 bytes, left-padded)
	calldata.extend_from_slice(&[0u8; 12]);
	calldata.extend_from_slice(account.as_bytes());

	calldata
}

/// Encode `transfer(address to, uint256 amount)` calldata.
fn encode_transfer(to: H160, amount: U256) -> Vec<u8> {
	let mut calldata = Vec::with_capacity(4 + 32 * 2);

	// Function selector
	calldata.extend_from_slice(&selectors::TRANSFER);

	// to address (32 bytes, left-padded)
	calldata.extend_from_slice(&[0u8; 12]);
	calldata.extend_from_slice(to.as_bytes());

	// amount (32 bytes) - U256 is big-endian encoded
	let amount_bytes: [u8; 32] = amount.to_big_endian();
	calldata.extend_from_slice(&amount_bytes);

	calldata
}

// ============================================================================
// ABI Decoding Functions
// ============================================================================

/// Decode a U256 value from EVM return data.
///
/// EVM returns U256 as 32 bytes in big-endian format.
fn decode_u256_return(data: &[u8]) -> Option<U256> {
	if data.len() < 32 {
		return None;
	}

	// U256 is returned as 32 bytes big-endian
	Some(U256::from_big_endian(&data[0..32]))
}

/// Check if ERC20 call returned true.
///
/// ERC20 functions return a boolean. Some implementations return empty
/// data on success, so we treat empty data as success.
fn check_bool_return(data: &[u8]) -> bool {
	if data.is_empty() {
		// Some ERC20 tokens don't return anything on success (like USDT)
		return true;
	}

	if data.len() < 32 {
		return false;
	}

	// Check if last byte is 1 (true) or all zeros except last byte
	data[31] == 1 && data[0..31].iter().all(|&b| b == 0)
}
