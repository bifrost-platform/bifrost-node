#![cfg_attr(not(feature = "std"), no_std)]

mod apis;
mod self_contained_call;

pub mod extensions;

use frame_support::traits::Get;
use pallet_bifrost_evm_tx_payment::LastFeeTokenUpdate;
use sp_core::H160;
use sp_runtime::traits::{Saturating, Zero};
use sp_std::marker::PhantomData;

/// Filter for feeless EVM calls.
///
/// Allows certain precompile calls (like setting/clearing fee tokens) to be
/// executed without paying gas fees. This prevents a chicken-and-egg problem
/// where users need to pay fees to set up their fee token preference.
///
/// If the caller is rate-limited, the call is NOT feeless (they must pay gas).
/// This prevents spam attacks where attackers repeatedly call feeless functions
/// that would revert anyway.
pub struct BifrostFeelessCalls<T>(PhantomData<T>);

impl<T> pallet_evm::FeelessCallFilter for BifrostFeelessCalls<T>
where
	T: frame_system::Config + pallet_bifrost_evm_tx_payment::Config,
{
	fn is_feeless(caller: H160, target: Option<H160>, input: &[u8]) -> bool {
		// BifrostTransactionPayment precompile address: 0x0000000000000000000000000000000000000810
		const TX_PAYMENT_PRECOMPILE: H160 =
			H160([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x08, 0x10]);

		// Function selectors for feeless calls
		const SET_USER_FEE_TOKEN: [u8; 4] = [0x8a, 0x5f, 0xb3, 0xe4]; // setUserFeeToken(address)
		const CLEAR_USER_FEE_TOKEN: [u8; 4] = [0x3e, 0x6e, 0x0a, 0x18]; // clearUserFeeToken()

		if let Some(target) = target {
			if target == TX_PAYMENT_PRECOMPILE && input.len() >= 4 {
				let selector: [u8; 4] = [input[0], input[1], input[2], input[3]];
				if selector == SET_USER_FEE_TOKEN || selector == CLEAR_USER_FEE_TOKEN {
					// Check rate limit - if rate limited, NOT feeless (must pay gas)
					return !Self::is_rate_limited(caller);
				}
			}
		}
		false
	}
}

impl<T> BifrostFeelessCalls<T>
where
	T: frame_system::Config + pallet_bifrost_evm_tx_payment::Config,
{
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
