#![cfg_attr(not(feature = "std"), no_std)]

mod pallet;

pub use pallet::pallet::*;

use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::{ConstU32, H160, U256};
use sp_runtime::{traits::One, BoundedBTreeMap, BoundedVec, FixedU128, Perquintill, RuntimeDebug};

// ---------------------------------------------------------------------------
// Primitive type aliases
// ---------------------------------------------------------------------------

/// 18-decimal fixed-point rate: stores `1 + rate_per_second`.
pub type Rate = FixedU128;

/// Pool identifier.
pub type PoolId = u64;

/// Epoch counter.
pub type EpochId = u32;

/// Maximum number of tranches per pool.
pub const MAX_TRANCHES: u32 = 10;

/// Maximum number of collateral NFTs per pool.
pub const MAX_COLLATERALS: u32 = 10;

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
	Ord,
	PartialOrd,
	RuntimeDebug,
	TypeInfo,
	MaxEncodedLen,
)]
pub struct TrancheId {
	/// EVM chain ID of the chain where the vault contract is deployed.
	pub chain_id: u64,
	/// ERC-7540 vault contract address on that chain.
	pub vault_address: H160,
}

// ---------------------------------------------------------------------------
// TrancheType
// ---------------------------------------------------------------------------

#[derive(
	Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen, DecodeWithMemTracking,
)]
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

// ---------------------------------------------------------------------------
// TranchePendingOrders — aggregate epoch order totals
// ---------------------------------------------------------------------------

/// Aggregate pending order totals for a tranche in the current epoch.
/// Kept as a sub-struct so it can be zeroed atomically on epoch advance.
#[derive(
	Clone,
	Default,
	Encode,
	Decode,
	DecodeWithMemTracking,
	PartialEq,
	RuntimeDebug,
	TypeInfo,
	MaxEncodedLen,
)]
pub struct TranchePendingOrders {
	/// Total USDC waiting to be deposited (confirmed on epoch close).
	pub deposit: U256,
	/// Total tranche tokens waiting to be redeemed (confirmed on epoch close).
	pub redeem: U256,
}

// ---------------------------------------------------------------------------
// Tranche
// ---------------------------------------------------------------------------

/// All tranche pricing is oracle-driven (NAV / token supply).
/// There is no on-chain interest accrual; senior yield is distributed through
/// the borrower repay flow.
#[derive(
	Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen, DecodeWithMemTracking,
)]
pub struct Tranche {
	pub tranche_type: TrancheType,
	/// Maximum USDC that can be deposited into this tranche (cumulative cap).
	/// None means uncapped.
	pub max_deposits: Option<U256>,
	/// Number of tranche tokens currently outstanding (minted − burned).
	pub token_supply: U256,
	/// The total amount of USDC invested into the tranche (cumulative inflow).
	/// The available amount to redeem or borrow from the tranche treasury will be (invested - borrowed).
	pub invested: U256,
	/// The amount of USDC borrowed from the tranche treasury.
	pub borrowed: U256,
	/// Aggregate pending invest/redeem orders for the current epoch.
	pub pending_orders: TranchePendingOrders,
	/// Token price locked at the start of the settlement window, derived from the finalized NAV.
	/// None outside the settlement window. Reset to None when the epoch advances.
	pub epoch_price: Option<FixedU128>,
}

impl Tranche {
	/// Token price = (NAV + treasury_liquidity) / token_supply.
	///
	/// `nav` is this tranche's share of the pool NAV (off-chain RWA value),
	/// provided by pallet-nav-oracle. Adding `treasury_liquidity` accounts for
	/// USDC deposited but not yet borrowed, preventing dilution when new investors
	/// join before the borrower deploys the funds.
	/// Returns ONE when no tokens are outstanding.
	pub fn token_price(&self, nav: U256) -> FixedU128 {
		let supply: u128 = self.token_supply.try_into().unwrap_or(u128::MAX);
		if supply == 0 {
			return FixedU128::one();
		}
		let total_value: u128 =
			nav.saturating_add(self.treasury_liquidity()).try_into().unwrap_or(u128::MAX);
		FixedU128::from_rational(total_value, supply)
	}

	pub fn treasury_liquidity(&self) -> U256 {
		self.invested.saturating_sub(self.borrowed)
	}
}

// ---------------------------------------------------------------------------
// TrancheInput — used when creating a pool
// ---------------------------------------------------------------------------

#[derive(
	Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen, DecodeWithMemTracking,
)]
pub struct TrancheInput {
	/// Tranche type.
	pub tranche_type: TrancheType,
	/// Globally unique tranche identifier: (chain_id, vault_address).
	pub tranche_id: TrancheId,
	/// Maximum USDC that can be deposited into this tranche. None means uncapped.
	pub max_deposits: Option<U256>,
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
pub enum SettlementMode {
	/// Orders settle automatically via `on_initialize` when the epoch ends.
	Automatic,
	/// Orders are frozen during the settlement window; the pool admin or borrower
	/// must explicitly call `approve_deposit_order` / `approve_redeem_order`.
	Approval,
}

// ---------------------------------------------------------------------------
// CollateralAsset — NFT representing the off-chain RWA
// ---------------------------------------------------------------------------

#[derive(
	Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen, DecodeWithMemTracking,
)]
pub struct CollateralAsset {
	/// ERC-721 / ERC-1155 contract address on Bifrost EVM.
	pub nft_contract: H160,
	/// Token ID identifying the specific NFT.
	pub nft_token_id: U256,
}

