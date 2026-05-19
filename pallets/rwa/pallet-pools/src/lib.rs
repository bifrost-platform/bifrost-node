#![cfg_attr(not(feature = "std"), no_std)]

mod pallet;

pub use pallet::pallet::*;

use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::{H160, U256};
use sp_runtime::{traits::One, FixedU128, Perquintill, RuntimeDebug};

#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Primitive type aliases
// ---------------------------------------------------------------------------

/// 18-decimal fixed-point rate: stores `1 + rate_per_second`.
pub type Rate = FixedU128;

/// Pool identifier.
pub type PoolId = u64;

/// Tranche index within a pool. 0 = most senior, last = residual (junior).
pub type TrancheIndex = u32;

/// Epoch counter.
pub type EpochId = u32;

// ---------------------------------------------------------------------------
// TrancheId
// ---------------------------------------------------------------------------

/// Globally unique tranche identifier: the EVM chain where the vault is deployed
/// paired with the ERC-7540 vault contract address on that chain.
#[derive(
	Clone,
	Encode,
	Decode,
	DecodeWithMemTracking,
	PartialEq,
	Eq,
	RuntimeDebug,
	TypeInfo,
	MaxEncodedLen,
)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct TrancheId {
	/// EVM chain ID of the chain where the vault contract is deployed.
	pub chain_id: u64,
	/// ERC-7540 vault contract address on that chain.
	pub vault_address: H160,
}

// ---------------------------------------------------------------------------
// TrancheType
// ---------------------------------------------------------------------------

#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub enum TrancheType {
	/// Residual (junior) tranche — absorbs losses first, receives residual return.
	Junior,
	/// Senior tranche — protected by the junior buffer.
	/// `interest_rate_per_sec` and `min_risk_buffer` are stored for record-keeping
	/// and loss-waterfall scoring; no on-chain interest accrual is performed.
	/// Yield is distributed by the borrower funding the pool reserve via the repay flow.
	Senior {
		/// Agreed target yield, stored as `1 + annual_rate / SECONDS_PER_YEAR`.
		interest_rate_per_sec: Rate,
		/// Minimum junior-to-pool ratio required for a healthy epoch solution.
		min_risk_buffer: Perquintill,
	},
}

impl TrancheType {
	pub fn is_junior(&self) -> bool {
		matches!(self, TrancheType::Junior)
	}
}

// ---------------------------------------------------------------------------
// Tranche
// ---------------------------------------------------------------------------

/// All tranche pricing is oracle-driven (NAV / token supply).
/// There is no on-chain interest accrual; senior yield is distributed through
/// the borrower repay flow.
#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct Tranche {
	pub tranche_type: TrancheType,
	/// Globally unique tranche identifier: (chain_id, vault_address).
	pub tranche_id: TrancheId,
	/// Outstanding ERC-1404 token supply for this tranche.
	pub total: U256,
	/// Seniority weight used in epoch solution scoring. 0 = most senior.
	pub seniority: u32,
}

impl Tranche {
	/// Token price = tranche_nav / token_supply.
	/// `nav` is this tranche's share of the pool NAV, provided by pallet-nav-oracle.
	/// Returns ONE when no tokens are outstanding.
	pub fn token_price(&self, nav: U256) -> FixedU128 {
		let nav: u128 = nav.try_into().unwrap_or(u128::MAX);
		let supply: u128 = self.total.try_into().unwrap_or(u128::MAX);
		if supply == 0 {
			return FixedU128::one();
		}
		FixedU128::from_rational(nav, supply)
	}
}

// ---------------------------------------------------------------------------
// TrancheInput — used when creating a pool
// ---------------------------------------------------------------------------

#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct TrancheInput {
	pub tranche_type: TrancheType,
	/// Globally unique tranche identifier: (chain_id, vault_address).
	pub tranche_id: TrancheId,
	/// Seniority weight for scoring. 0 = most senior.
	pub seniority: u32,
}

// ---------------------------------------------------------------------------
// SettlementMode
// ---------------------------------------------------------------------------

#[derive(
	Clone,
	Copy,
	Encode,
	Decode,
	DecodeWithMemTracking,
	PartialEq,
	Eq,
	RuntimeDebug,
	TypeInfo,
	MaxEncodedLen,
)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub enum SettlementMode {
	/// Orders settle automatically via `on_initialize` when the epoch ends.
	Automatic,
	/// Orders are frozen during the settlement window; the pool admin or borrower
	/// must explicitly call `approve_invest_orders` / `approve_redeem_orders`.
	Approval,
}

// ---------------------------------------------------------------------------
// CollateralAsset — NFT representing the off-chain RWA
// ---------------------------------------------------------------------------

#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct CollateralAsset {
	/// ERC-721 / ERC-1155 contract address on Bifrost EVM.
	pub nft_contract: H160,
	/// Token ID identifying the specific NFT.
	pub nft_token_id: U256,
}

// ---------------------------------------------------------------------------
// ReserveDetails
// ---------------------------------------------------------------------------

#[derive(Clone, Default, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct ReserveDetails {
	/// Admin-configured maximum reserve (total reserve cannot exceed this amount).
	pub max: U256,
	/// Total reserve balance.
	pub total: U256,
	/// Available reserve for borrowers.
	pub available: U256,
}

impl ReserveDetails {
	pub fn deposit(&mut self, amount: U256) {
		self.total = self.total.saturating_add(amount);
		self.available = self.available.saturating_add(amount);
	}

