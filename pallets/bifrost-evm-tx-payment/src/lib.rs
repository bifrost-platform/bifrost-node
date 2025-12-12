//! # EVM Fee Token Pallet
//!
//! This pallet enables ERC20 token payments for EVM transaction gas fees.
//!
//! ## Overview
//!
//! The pallet provides:
//! - Registry of accepted ERC20 tokens for fee payment
//! - User preference storage for fee token selection
//! - Oracle integration for price conversion (Chainlink-style)
//! - ERC20 transfer execution via pallet-evm Runner
//!
//! ## Architecture
//!
//! ```text
//! [User EVM Tx] → [OnChargeEVMTransaction] → [ERC20FeeAdapter]
//!                                                    ↓
//!                                           Check fee token preference
//!                                                    ↓
//!                                    ┌───────────────┴───────────────┐
//!                                    │                               │
//!                              [Native BFC]                   [ERC20 Token]
//!                                    │                               │
//!                              Currency::withdraw          Oracle price query
//!                                                                    │
//!                                                          Runner::call(ERC20)
//! ```

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]

pub mod adapter;
pub mod erc20;
pub mod oracle;
pub mod types;
pub mod weights;

pub use adapter::ERC20FeeAdapter;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub use pallet::*;
pub use types::*;
pub use weights::WeightInfo;

