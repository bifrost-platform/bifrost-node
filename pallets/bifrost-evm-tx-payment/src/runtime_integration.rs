//! Runtime Integration Guide
//!
//! This file provides example configurations for integrating the EVM Fee Token
//! pallet into a Bifrost runtime.
//!
//! ## Step 1: Add to Cargo.toml
//!
//! ```toml
//! # In runtime/{network}/Cargo.toml
//! [dependencies]
//! pallet-bifrost-evm-tx-payment = { path = "../../pallets/bifrost-evm-tx-payment", default-features = false }
//! precompile-bifrost-evm-tx-payment = { path = "../../precompiles/bifrost-evm-tx-payment", default-features = false }
//!
//! [features]
//! std = [
//!     # ... existing features ...
//!     "pallet-bifrost-evm-tx-payment/std",
//!     "precompile-bifrost-evm-tx-payment/std",
//! ]
//! ```
//!
//! ## Step 2: Configure the Pallet
//!
//! Add the following to `runtime/{network}/src/lib.rs`:
//!
//! ```rust,ignore
//! // ============================================================================
//! // EVM Fee Token Configuration
//! // ============================================================================
//!
//! parameter_types! {
//!     /// Fee collector address (precompile address) to receive and hold fee tokens.
//!     /// This should match the precompile address (0x0810).
//!     pub FeeCollectorEVMAddress: H160 = H160::from_low_u64_be(0x0810);
//!
//!     /// Maximum number of accepted fee tokens
//!     pub const MaxAcceptedFeeTokens: u32 = 20;
//!
//!     /// Gas limit for ERC20 transfer calls
//!     pub const ERC20TransferGasLimit: u64 = 100_000;
//! }
//!
//! impl pallet_bifrost_evm_tx_payment::Config for Runtime {
//!     type RuntimeEvent = RuntimeEvent;
//!     type AdminOrigin = EnsureRoot<AccountId>;
//!     type FeeCollectorAddress = FeeCollectorEVMAddress;
//!     type MaxAcceptedTokens = MaxAcceptedFeeTokens;
//!     type ERC20TransferGasLimit = ERC20TransferGasLimit;
//!     type WeightInfo = pallet_bifrost_evm_tx_payment::weights::SubstrateWeight<Runtime>;
//! }
//! ```
//!
//! ## Step 3: Update EVM Configuration
//!
//! Replace the existing `OnChargeTransaction` with `ERC20FeeAdapter`:
//!
//! ```rust,ignore
//! use pallet_bifrost_evm_tx_payment::ERC20FeeAdapter;
//!
//! impl pallet_evm::Config for Runtime {
//!     // ... existing config ...
//!
//!     // Replace EVMCurrencyAdapter with ERC20FeeAdapter
//!     type OnChargeTransaction = ERC20FeeAdapter<
//!         Self,
//!         Balances,
//!         DealWithFees<Runtime>,
//!     >;
//!
//!     // ... rest of config ...
//! }
//! ```
//!
//! ## Step 4: Add to construct_runtime!
//!
//! ```rust,ignore
//! construct_runtime!(
//!     pub enum Runtime {
//!         // ... existing pallets ...
//!
//!         // Add after pallet-evm
//!         EVMFeeToken: pallet_bifrost_evm_tx_payment = 60,
//!     }
//! );
//! ```
//!
//! ## Step 5: Add Precompile
//!
//! In `runtime/{network}/src/precompiles.rs`:
//!
//! ```rust,ignore
//! use precompile_bifrost_evm_tx_payment::EVMFeeTokenPrecompile;
//!
//! // Add to precompile set at address 0x0810
//! pub type BifrostPrecompiles<R> = PrecompileSetBuilder<
//!     R,
//!     (
//!         // ... existing precompiles ...
//!
//!         // EVM Fee Token Precompile
//!         PrecompileAt<
//!             AddressU64<0x0810>,
//!             EVMFeeTokenPrecompile<R>,
//!         >,
//!     ),
//! >;
//! ```
//!
//! ## Step 6: Genesis Configuration (Optional)
//!
//! To add accepted fee tokens at genesis:
//!
//! ```rust,ignore
//! // In node/{network}/src/chain_spec.rs
//!
//! fn genesis_config() -> RuntimeGenesisConfig {
//!     RuntimeGenesisConfig {
//!         // ... other configs ...
//!
//!         evm_fee_token: pallet_bifrost_evm_tx_payment::GenesisConfig {
//!             accepted_tokens: vec![
//!                 // USDC example
//!                 (
//!                     H160::from_str("0xUSDC_ADDRESS").unwrap(),
//!                     pallet_bifrost_evm_tx_payment::FeeTokenConfig {
//!                         enabled: true,
//!                         oracle_address: H160::from_str("0xUSDC_USD_ORACLE").unwrap(),
//!                         decimals: 6,            // USDC token decimals
//!                         oracle_decimals: 8,     // Chainlink standard decimals
//!                     },
//!                 ),
//!             ],
//!             ..Default::default()
//!         },
//!     }
//! }
//! ```
//!
//! ## Usage Example (Solidity)
//!
//! ```solidity
//! // SPDX-License-Identifier: MIT
//! pragma solidity ^0.8.0;
//!
//! interface IEVMFeeToken {
//!     function setUserFeeToken(address token) external;
//!     function clearUserFeeToken() external;
//!     function getUserFeeToken(address user) external view returns (address);
//!     function estimateFeeInToken(address token, uint256 gasAmount) external view returns (uint256);
//!     function isAcceptedToken(address token) external view returns (bool);
//! }
//!
//! contract FeeTokenUser {
//!     IEVMFeeToken constant FEE_TOKEN = IEVMFeeToken(0x0000000000000000000000000000000000000810);
//!     IERC20 constant USDC = IERC20(0x...); // Your USDC address
//!
//!     function setupUSDCPayment() external {
//!         // Just set USDC as your fee token - no approval needed!
//!         // The system transfers tokens directly from your account.
//!         FEE_TOKEN.setUserFeeToken(address(USDC));
//!     }
//!
//!     function checkFeeEstimate(uint256 gasAmount) external view returns (uint256) {
//!         return FEE_TOKEN.estimateFeeInToken(address(USDC), gasAmount);
//!     }
//!
//!     function switchToNativeToken() external {
//!         FEE_TOKEN.clearUserFeeToken();
//!     }
//! }
//! ```

// This module is for documentation only
