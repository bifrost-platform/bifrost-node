//! Custom OnChargeEVMTransaction implementation for ERC20 fee payment.
//!
//! This module provides `ERC20FeeAdapter` which implements the `OnChargeEVMTransaction`
//! trait from pallet-evm, enabling users to pay gas fees in ERC20 tokens.

use crate::{Config, Pallet, UserFeeToken};
use frame_support::traits::{
	tokens::{
		currency::Currency,
		imbalance::{Imbalance, OnUnbalanced},
		ExistenceRequirement, WithdrawReasons,
	},
	Get,
};
use pallet_evm::{AccountIdOf, AddressMapping, Error, OnChargeEVMTransaction};
use sp_core::{H160, U256};
use sp_runtime::{traits::UniqueSaturatedInto, Saturating};
use sp_std::marker::PhantomData;

/// Balance type alias - using pallet_evm::AccountIdOf which resolves properly.
pub type BalanceOf<T, C> = <C as Currency<AccountIdOf<T>>>::Balance;

/// Negative imbalance type alias.
pub type NegativeImbalanceOf<T, C> = <C as Currency<AccountIdOf<T>>>::NegativeImbalance;

/// Fee payment information for tracking what was withdrawn.
#[derive(Default)]
pub enum LiquidityInfo<T: pallet_evm::Config, C: Currency<AccountIdOf<T>>> {
	/// No fee was withdrawn.
	#[default]
	None,
	/// Fee was paid in native token.
	Native(Option<NegativeImbalanceOf<T, C>>),
	/// Fee was withdrawn in ERC20 token (returned from withdraw_fee).
	ERC20Withdrawn {
		/// Token address.
		token: H160,
		/// Amount withdrawn in tokens.
		amount: U256,
		/// Equivalent amount in native token (for price calculation).
		native_equivalent: U256,
	},
	/// Tip to be paid in ERC20 token (returned from correct_and_deposit_fee).
	ERC20Tip {
		/// Token address.
		token: H160,
		/// Tip amount in tokens.
		tip_amount: U256,
	},
}

/// ERC20 Fee Adapter for EVM transactions.
///
/// This adapter implements `OnChargeEVMTransaction` and supports both:
/// - Native token (BFC) fee payment (default behavior)
/// - ERC20 token fee payment (when user has set preference)
///
/// # Type Parameters
/// * `T` - Runtime configuration (must implement both `pallet_evm::Config` and `pallet_bifrost_evm_tx_payment::Config`)
/// * `C` - Currency type for native token operations
/// * `OU` - OnUnbalanced handler for fee distribution (burn/treasury)
pub struct ERC20FeeAdapter<T, C, OU>(PhantomData<(T, C, OU)>);

impl<T, C, OU> OnChargeEVMTransaction<T> for ERC20FeeAdapter<T, C, OU>
where
	T: pallet_evm::Config + Config,
	C: Currency<AccountIdOf<T>>,
	C::PositiveImbalance: Imbalance<C::Balance, Opposite = C::NegativeImbalance>,
	C::NegativeImbalance: Imbalance<C::Balance, Opposite = C::PositiveImbalance>,
	OU: OnUnbalanced<NegativeImbalanceOf<T, C>>,
	U256: UniqueSaturatedInto<BalanceOf<T, C>>,
{
	type LiquidityInfo = LiquidityInfo<T, C>;

	/// Withdraw fee from the user.
	///
	/// This is called before transaction execution to secure the fee payment.
	fn withdraw_fee(who: &H160, fee: U256) -> Result<Self::LiquidityInfo, Error<T>> {
		if fee.is_zero() {
			return Ok(LiquidityInfo::None);
		}

		// Check if ERC20 fee payment is enabled (native oracle must be set)
		if !Pallet::<T>::is_erc20_fee_enabled() {
			log::debug!(
				target: "evm-fee-token",
				"ERC20 fee payment disabled (native oracle not set), using native"
			);
			return Self::withdraw_native_fee(who, fee);
		}

		// Check if user has set a fee token preference
		let fee_token = UserFeeToken::<T>::get(who);

		log::debug!(
			target: "evm-fee-token",
			"withdraw_fee called: who={:?}, fee={:?}, user_fee_token={:?}",
			who, fee, fee_token
		);

		match fee_token {
			None => {
				// Use native token (existing behavior)
				log::debug!(target: "evm-fee-token", "No fee token set, using native");
				Self::withdraw_native_fee(who, fee)
			},
			Some(token) => {
				// Use ERC20 token
				log::debug!(target: "evm-fee-token", "Using ERC20 token {:?}", token);
				Self::withdraw_erc20_fee(who, fee, token)
			},
		}
	}

	/// Correct the fee and deposit/refund as needed.
	///
	/// Called after transaction execution with the actual fee used.
	fn correct_and_deposit_fee(
		who: &H160,
		corrected_fee: U256,
		base_fee: U256,
		already_withdrawn: Self::LiquidityInfo,
	) -> Self::LiquidityInfo {
		match already_withdrawn {
			LiquidityInfo::None => LiquidityInfo::None,

			LiquidityInfo::Native(imbalance) => {
				Self::correct_native_fee(who, corrected_fee, base_fee, imbalance)
			},

			LiquidityInfo::ERC20Withdrawn { token, amount, native_equivalent } => {
				Self::correct_erc20_fee(
					who,
					corrected_fee,
					base_fee,
					token,
					amount,
					native_equivalent,
				)
			},

			LiquidityInfo::ERC20Tip { .. } => {
				// This variant should only be returned, not passed in
				LiquidityInfo::None
			},
		}
	}

	/// Pay the priority fee (tip) to the block author.
	fn pay_priority_fee(tip: Self::LiquidityInfo) {
		match tip {
			LiquidityInfo::Native(Some(tip_imbalance)) => {
				// Pay tip to block author in native token
				let author = <pallet_evm::Pallet<T>>::find_author();
				let account_id = T::AddressMapping::into_account_id(author);

				let _ = C::deposit_into_existing(&account_id, tip_imbalance.peek());
			},

			LiquidityInfo::ERC20Tip { token, tip_amount } => {
				// For ERC20 tips, transfer from fee collector (precompile) to block author
				if !tip_amount.is_zero() {
					let author = <pallet_evm::Pallet<T>>::find_author();
					let fee_collector = T::FeeCollectorAddress::get();
					if crate::erc20::transfer_to_user::<T>(fee_collector, token, author, tip_amount)
						.is_ok()
					{
						// Emit tip payment event
						Pallet::<T>::deposit_event(crate::Event::TipPaymentInToken {
							author,
							token,
							amount: tip_amount,
						});
					}
				}
			},

			_ => {},
		}
	}
}

