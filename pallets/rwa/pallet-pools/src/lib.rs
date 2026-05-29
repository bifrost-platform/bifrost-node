#![cfg_attr(not(feature = "std"), no_std)]

mod pallet;

pub use pallet::pallet::*;

use frame_support::{pallet_prelude::DispatchError, traits::EnsureOrigin};
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::{ConstU32, H160, U256};
use sp_runtime::{
	traits::{One, Saturating},
	BoundedBTreeMap, BoundedVec, FixedPointNumber, FixedU128, RuntimeDebug,
};

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

/// Seconds in a 365-day year, used to convert APR → per-second rate factor.
pub const SECONDS_PER_YEAR: u32 = 365 * 24 * 3600;

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
	/// At each epoch-open, `Tranche::accrue_interest` compounds `accrued_nav` by
	/// `interest_rate_per_sec^elapsed_blocks`, giving the senior's current total claim.
	Senior {
		/// Nominal annual rate as provided by the pool creator (e.g. 0.05 = 5%).
		apr: Rate,
		/// Per-block rate factor derived from `apr`: `1 + apr / SECONDS_PER_YEAR`.
		interest_rate_per_sec: Rate,
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
	/// Total asset amount waiting to be deposited (confirmed on epoch close).
	pub deposit: U256,
	/// Total tranche tokens waiting to be redeemed (confirmed on epoch close).
	pub redeem: U256,
}

// ---------------------------------------------------------------------------
// Tranche
// ---------------------------------------------------------------------------

#[derive(
	Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen, DecodeWithMemTracking,
)]
pub struct Tranche {
	pub tranche_type: TrancheType,
	/// Maximum asset amount that can be deposited into this tranche (cumulative cap).
	/// None means uncapped.
	pub max_deposits: Option<U256>,
	/// Number of tranche tokens currently outstanding (minted − burned).
	pub token_supply: U256,
	/// The total amount of assets invested into the tranche (cumulative inflow).
	/// The available amount to redeem or borrow from the tranche treasury will be (invested - borrowed).
	pub invested: U256,
	/// The amount of assets borrowed from the tranche treasury.
	pub borrowed: U256,
	/// Aggregate pending invest/redeem orders for the current epoch.
	pub pending_orders: TranchePendingOrders,
	/// Token price locked at the start of the settlement window, derived from the finalized NAV.
	/// None outside the settlement window. Reset to None when the epoch advances.
	pub epoch_price: Option<FixedU128>,
	/// Running accrued NAV for senior tranches.
	/// Grows by settled deposit amounts, shrinks by redeem payouts, and is compounded
	/// by `interest_rate_per_sec^elapsed_secs` at each epoch-open.
	/// Always zero for junior tranches.
	pub accrued_nav: U256,
}

impl Tranche {
	/// Token price = tranche_nav / token_supply.
	///
	/// `tranche_nav` is this tranche's share of total pool value after the waterfall split
	/// (oracle NAV + all treasury liquidity). Returns ONE when no tokens are outstanding.
	pub fn token_price(&self, tranche_nav: U256) -> FixedU128 {
		let supply: u128 = self.token_supply.try_into().unwrap_or(u128::MAX);
		if supply == 0 {
			return FixedU128::one();
		}
		let nav: u128 = tranche_nav.try_into().unwrap_or(u128::MAX);
		FixedU128::from_rational(nav, supply)
	}

	/// Compound-accrue `accrued_nav` for `elapsed_secs` seconds. No-op for junior tranches.
	pub fn accrue_interest(&mut self, elapsed_secs: u64) {
		if let TrancheType::Senior { interest_rate_per_sec, .. } = self.tranche_type {
			if elapsed_secs == 0 || self.accrued_nav.is_zero() {
				return;
			}
			let factor = fixed_u128_pow(interest_rate_per_sec, elapsed_secs as u32);
			let nav: u128 = self.accrued_nav.try_into().unwrap_or(u128::MAX);
			let new_nav = factor.checked_mul_int(nav).unwrap_or(nav);
			self.accrued_nav = U256::from(new_nav);
		}
	}

	pub fn treasury_liquidity(&self) -> U256 {
		self.invested.saturating_sub(self.borrowed)
	}
}

/// Binary exponentiation for FixedU128 in O(log exp) multiplications.
pub fn fixed_u128_pow(mut base: FixedU128, mut exp: u32) -> FixedU128 {
	let mut result = FixedU128::one();
	while exp > 0 {
		if exp % 2 == 1 {
			result = result.saturating_mul(base);
		}
		base = base.saturating_mul(base);
		exp /= 2;
	}
	result
}