use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;
use sp_core::{H160, U256};

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_evm::Config {
		/// Origin that can manage accepted fee tokens (typically governance).
		type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Fee collector address to receive and hold fee tokens.
		/// Must be an address outside the precompile range (0x0800-0x0FFF) to allow
		/// outgoing EVM calls for refunds and tip payments.
		/// Recommended: derive from a PalletId using `into_account_truncating()`.
		#[pallet::constant]
		type FeeCollectorAddress: Get<H160>;

		/// Weight information for extrinsics.
		type WeightInfo: WeightInfo;
	}

	/// Accepted fee tokens with their configuration.
	/// Key: ERC20 token contract address
	/// Value: Token configuration including oracle address and parameters
	#[pallet::storage]
	#[pallet::getter(fn accepted_fee_tokens)]
	pub type AcceptedFeeTokens<T: Config> =
		StorageMap<_, Blake2_128Concat, H160, FeeTokenConfig, OptionQuery>;

	/// User's preferred fee token.
	/// Key: User's EVM address (H160)
	/// Value: ERC20 token address (None means use native token)
	#[pallet::storage]
	#[pallet::getter(fn user_fee_token)]
	pub type UserFeeToken<T: Config> = StorageMap<_, Blake2_128Concat, H160, H160, OptionQuery>;

	/// BFC/USD Oracle address for native token price conversion.
	/// When None, ERC20 fee payment is disabled (falls back to native).
	#[pallet::storage]
	#[pallet::getter(fn native_token_oracle)]
	pub type NativeTokenOracle<T: Config> = StorageValue<_, H160, OptionQuery>;

	/// Decimals of the native token oracle (e.g., 8 for Chainlink standard).
	#[pallet::storage]
	#[pallet::getter(fn native_oracle_decimals)]
	pub type NativeOracleDecimals<T: Config> = StorageValue<_, u8, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new fee token has been added.
		FeeTokenAdded { token: H160, oracle: H160, decimals: u8 },
		/// A fee token has been removed.
		FeeTokenRemoved { token: H160 },
		/// A fee token configuration has been updated.
		FeeTokenUpdated { token: H160 },
		/// User's fee token preference has been set.
		UserFeeTokenSet { user: H160, token: Option<H160> },
		/// Fee was paid in ERC20 token (withdrawn before execution).
		FeePaymentInToken { user: H160, token: H160, amount: U256, native_equivalent: U256 },
		/// Native token oracle has been set or cleared.
		NativeOracleSet { oracle: Option<H160> },
		/// Excess fee was refunded to user in ERC20 token.
		FeeRefundInToken { user: H160, token: H160, amount: U256 },
		/// Tip was paid to block author in ERC20 token.
		TipPaymentInToken { author: H160, token: H160, amount: U256 },
		/// Collected fees were withdrawn from fee collector.
		CollectedFeesWithdrawn { token: H160, to: H160, amount: U256 },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Token is not in the accepted list.
		TokenNotAccepted,
		/// Token is already in the accepted list.
		TokenAlreadyAccepted,
		/// Oracle address is invalid.
		InvalidOracle,
		/// Failed to get price from oracle.
		OraclePriceFailed,
		/// Price conversion overflow.
		PriceOverflow,
		/// ERC20 transfer failed.
		ERC20TransferFailed,
		/// Invalid token configuration.
		InvalidTokenConfig,
		/// Token is disabled.
		TokenDisabled,
		/// Native token oracle is not set (ERC20 fee payment disabled).
		NativeOracleNotSet,
		/// AccountId could not be converted to H160 (invalid format).
		InvalidAccountFormat,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Add a new accepted fee token.
		///
		/// Only callable by AdminOrigin (governance).
		///
		/// Parameters:
		/// - `token`: ERC20 token contract address
		/// - `config`: Token configuration including oracle address
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::add_fee_token())]
		pub fn add_fee_token(
			origin: OriginFor<T>,
			token: H160,
			config: FeeTokenConfig,
		) -> DispatchResult {
			T::AdminOrigin::ensure_origin(origin)?;

			ensure!(!AcceptedFeeTokens::<T>::contains_key(token), Error::<T>::TokenAlreadyAccepted);

			// Validate configuration
			ensure!(config.oracle_address != H160::zero(), Error::<T>::InvalidOracle);
			ensure!(config.decimals <= 18, Error::<T>::InvalidTokenConfig);
			ensure!(config.oracle_decimals <= 18, Error::<T>::InvalidTokenConfig);

			AcceptedFeeTokens::<T>::insert(token, config.clone());

			Self::deposit_event(Event::FeeTokenAdded {
				token,
				oracle: config.oracle_address,
				decimals: config.decimals,
			});

			Ok(())
		}

		/// Remove an accepted fee token.
		///
		/// Only callable by AdminOrigin (governance).
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::remove_fee_token())]
		pub fn remove_fee_token(origin: OriginFor<T>, token: H160) -> DispatchResult {
			T::AdminOrigin::ensure_origin(origin)?;

			ensure!(AcceptedFeeTokens::<T>::contains_key(token), Error::<T>::TokenNotAccepted);

			AcceptedFeeTokens::<T>::remove(token);

			Self::deposit_event(Event::FeeTokenRemoved { token });

			Ok(())
		}

		/// Update fee token configuration.
		///
		/// Only callable by AdminOrigin (governance).
		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::update_fee_token())]
		pub fn update_fee_token(
			origin: OriginFor<T>,
			token: H160,
			config: FeeTokenConfig,
		) -> DispatchResult {
			T::AdminOrigin::ensure_origin(origin)?;

			ensure!(AcceptedFeeTokens::<T>::contains_key(token), Error::<T>::TokenNotAccepted);
			ensure!(config.oracle_address != H160::zero(), Error::<T>::InvalidOracle);
			ensure!(config.decimals <= 18, Error::<T>::InvalidTokenConfig);
			ensure!(config.oracle_decimals <= 18, Error::<T>::InvalidTokenConfig);

			AcceptedFeeTokens::<T>::insert(token, config);

			Self::deposit_event(Event::FeeTokenUpdated { token });

			Ok(())
		}

		/// Set user's preferred fee token.
		///
		/// Callable by any user for their own address.
		/// Pass `None` to use native token (BFC).
		#[pallet::call_index(3)]
		#[pallet::weight(<T as Config>::WeightInfo::set_user_fee_token())]
		pub fn set_user_fee_token(origin: OriginFor<T>, token: Option<H160>) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let user_address = Self::account_to_h160(&who)?;

			if let Some(t) = token {
				let config = AcceptedFeeTokens::<T>::get(t).ok_or(Error::<T>::TokenNotAccepted)?;
				ensure!(config.enabled, Error::<T>::TokenDisabled);
				UserFeeToken::<T>::insert(user_address, t);
			} else {
				UserFeeToken::<T>::remove(user_address);
			}

			Self::deposit_event(Event::UserFeeTokenSet { user: user_address, token });

			Ok(())
		}

		/// Set the BFC/USD oracle address and decimals.
		///
		/// Only callable by AdminOrigin (sudo/governance).
		/// Pass `None` for oracle to disable ERC20 fee payment (all users fall back to native).
		///
		/// Parameters:
		/// - `oracle`: BFC/USD oracle address (None to disable)
		/// - `decimals`: Oracle price decimals (e.g., 8 for Chainlink standard)
		#[pallet::call_index(4)]
		#[pallet::weight(<T as Config>::WeightInfo::set_native_oracle())]
		pub fn set_native_oracle(
			origin: OriginFor<T>,
			oracle: Option<H160>,
			decimals: u8,
		) -> DispatchResult {
			T::AdminOrigin::ensure_origin(origin)?;

			ensure!(decimals <= 18, Error::<T>::InvalidTokenConfig);

			if let Some(addr) = oracle {
				ensure!(addr != H160::zero(), Error::<T>::InvalidOracle);
				NativeTokenOracle::<T>::put(addr);
				NativeOracleDecimals::<T>::put(decimals);
			} else {
				NativeTokenOracle::<T>::kill();
				NativeOracleDecimals::<T>::kill();
			}

			Self::deposit_event(Event::NativeOracleSet { oracle });

			Ok(())
		}

		/// Withdraw collected fee tokens from the fee collector address.
		///
		/// Only callable by AdminOrigin (sudo/governance).
		/// This allows withdrawing ERC20 tokens that have been collected as gas fees
		/// from the fee collector (precompile) address to a specified destination.
		///
		/// Parameters:
		/// - `token`: ERC20 token contract address to withdraw
		/// - `to`: Destination address to receive the tokens
		/// - `amount`: Amount of tokens to withdraw
		#[pallet::call_index(5)]
		#[pallet::weight(<T as Config>::WeightInfo::withdraw_collected_fees())]
		pub fn withdraw_collected_fees(
			origin: OriginFor<T>,
			token: H160,
			to: H160,
			amount: U256,
		) -> DispatchResult {
			T::AdminOrigin::ensure_origin(origin)?;

			ensure!(!amount.is_zero(), Error::<T>::InvalidTokenConfig);
			ensure!(to != H160::zero(), Error::<T>::InvalidTokenConfig);

			// Transfer from fee collector to destination
			let fee_collector = T::FeeCollectorAddress::get();
			crate::erc20::transfer_to_user::<T>(fee_collector, token, to, amount)
				.map_err(|_| Error::<T>::ERC20TransferFailed)?;

			Self::deposit_event(Event::CollectedFeesWithdrawn { token, to, amount });

			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Convert AccountId to H160 address.
		///
		/// For Bifrost (AccountId20), AccountId is already H160-compatible (20 bytes).
		/// Returns an error if the AccountId encoding is less than 20 bytes.
		pub fn account_to_h160(account: &T::AccountId) -> Result<H160, Error<T>> {
			use parity_scale_codec::Encode;
			let account_bytes = account.encode();
			if account_bytes.len() >= 20 {
				Ok(H160::from_slice(&account_bytes[0..20]))
			} else {
				log::error!(
					target: "evm-fee-token",
					"account_to_h160: AccountId encoding too short ({} bytes, expected >= 20)",
					account_bytes.len()
				);
				Err(Error::<T>::InvalidAccountFormat)
			}
		}

		/// Get the fee token for a user (None means native token).
		pub fn get_user_fee_token(user: &H160) -> Option<H160> {
			UserFeeToken::<T>::get(user)
		}

		/// Check if a token is accepted for fee payment.
		pub fn is_token_accepted(token: &H160) -> bool {
			AcceptedFeeTokens::<T>::get(token).map(|c| c.enabled).unwrap_or(false)
		}

		/// Get token configuration.
		pub fn get_token_config(token: &H160) -> Option<FeeTokenConfig> {
			AcceptedFeeTokens::<T>::get(token)
		}

		/// Check if ERC20 fee payment is enabled (native oracle is set).
		pub fn is_erc20_fee_enabled() -> bool {
			NativeTokenOracle::<T>::get().is_some()
		}

		/// Convert native fee amount to token amount.
		///
		/// Uses BFC/USD and Token/USD oracles to calculate the conversion.
		///
		/// Formula (with oracle decimals):
		/// ```text
		/// token_amount = native_fee * bfc_usd_price * 10^token_decimals * 10^token_oracle_decimals
		///                / (token_usd_price * 10^18 * 10^native_oracle_decimals)
		/// ```
		///
		/// Returns Err(NativeOracleNotSet) if BFC/USD oracle is not configured.
		pub fn convert_native_to_token(native_fee: U256, token: H160) -> Result<U256, Error<T>> {
			let config = AcceptedFeeTokens::<T>::get(token).ok_or(Error::<T>::TokenNotAccepted)?;
			ensure!(config.enabled, Error::<T>::TokenDisabled);

			// Get BFC/USD price (required for conversion)
			let bfc_usd_price = Self::get_bfc_usd_price()?;
			let native_oracle_decimals = NativeOracleDecimals::<T>::get();

			// Get Token/USD price
			let token_usd_price = Self::get_oracle_price(config.oracle_address)?;
			let token_oracle_decimals = config.oracle_decimals;

			log::debug!(
				target: "evm-fee-token",
				"Price conversion: native_fee={}, bfc_usd={} (dec={}), token_usd={} (dec={}), token_decimals={}",
				native_fee, bfc_usd_price, native_oracle_decimals, token_usd_price, token_oracle_decimals, config.decimals
			);

			// Conversion formula:
			// token_amount = native_fee * bfc_usd_price * 10^token_decimals * 10^token_oracle_decimals
			//                / (token_usd_price * 10^18 * 10^native_oracle_decimals)
			//
			// Simplify: multiply numerator, then divide by denominator
			// Numerator: native_fee * bfc_usd_price * 10^(token_decimals + token_oracle_decimals)
			// Denominator: token_usd_price * 10^(18 + native_oracle_decimals)

			let numerator_decimal_exp = config.decimals as u32 + token_oracle_decimals as u32;
			let denominator_decimal_exp = 18u32 + native_oracle_decimals as u32;

			let token_amount = native_fee
				.checked_mul(bfc_usd_price)
				.ok_or(Error::<T>::PriceOverflow)?
				.checked_mul(U256::from(10u128.pow(numerator_decimal_exp)))
				.ok_or(Error::<T>::PriceOverflow)?
				.checked_div(token_usd_price)
				.ok_or(Error::<T>::PriceOverflow)?
				.checked_div(U256::from(10u128.pow(denominator_decimal_exp)))
				.ok_or(Error::<T>::PriceOverflow)?;

			log::debug!(
				target: "evm-fee-token",
				"Converted token_amount: {}",
				token_amount
			);

			Ok(token_amount)
		}

		/// Get BFC/USD price from the native token oracle.
		fn get_bfc_usd_price() -> Result<U256, Error<T>> {
			let oracle_address =
				NativeTokenOracle::<T>::get().ok_or(Error::<T>::NativeOracleNotSet)?;

			Self::get_oracle_price(oracle_address)
		}

		/// Get price from a Chainlink-style oracle (decimal 8).
		fn get_oracle_price(oracle_address: H160) -> Result<U256, Error<T>> {
			log::debug!(
				target: "evm-fee-token",
				"Fetching price from oracle {:?}",
				oracle_address
			);

			match crate::oracle::get_oracle_price::<T>(oracle_address) {
				Ok(price) => {
					log::debug!(
						target: "evm-fee-token",
						"Oracle price fetched: {}",
						price
					);
					Ok(price)
				},
				Err(_) => {
					log::error!(
						target: "evm-fee-token",
						"Oracle call failed for oracle {:?}",
						oracle_address
					);
					Err(Error::<T>::OraclePriceFailed)
				},
			}
		}

		/// Execute ERC20 transfer for fee payment.
		///
		/// User transfers tokens to fee collector (precompile address).
		/// This calls the ERC20 contract via pallet-evm Runner with user as msg.sender.
		pub fn execute_fee_transfer(from: H160, token: H160, amount: U256) -> Result<(), Error<T>> {
			let fee_collector = T::FeeCollectorAddress::get();
			crate::erc20::transfer_from_user::<T>(from, token, fee_collector, amount)
				.map_err(|_| Error::<T>::ERC20TransferFailed)
		}

		/// Refund excess ERC20 tokens from fee collector to user.
		pub fn refund_token(to: H160, token: H160, amount: U256) -> Result<(), Error<T>> {
			if amount.is_zero() {
				return Ok(());
			}

			let fee_collector = T::FeeCollectorAddress::get();
			crate::erc20::transfer_to_user::<T>(fee_collector, token, to, amount)
				.map_err(|_| Error::<T>::ERC20TransferFailed)
		}

		/// Get the fee collector address (precompile address).
		pub fn fee_collector_address() -> H160 {
			T::FeeCollectorAddress::get()
		}
	}
}