impl<T, C, OU> ERC20FeeAdapter<T, C, OU>
where
	T: pallet_evm::Config + Config,
	C: Currency<AccountIdOf<T>>,
	C::PositiveImbalance: Imbalance<C::Balance, Opposite = C::NegativeImbalance>,
	C::NegativeImbalance: Imbalance<C::Balance, Opposite = C::PositiveImbalance>,
	OU: OnUnbalanced<NegativeImbalanceOf<T, C>>,
	U256: UniqueSaturatedInto<BalanceOf<T, C>>,
{
	/// Withdraw fee in native token (BFC).
	///
	/// This is the standard behavior from EVMCurrencyAdapter.
	fn withdraw_native_fee(who: &H160, fee: U256) -> Result<LiquidityInfo<T, C>, Error<T>> {
		let account_id = T::AddressMapping::into_account_id(*who);

		let imbalance = C::withdraw(
			&account_id,
			fee.unique_saturated_into(),
			WithdrawReasons::FEE,
			ExistenceRequirement::AllowDeath,
		)
		.map_err(|_| Error::<T>::BalanceLow)?;

		Ok(LiquidityInfo::Native(Some(imbalance)))
	}

	/// Withdraw fee in ERC20 token.
	///
	/// 1. Converts native fee to token amount using oracle
	/// 2. Executes ERC20 transferFrom to treasury
	/// 3. Falls back to native token on any failure
	fn withdraw_erc20_fee(
		who: &H160,
		native_fee: U256,
		token: H160,
	) -> Result<LiquidityInfo<T, C>, Error<T>> {
		// Check if token is still accepted and enabled
		let config = match crate::AcceptedFeeTokens::<T>::get(token) {
			Some(c) => c,
			None => {
				// Token not registered, fallback to native
				log::warn!(
					target: "evm-fee-token",
					"ERC20 fee payment failed: token {:?} not registered, falling back to native",
					token
				);
				return Self::withdraw_native_fee(who, native_fee);
			},
		};

		if !config.enabled {
			// Fallback to native token if token is disabled
			log::warn!(
				target: "evm-fee-token",
				"ERC20 fee payment failed: token {:?} is disabled, falling back to native",
				token
			);
			return Self::withdraw_native_fee(who, native_fee);
		}

		// Convert native fee to token amount
		let token_amount = match Pallet::<T>::convert_native_to_token(native_fee, token) {
			Ok(amount) => amount,
			Err(e) => {
				// Oracle/conversion failed, fallback to native
				log::warn!(
					target: "evm-fee-token",
					"ERC20 fee payment failed: price conversion error {:?}, falling back to native",
					e
				);
				return Self::withdraw_native_fee(who, native_fee);
			},
		};

		// Execute ERC20 transferFrom
		// User must have approved treasury to spend their tokens
		if let Err(e) = Pallet::<T>::execute_fee_transfer(*who, token, token_amount) {
			// ERC20 transfer failed (insufficient balance/allowance), fallback to native
			log::warn!(
				target: "evm-fee-token",
				"ERC20 fee payment failed: transfer error {:?} for user {:?}, amount {:?}, falling back to native",
				e, who, token_amount
			);
			return Self::withdraw_native_fee(who, native_fee);
		}

		log::info!(
			target: "evm-fee-token",
			"ERC20 fee payment success: user {:?}, token {:?}, amount {:?}",
			who, token, token_amount
		);

		// Emit event
		Pallet::<T>::deposit_event(crate::Event::FeePaymentInToken {
			user: *who,
			token,
			amount: token_amount,
			native_equivalent: native_fee,
		});

		Ok(LiquidityInfo::ERC20Withdrawn {
			token,
			amount: token_amount,
			native_equivalent: native_fee,
		})
	}

	/// Correct native fee after execution.
	fn correct_native_fee(
		who: &H160,
		corrected_fee: U256,
		base_fee: U256,
		already_withdrawn: Option<NegativeImbalanceOf<T, C>>,
	) -> LiquidityInfo<T, C> {
		if let Some(paid) = already_withdrawn {
			let account_id = T::AddressMapping::into_account_id(*who);

			// Calculate refund amount
			let refund_amount = paid.peek().saturating_sub(corrected_fee.unique_saturated_into());

			// Refund to the account
			let refund_imbalance = C::deposit_into_existing(&account_id, refund_amount)
				.unwrap_or_else(|_| C::PositiveImbalance::zero());

			// Merge imbalances
			let adjusted_paid = paid
				.offset(refund_imbalance)
				.same()
				.unwrap_or_else(|_| C::NegativeImbalance::zero());

			// Split into base fee and tip
			let (base_fee_imbalance, tip) = adjusted_paid.split(base_fee.unique_saturated_into());

			// Handle base fee (burn/treasury)
			OU::on_unbalanced(base_fee_imbalance);

			return LiquidityInfo::Native(Some(tip));
		}

		LiquidityInfo::None
	}

	/// Correct ERC20 fee after execution.
	fn correct_erc20_fee(
		who: &H160,
		corrected_fee: U256,
		base_fee: U256,
		token: H160,
		withdrawn_amount: U256,
		native_equivalent: U256,
	) -> LiquidityInfo<T, C> {
		// Calculate the ratio of actual fee to initially estimated fee
		// corrected_fee / native_equivalent gives us the utilization ratio
		if native_equivalent.is_zero() {
			return LiquidityInfo::None;
		}

		// Calculate actual token amount needed
		// actual_token = withdrawn_amount * (corrected_fee / native_equivalent)
		let actual_token_amount = withdrawn_amount
			.checked_mul(corrected_fee)
			.and_then(|v| v.checked_div(native_equivalent))
			.unwrap_or(withdrawn_amount);

		// Calculate refund
		let refund_amount = withdrawn_amount.saturating_sub(actual_token_amount);

		// Refund excess tokens to user
		if !refund_amount.is_zero() {
			if Pallet::<T>::refund_token(*who, token, refund_amount).is_ok() {
				// Emit refund event
				Pallet::<T>::deposit_event(crate::Event::FeeRefundInToken {
					user: *who,
					token,
					amount: refund_amount,
				});
			}
		}

		// Calculate tip portion based on corrected_fee (not native_equivalent)
		// tip_ratio = (corrected_fee - base_fee) / corrected_fee
		// When max_priority_fee = 0, corrected_fee == base_fee, so tip = 0
		//
		// To avoid overflow, calculate as: tip = actual - (actual * base / corrected)
		// This is equivalent to: actual * (1 - base/corrected) = actual * (corrected - base) / corrected
		let tip_token_amount = if corrected_fee > base_fee && !corrected_fee.is_zero() {
			let base_portion = actual_token_amount
				.checked_mul(base_fee)
				.and_then(|v| v.checked_div(corrected_fee))
				.unwrap_or(actual_token_amount);
			actual_token_amount.saturating_sub(base_portion)
		} else {
			U256::zero()
		};

		log::debug!(
			target: "evm-fee-token",
			"correct_erc20_fee: corrected_fee={}, base_fee={}, actual_token={}, tip_token={}, refund={}",
			corrected_fee, base_fee, actual_token_amount, tip_token_amount, refund_amount
		);

		// Note: For ERC20, we don't burn - tokens stay in treasury
		// The treasury can periodically convert and burn/distribute

		// Return tip info for pay_priority_fee
		LiquidityInfo::ERC20Tip { token, tip_amount: tip_token_amount }
	}
}