// ---------------------------------------------------------------------------
// TrancheTypeInput / TrancheInput — extrinsic-facing types
// ---------------------------------------------------------------------------

/// Caller-supplied tranche type. Accepts APR; `create_pool` / `add_vault` derive
/// `interest_rate_per_sec` and store both values in `TrancheType`.
#[derive(
	Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen, DecodeWithMemTracking,
)]
pub enum TrancheTypeInput {
	/// Residual (junior) tranche.
	Junior,
	/// Senior tranche. Caller provides the nominal annual rate (APR).
	Senior {
		/// Nominal annual rate, e.g. `FixedU128::from_rational(5, 100)` for 5%.
		apr: Rate,
	},
}

impl TrancheTypeInput {
	/// Convert to the storage type, computing `interest_rate_per_sec = 1 + apr / SECONDS_PER_YEAR`.
	/// Returns `None` only if `apr` is so large that the division overflows (not realistic).
	pub fn try_into_tranche_type(self) -> Option<TrancheType> {
		match self {
			TrancheTypeInput::Junior => Some(TrancheType::Junior),
			TrancheTypeInput::Senior { apr } => {
				let rate_delta =
					Rate::from_inner(apr.into_inner().checked_div(SECONDS_PER_YEAR as u128)?);
				let interest_rate_per_sec = Rate::one().saturating_add(rate_delta);
				Some(TrancheType::Senior { apr, interest_rate_per_sec })
			},
		}
	}
}

#[derive(
	Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen, DecodeWithMemTracking,
)]
pub struct TrancheInput {
	/// Tranche type with APR (converted to `interest_rate_per_sec` at storage time).
	pub tranche_type: TrancheTypeInput,
	/// Globally unique tranche identifier: (chain_id, vault_address).
	pub tranche_id: TrancheId,
	/// Maximum asset amount that can be deposited into this tranche. None means uncapped.
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
// EpochInfo — timestamp-based epoch tracking with settlement window
// ---------------------------------------------------------------------------

#[derive(
	Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen, DecodeWithMemTracking,
)]
pub struct EpochInfo {
	/// Current epoch index.
	pub current_epoch: EpochId,
	/// Unix timestamp (seconds) when the current epoch started.
	pub epoch_start_secs: u64,
	/// Intended epoch duration in seconds (e.g. 86_400 = 1 day).
	pub epoch_length_secs: u64,
	/// Seconds before epoch end when the settlement window opens.
	/// During [settlement_start_secs, epoch_end), new orders are rejected.
	pub settlement_offset_secs: u64,
}

impl EpochInfo {
	pub fn new(epoch_length_secs: u64, settlement_offset_secs: u64, start_secs: u64) -> Self {
		EpochInfo {
			current_epoch: 0,
			epoch_start_secs: start_secs,
			epoch_length_secs,
			settlement_offset_secs,
		}
	}

	/// True when `now_secs` has reached or passed the end of the current epoch.
	pub fn should_advance(&self, now_secs: u64) -> bool {
		now_secs.saturating_sub(self.epoch_start_secs) >= self.epoch_length_secs
	}

	/// Advance to the next epoch starting at `now_secs`.
	pub fn advance(&mut self, now_secs: u64) {
		self.current_epoch = self.current_epoch.saturating_add(1);
		self.epoch_start_secs = now_secs;
	}

	/// Unix timestamp (seconds) when the settlement window opens for the current epoch.
	pub fn settlement_start_secs(&self) -> u64 {
		self.epoch_start_secs
			.saturating_add(self.epoch_length_secs)
			.saturating_sub(self.settlement_offset_secs)
	}