// ---------------------------------------------------------------------------
// EpochInfo — block-number-based epoch tracking with settlement window
// ---------------------------------------------------------------------------

#[derive(
	Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen, DecodeWithMemTracking,
)]
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
	pub settlement_offset: u32,
}

impl EpochInfo {
	pub fn new(epoch_length: u32, settlement_offset: u32, start_block: u32) -> Self {
		EpochInfo {
			current_epoch: 0,
			epoch_start_block: start_block,
			epoch_length,
			settlement_offset,
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
			.saturating_sub(self.settlement_offset)
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

#[derive(
	Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen, DecodeWithMemTracking,
)]
pub struct PoolDetails<AccountId> {
	/// The institution (borrower) EOA authorized to borrow, repay, and approve orders.
	pub borrower: AccountId,
	/// Total amount of USDC invested into the pool. (= sum of all tranches' invested)
	pub total: U256,
	/// Mapped tranche IDs to tranches.
	pub tranches: BoundedBTreeMap<TrancheId, Tranche, ConstU32<MAX_TRANCHES>>,
	/// Block-number-based epoch tracking.
	pub epoch: EpochInfo,
	/// NFT collaterals representing the off-chain RWA. At least one required.
	pub collaterals: BoundedVec<CollateralAsset, ConstU32<MAX_COLLATERALS>>,
	/// Settlement mode for deposit orders.
	pub deposit_settlement: SettlementMode,
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
	/// Returns the borrower (institution EOA) authorized for this pool.
	fn pool_borrower(pool_id: PoolId) -> Option<AccountId>;
	fn tranche_exists(pool_id: PoolId, tranche_id: TrancheId) -> bool;
	/// True when the pool is currently inside its settlement window.
	fn in_settlement_window(pool_id: PoolId) -> bool;
	/// True when adding `amount` USDC would push `invested + pending_deposit` above
	/// the tranche's `max_deposits` cap. Always false if the tranche is uncapped.
	fn deposit_cap_exceeded(pool_id: PoolId, tranche_id: TrancheId, amount: U256) -> bool;
	/// Returns `invested - borrowed` for the tranche — the USDC available to cover redemptions.
	fn treasury_liquidity(pool_id: PoolId, tranche_id: TrancheId) -> U256;
	/// Returns the token price locked at settlement start for this tranche, if finalized.
	fn epoch_price(pool_id: PoolId, tranche_id: TrancheId) -> Option<FixedU128>;
}

/// Defined here, implemented by pallet-investments.
/// Called from pallet-pools' `on_initialize` during automatic deposit settlement.
pub trait DepositSettlement<PoolId, TrancheId, Balance> {
	/// Pro-rata confirm pending deposit orders for a tranche up to `max_amount` USDC.
	///
	/// If total pending <= `max_amount`, all orders are confirmed in full.
	/// If total pending > `max_amount`, each investor's order is scaled down proportionally
	/// and the remainder stays in `PendingDepositOrders` for the next epoch.
	///
	/// Converts confirmed USDC amounts to tokens-to-mint using `epoch_price` and stores
	/// tokens in `ApprovedDepositOrders`.
	///
	/// Returns the actual USDC amount confirmed (for `tranche.invested` accounting).
	fn settle_deposit_orders(
		pool_id: PoolId,
		tranche_id: TrancheId,
		max_amount: Balance,
		epoch_price: FixedU128,
	) -> Balance;
}

/// Implemented by pallet-pools. Called by pallet-investments to keep aggregate
/// tranche pending totals and token supply in sync with per-investor order storage.
pub trait TrancheMutate<Balance> {
	/// Increment the tranche's aggregate pending deposit total.
	fn add_pending_deposit(
		pool_id: PoolId,
		tranche_id: TrancheId,
		amount: Balance,
	) -> frame_support::dispatch::DispatchResult;

	/// Decrement the tranche's aggregate pending deposit total.
	/// Called when an individual order is approved (Approval mode).
	fn sub_pending_deposit(
		pool_id: PoolId,
		tranche_id: TrancheId,
		amount: Balance,
	) -> frame_support::dispatch::DispatchResult;

	/// Increment the tranche's aggregate pending redeem total.
	fn add_pending_redeem(
		pool_id: PoolId,
		tranche_id: TrancheId,
		amount: Balance,
	) -> frame_support::dispatch::DispatchResult;

	/// Decrement the tranche's aggregate pending redeem total.
	/// Called when an individual redeem order is approved (Approval mode).
	fn sub_pending_redeem(
		pool_id: PoolId,
		tranche_id: TrancheId,
		amount: Balance,
	) -> frame_support::dispatch::DispatchResult;

	/// Decrement outstanding token supply.
	/// Called when a redeem request is submitted — tokens are burned on the Spoke
	/// chain at request time, so Hub state must reflect the burn immediately.
	fn sub_token_supply(
		pool_id: PoolId,
		tranche_id: TrancheId,
		amount: Balance,
	) -> frame_support::dispatch::DispatchResult;

	/// Increment the tranche's cumulative invested total.
	/// Called when deposit orders are confirmed in Approval mode so that
	/// `treasury_liquidity` (`invested - borrowed`) stays accurate.
	fn add_invested(
		pool_id: PoolId,
		tranche_id: TrancheId,
		amount: Balance,
	) -> frame_support::dispatch::DispatchResult;
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
