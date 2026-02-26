//! # EVM Fee Token Pallet
//!
//! This pallet enables ERC20 token payments for EVM transaction gas fees.
//!
//! ## Overview
//!
//! The pallet provides:
//! - Registry of accepted ERC20 tokens for fee payment
//! - User preference storage for fee token selection
//! - Oracle integration for price conversion (via oracle-registry)
//! - ERC20 transfer execution via pallet-evm Runner
//!
//! ## Architecture
//!
//! ```text
//! [User EVM Tx] → [OnChargeEVMTransaction] → [BifrostFeeAdapter]
//!                                                    ↓
//!                                           Check fee token preference
//!                                                    ↓
//!                                    ┌───────────────┴───────────────┐
//!                                    │                               │
//!                              [Native BFC]                   [ERC20 Token]
//!                                    │                               │
//!                              Currency::withdraw      Oracle price query
//!                                                     (via oracle-registry)
//!                                                                    │
//!                                                          Runner::call(ERC20)
//! ```

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]

pub mod adapter;
pub mod erc20;
pub mod migrations;
pub mod oracle;
pub mod types;
pub mod weights;

pub use adapter::BifrostFeeAdapter;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub use pallet::*;
pub use types::*;
pub use weights::WeightInfo;

use frame_support::{pallet_prelude::*, traits::{Hooks, OnRuntimeUpgrade}, weights::Weight};
use frame_system::pallet_prelude::*;
use sp_core::{H160, H256, U256};