	/// Returns false if insufficient available reserve.
	pub fn withdraw(&mut self, amount: U256) -> bool {
		if self.available < amount {
			return false;
		}
		self.available = self.available.saturating_sub(amount);
		self.total = self.total.saturating_sub(amount);
		true
	}
}

// ---------------------------------------------------------------------------
// EpochInfo — block-number-based epoch tracking with settlement window
// ---------------------------------------------------------------------------

#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct EpochInfo {
	/// Current epoch index.
	pub current_epoch: EpochId,
	/// Block number when the current epoch started.
	pub epoch_start_block: u32,
	/// Number of blocks each epoch lasts.
	pub epoch_length: u32,
	/// Number of blocks before epoch end when the settlement window opens.
	/// During [settlement_start_block, epoch_end), new orders are rejected and
	/// confirmed orders can be executed.
	pub settlement_start_offset: u32,
}

impl EpochInfo {
	pub fn new(epoch_length: u32, settlement_start_offset: u32, start_block: u32) -> Self {
		EpochInfo {
			current_epoch: 0,
			epoch_start_block: start_block,
			epoch_length,
			settlement_start_offset,
		}
	}

	/// True when `now` has reached or passed the end of the current epoch.
	pub fn should_advance(&self, now: u32) -> bool {
		now.saturating_sub(self.epoch_start_block) >= self.epoch_length
	}

	/// Advance to the next epoch starting at `now`.
	pub fn advance(&mut self, now: u32) {
		self.current_epoch = self.current_epoch.saturating_add(1);
		self.epoch_start_block = now;
	}

	/// First block of the settlement window for the current epoch.
	pub fn settlement_start_block(&self) -> u32 {
		self.epoch_start_block
			.saturating_add(self.epoch_length)
			.saturating_sub(self.settlement_start_offset)
	}

	/// True when `now` is inside the settlement window: new orders are frozen,
	/// confirmed orders may be executed.
	pub fn in_settlement_window(&self, now: u32) -> bool {
		!self.should_advance(now) && now >= self.settlement_start_block()
	}
}

// ---------------------------------------------------------------------------
// PoolDetails
// ---------------------------------------------------------------------------

#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct PoolDetails<AccountId> {
	/// Pool admin account.
	pub admin: AccountId,
	/// Accepted currency contract address on Bifrost EVM (e.g. UnifiedUSDC).
	pub currency: H160,
	/// Reserve details.
	pub reserve: ReserveDetails,
	/// Ordered tranches: index 0 = most senior, last = junior.
	pub tranches: sp_std::vec::Vec<Tranche>,
	/// Block-number-based epoch tracking.
	pub epoch: EpochInfo,
	/// NFT collateral representing the off-chain RWA.
	pub collateral: CollateralAsset,
	/// Maximum total investment the pool will accept.
	pub investment_ceiling: U256,
	/// Settlement mode for invest orders.
	pub invest_settlement: SettlementMode,
	/// Settlement mode for redeem orders.
	pub redeem_settlement: SettlementMode,
}

// ---------------------------------------------------------------------------
// Traits
// ---------------------------------------------------------------------------

use frame_support::pallet_prelude::DispatchError;

/// Implemented by pallet-pools. Called by pallet-investments and the gateway
/// to validate pool/tranche existence, resolve pool admin, and query epoch state.
pub trait PoolInspect<AccountId> {
	fn pool_exists(pool_id: PoolId) -> bool;
	fn pool_admin(pool_id: PoolId) -> Option<AccountId>;
	fn tranche_exists(pool_id: PoolId, tranche_id: TrancheId) -> bool;
	/// True when the pool is currently inside its settlement window.
	fn in_settlement_window(pool_id: PoolId) -> bool;
}

/// Defined here, implemented by pallet-investments.
/// Called from pallet-pools' `on_initialize` during automatic invest settlement.
pub trait InvestmentSettlement<PoolId, TrancheId, Balance> {
	/// Pro-rata confirm pending invest orders for a tranche up to `max_amount` USDC.
	///
	/// If total pending <= `max_amount`, all orders are confirmed in full.
	/// If total pending > `max_amount`, each investor's order is scaled down proportionally
	/// and the remainder stays in `PendingInvestOrders` for the next epoch.
	///
	/// Returns the actual USDC amount moved to `ConfirmedInvestOrders`.
	fn settle_invest_orders(pool_id: PoolId, tranche_id: TrancheId, max_amount: Balance)
		-> Balance;
}

/// Implemented by pallet-nav-oracle. Called by pallet-pools to fetch the current
/// NAV (net asset value = total collateral AUM) for a pool.
pub trait PoolNAV<PoolId, Balance> {
	/// Returns `(nav, block_number)` of the last recorded NAV without recomputing.
	fn nav(pool_id: PoolId) -> Option<(Balance, u32)>;

	/// Triggers a fresh NAV report and returns the result.
	fn update_nav(pool_id: PoolId) -> Result<Balance, DispatchError>;
}

/// Implemented by pallet-pools. Called by the settlement layer when disbursing
/// or receiving payments so pool reserve accounting stays consistent.
pub trait PoolReserve<Balance> {
	/// Decrease available reserve by `amount` (borrower draw-down).
	/// Returns `Err` if insufficient available reserve.
	fn withdraw(pool_id: PoolId, amount: Balance) -> frame_support::dispatch::DispatchResult;

	/// Increase total and available reserve by `amount` (repayment or invest settlement).
	fn deposit(pool_id: PoolId, amount: Balance) -> frame_support::dispatch::DispatchResult;

	/// Read available reserve for a pool.
	fn available_reserve(pool_id: PoolId) -> Balance;
}