	/// True when `now_secs` is inside the settlement window.
	pub fn in_settlement_window(&self, now_secs: u64) -> bool {
		!self.should_advance(now_secs) && now_secs >= self.settlement_start_secs()
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

/// Implemented by pallet-pools. Called by pallet-investments and the gateway
/// to validate pool/tranche existence, resolve the borrower, and query epoch state.
pub trait PoolInspect<AccountId> {
	fn pool_exists(pool_id: PoolId) -> bool;
	/// Returns the borrower (institution EOA) authorized for this pool.
	fn pool_borrower(pool_id: PoolId) -> Option<AccountId>;
	fn tranche_exists(pool_id: PoolId, tranche_id: TrancheId) -> bool;
	/// True when the pool is currently inside its settlement window.
	fn in_settlement_window(pool_id: PoolId) -> bool;
	/// True when adding `amount` would push `invested + pending_deposit` above
	/// the tranche's `max_deposits` cap. Always false if the tranche is uncapped.
	fn deposit_cap_exceeded(pool_id: PoolId, tranche_id: TrancheId, amount: U256) -> bool;
	/// Returns `invested - borrowed` for the tranche — assets available to cover redemptions.
	fn treasury_liquidity(pool_id: PoolId, tranche_id: TrancheId) -> U256;
	/// Returns the token price locked at settlement start for this tranche, if finalized.
	fn epoch_price(pool_id: PoolId, tranche_id: TrancheId) -> Option<FixedU128>;
	/// Returns the current Gateway EVM contract address stored on-chain.
	/// Both precompiles read this to verify the EVM-level caller before dispatching.
	fn gateway_address() -> H160;
	/// Returns the current epoch index for a pool, or `None` if the pool does not exist.
	fn current_epoch(pool_id: PoolId) -> Option<EpochId>;
	/// Returns the deposit settlement mode for a pool, or `None` if the pool does not exist.
	fn deposit_settlement_mode(pool_id: PoolId) -> Option<SettlementMode>;
	/// Returns the redeem settlement mode for a pool, or `None` if the pool does not exist.
	fn redeem_settlement_mode(pool_id: PoolId) -> Option<SettlementMode>;
}

/// Defined here, implemented by pallet-investments.
/// Called from pallet-pools' `on_initialize` during automatic epoch settlement.
pub trait Settlement<PoolId, TrancheId, Balance> {
	/// Settle all pending deposit orders for a tranche at the given epoch price.
	///
	/// Settled orders move to `ClaimableDepositOrders`; investors pull-claim via
	/// `claim_deposit`, which triggers outbound share minting on the spoke chain.
	///
	/// Returns the total amount settled (for `tranche.invested` accounting).
	fn settle_deposit_orders(
		pool_id: PoolId,
		tranche_id: TrancheId,
		epoch_price: FixedU128,
	) -> Balance;

	/// Pro-rata settle pending redeem orders for a tranche up to `max_asset_payout`
	/// (the tranche's available treasury liquidity).
	///
	/// If total asset value owed <= `max_asset_payout`, all orders are settled in full.
	/// If total asset value owed > `max_asset_payout`, each order is scaled proportionally
	/// and the remainder stays in `PendingRedeemOrders` for the next epoch.
	///
	/// Settled orders move to `ClaimableRedeemOrders`; investors pull-claim via
	/// `claim_redeem`, which triggers outbound asset payout on the spoke chain.
	///
	/// Returns `(tokens_settled, asset_payout)` — used to decrement
	/// `tranche.pending_orders.redeem` and `tranche.invested` in pallet-pools.
	fn settle_redeem_orders(
		pool_id: PoolId,
		tranche_id: TrancheId,
		max_asset_payout: Balance,
		epoch_price: FixedU128,
	) -> (Balance, Balance);
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

	/// Increment outstanding token supply.
	/// Called when deposit orders are approved (Approval) or settled (Automatic)
	/// so that `token_price()` divides by the correct outstanding supply.
	fn add_token_supply(
		pool_id: PoolId,
		tranche_id: TrancheId,
		amount: Balance,
	) -> frame_support::dispatch::DispatchResult;

	/// Increment the tranche's cumulative invested total.
	/// Called when deposit orders are approved (Approval mode) so that
	/// `treasury_liquidity` (`invested - borrowed`) stays accurate.
	fn add_invested(
		pool_id: PoolId,
		tranche_id: TrancheId,
		amount: Balance,
	) -> frame_support::dispatch::DispatchResult;

	/// Decrement the tranche's cumulative invested total.
	/// Called when redeem orders are approved (Approval mode) so that
	/// `treasury_liquidity` (`invested - borrowed`) stays accurate.
	fn sub_invested(
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

// ---------------------------------------------------------------------------
// Gateway origin
// ---------------------------------------------------------------------------

/// `EnsureOrigin` that accepts only the `Gateway` pallet origin.
/// The pools precompile creates this origin before dispatching to `borrow` / `repay`.
/// Wire as `type GatewayOrigin = pallet_pools::EnsureGateway` in the runtime.
pub struct EnsureGateway;

impl<OuterOrigin> EnsureOrigin<OuterOrigin> for EnsureGateway
where
	OuterOrigin: Into<Result<Origin, OuterOrigin>> + From<Origin>,
{
	type Success = ();
	fn try_origin(o: OuterOrigin) -> Result<Self::Success, OuterOrigin> {
		match o.into() {
			Ok(Origin::Gateway) => Ok(()),
			Err(o) => Err(o),
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin() -> Result<OuterOrigin, ()> {
		Ok(OuterOrigin::from(Origin::Gateway))
	}
}