/// The current storage version.
const STORAGE_VERSION: StorageVersion = StorageVersion::new(2);

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	#[pallet::without_storage_info]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			// Clear previous block's fee payment data at the start of new block.
			let _ = PendingFeePayments::<T>::clear(u32::MAX, None);
			// Reset transaction index counter for this block.
			CurrentTxIndex::<T>::kill();
			Weight::zero()
		}

		fn on_runtime_upgrade() -> Weight {
			crate::migrations::v1::MigrateToV2::<T>::on_runtime_upgrade()
		}
	}

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

		/// Cooldown period (in blocks) between fee token preference changes.
		/// Users must wait this many blocks before changing their fee token again.
		/// Set to 0 to disable rate limiting.
		#[pallet::constant]
		type FeeTokenUpdateCooldown: Get<BlockNumberFor<Self>>;

		/// Oracle registry for price lookups.
		/// Provides token → oracle ID mapping and oracle price queries.
		type OracleRegistry: bp_oracle::traits::OracleRegistryManager;

		/// Weight information for extrinsics.
		type WeightInfo: WeightInfo;
	}

	/// Accepted fee tokens with their configuration.
	/// Key: ERC20 token contract address
	/// Value: Token configuration
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

	/// BFC/USD Oracle ID for native token price conversion.
	/// When None, ERC20 fee payment is disabled (falls back to native).
	#[pallet::storage]
	#[pallet::getter(fn native_oracle_id)]
	pub type NativeOracleId<T: Config> = StorageValue<_, H256, OptionQuery>;

	/// Last block number when user updated their fee token preference.
	/// Used for rate limiting fee token changes.
	#[pallet::storage]
	#[pallet::getter(fn last_fee_token_update)]
	pub type LastFeeTokenUpdate<T: Config> =
		StorageMap<_, Blake2_128Concat, H160, BlockNumberFor<T>, OptionQuery>;

	/// Counter for tracking current transaction index within a block.
	/// This is incremented each time `withdraw_fee` is called (for any fee payment type).
	/// Cleared at the start of each block via `on_initialize`.
	#[pallet::storage]
	pub type CurrentTxIndex<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// Fee payment information indexed by user address and transaction index.
	///
	/// This storage is populated when a transaction pays fees in ERC20 tokens.
	/// Since `withdraw_fee` is called before the transaction hash is known,
	/// we index by user address and transaction index within the block.
	///
	/// The RPC layer queries this by (from_address, tx_index) to get fee payment info.
	/// This storage is cleared at the start of each block via `on_initialize`.
	#[pallet::storage]
	#[pallet::getter(fn pending_fee_payments)]
	pub type PendingFeePayments<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		H160, // user address (from)
		Blake2_128Concat,
		u32, // transaction index within block
		FeePaymentInfo,
		OptionQuery,
	>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new fee token has been added.
		FeeTokenAdded { token: H160, decimals: u8 },
		/// A fee token has been removed.
		FeeTokenRemoved { token: H160 },
		/// A fee token configuration has been updated.
		FeeTokenUpdated { token: H160 },
		/// User's fee token preference has been set.
		UserFeeTokenSet { user: H160, token: Option<H160> },
		/// Fee was paid in ERC20 token (withdrawn before execution).
		FeePaymentInToken { user: H160, token: H160, amount: U256, native_equivalent: U256 },
		/// Native token oracle ID has been set or cleared.
		NativeOracleSet { oracle_id: Option<H256> },
		/// Excess fee was refunded to user in ERC20 token.
		FeeRefundInToken { user: H160, token: H160, amount: U256 },
		/// Tip was paid to block author in ERC20 token.
		TipPaymentInToken { author: H160, token: H160, amount: U256 },
		/// Collected fees were withdrawn from fee collector.
		CollectedFeesWithdrawn { token: H160, to: H160, amount: U256 },
		/// ERC20 fee payment failed, fell back to native token.
		FeePaymentFallbackToNative { user: H160, token: H160, reason: FallbackReason },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Token is not in the accepted list.
		TokenNotAccepted,
		/// Token is already in the accepted list.
		TokenAlreadyAccepted,
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
		/// Rate limited: must wait before changing fee token again.
		RateLimited,
		/// Oracle price data is stale (updated_at too old).
		OraclePriceStale,
		/// No oracle registered for token in oracle-registry.
		OracleNotRegistered,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Add a new accepted fee token.
		///
		/// Only callable by AdminOrigin (governance).
		///
		/// Parameters:
		/// - `token`: ERC20 token contract address
		/// - `config`: Token configuration
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
			ensure!(config.decimals <= 18, Error::<T>::InvalidTokenConfig);

			AcceptedFeeTokens::<T>::insert(token, config.clone());

			Self::deposit_event(Event::FeeTokenAdded {
				token,
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
			ensure!(config.decimals <= 18, Error::<T>::InvalidTokenConfig);

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

		/// Set the BFC/USD oracle ID.
		///
		/// Only callable by AdminOrigin (sudo/governance).
		/// Pass `None` to disable ERC20 fee payment (all users fall back to native).
		///
		/// Parameters:
		/// - `oracle_id`: BFC/USD oracle ID (H256, None to disable)
		#[pallet::call_index(4)]
		#[pallet::weight(<T as Config>::WeightInfo::set_native_oracle())]
		pub fn set_native_oracle(
			origin: OriginFor<T>,
			oracle_id: Option<H256>,
		) -> DispatchResult {
			T::AdminOrigin::ensure_origin(origin)?;

			if let Some(id) = oracle_id {
				ensure!(id != H256::zero(), Error::<T>::InvalidTokenConfig);
				NativeOracleId::<T>::put(id);
			} else {
				NativeOracleId::<T>::kill();
			}

			Self::deposit_event(Event::NativeOracleSet { oracle_id });

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
					target: "bifrost-tx-payment",
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

		/// Check if ERC20 fee payment is enabled (native oracle ID is set).
		pub fn is_erc20_fee_enabled() -> bool {
			NativeOracleId::<T>::get().is_some()
		}

		/// Convert native fee amount to token amount.
		///
		/// Uses BFC/USD and Token/USD oracles (via oracle-registry) to calculate the conversion.
		/// All oracle prices use 18 decimals.
		///
		/// Formula:
		/// ```text
		/// token_amount = native_fee * bfc_usd_price * 10^token_decimals
		///                / (token_usd_price * 10^18)
		/// ```
		///
		/// Returns Err(NativeOracleNotSet) if BFC/USD oracle ID is not configured.
		/// Returns Err(OraclePriceStale) if the token's oracle price is stale.
		pub fn convert_native_to_token(native_fee: U256, token: H160) -> Result<U256, Error<T>> {
			Self::convert_native_to_token_with_prices(native_fee, token).map(|(amount, _, _)| amount)
		}

		/// Convert native fee amount to token amount and return oracle prices.
		///
		/// Same as `convert_native_to_token` but also returns the oracle prices used
		/// for the conversion, which can be included in fee payment receipts.
		///
		/// Returns: (token_amount, token_price, native_price)
		pub fn convert_native_to_token_with_prices(
			native_fee: U256,
			token: H160,
		) -> Result<(U256, U256, U256), Error<T>> {
			let config = AcceptedFeeTokens::<T>::get(token).ok_or(Error::<T>::TokenNotAccepted)?;
			ensure!(config.enabled, Error::<T>::TokenDisabled);

			// Get BFC/USD price (required for conversion) - no staleness check for native oracle
			let bfc_usd_price = Self::get_bfc_usd_price()?;

			// Get Token/USD price via oracle-registry with staleness check
			let token_usd_price =
				crate::oracle::get_token_price_via_registry::<T>(token, config.max_staleness_seconds)
					.map_err(|e| match e {
						crate::oracle::OracleError::StalePrice => Error::<T>::OraclePriceStale,
						crate::oracle::OracleError::OracleNotRegistered =>
							Error::<T>::OracleNotRegistered,
						_ => Error::<T>::OraclePriceFailed,
					})?;

			log::debug!(
				target: "bifrost-tx-payment",
				"Price conversion: native_fee={}, bfc_usd={}, token_usd={}, token_decimals={}, max_staleness={}s",
				native_fee, bfc_usd_price, token_usd_price, config.decimals, config.max_staleness_seconds
			);

			// Conversion formula (all oracle prices are 18 decimals):
			// token_amount = native_fee * bfc_usd_price * 10^token_decimals
			//                / (token_usd_price * 10^18)
			let token_amount = native_fee
				.checked_mul(bfc_usd_price)
				.ok_or(Error::<T>::PriceOverflow)?
				.checked_mul(U256::from(10u128.pow(config.decimals as u32)))
				.ok_or(Error::<T>::PriceOverflow)?
				.checked_div(token_usd_price)
				.ok_or(Error::<T>::PriceOverflow)?
				.checked_div(U256::from(10u128.pow(18u32)))
				.ok_or(Error::<T>::PriceOverflow)?;

			log::debug!(
				target: "bifrost-tx-payment",
				"Converted token_amount: {}",
				token_amount
			);

			Ok((token_amount, token_usd_price, bfc_usd_price))
		}

		/// Get BFC/USD price from oracle-registry using the stored native oracle ID.
		fn get_bfc_usd_price() -> Result<U256, Error<T>> {
			let oracle_id =
				NativeOracleId::<T>::get().ok_or(Error::<T>::NativeOracleNotSet)?;

			crate::oracle::get_oracle_price_from_registry::<T>(oracle_id, 0).map_err(|e| {
				log::error!(
					target: "bifrost-tx-payment",
					"BFC/USD oracle call failed: {:?}",
					e
				);
				Error::<T>::OraclePriceFailed
			})
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
