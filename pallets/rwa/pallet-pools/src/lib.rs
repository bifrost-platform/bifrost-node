#![cfg_attr(not(feature = "std"), no_std)]

mod pallet;

pub use pallet::pallet::*;

use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::{H160, U256};
use sp_runtime::{traits::One, FixedPointNumber, FixedU128, Perquintill, RuntimeDebug, Saturating};

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
	/// Residual (junior) tranche — absorbs losses, receives residual return.
	Junior,
	/// Non-residual (senior) tranche with a fixed rate and minimum risk buffer.
	Senior {
		/// Stored as `1 + annual_rate / SECONDS_PER_YEAR` (FixedU128).
		interest_rate_per_sec: Rate,
		/// Minimum junior-protection ratio required for a healthy solution.
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

#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct Tranche {
	pub tranche_type: TrancheType,
	/// Globally unique tranche identifier: (chain_id, vault_address).
	pub tranche_id: TrancheId,
	/// Original amount invested by investors — reduced only when redemptions are settled.
	pub principal: U256,
	/// Compound interest accrued on top of `principal` since last settlement.
	pub interest: U256,
	/// Total invested amount in this tranche.
	pub total: U256,
	/// Block number when interest was last accrued.
	pub last_updated_interest: u32,
	/// Seniority weight used in epoch solution scoring.
	pub seniority: u32,
}

impl Tranche {
	/// Total obligation to investors: principal + accrued interest.
	pub fn debt(&self) -> U256 {
		self.principal.saturating_add(self.interest)
	}

	/// Compound interest on the outstanding debt up to block `now`.
	/// Only applies to Senior tranches — Junior has no fixed rate.
	pub fn accrue(&mut self, now: u32) {
		if let TrancheType::Senior { interest_rate_per_sec, .. } = self.tranche_type {
			let delta = now.saturating_sub(self.last_updated_interest);
			if delta > 0 && !self.principal.is_zero() {
				let current_debt: u128 = self.debt().try_into().unwrap_or(u128::MAX);
				let new_debt = compound(interest_rate_per_sec, current_debt, delta as u64);
				let principal: u128 = self.principal.try_into().unwrap_or(u128::MAX);
				self.interest = U256::from(new_debt.saturating_sub(principal));
			}
		}
		self.last_updated_interest = now;
	}

	/// Token price = debt / token_supply. Returns ONE when no tokens are outstanding.
	pub fn token_price(&self) -> FixedU128 {
		let debt: u128 = self.debt().try_into().unwrap_or(u128::MAX);
		let supply: u128 = self.total.try_into().unwrap_or(u128::MAX);
		if supply == 0 {
			return FixedU128::one();
		}
		FixedU128::from_rational(debt, supply)
	}
}

/// Fast exponentiation: `base^exp` applied to `principal`.
fn compound(rate: Rate, principal: u128, exp: u64) -> u128 {
	let mut base = rate;
	let mut result = FixedU128::one();
	let mut n = exp;
	while n > 0 {
		if n & 1 == 1 {
			result = result.saturating_mul(base);
		}
		base = base.saturating_mul(base);
		n >>= 1;
	}
	result.saturating_mul_int(principal)
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
	/// Seniority weight for scoring.
	pub seniority: u32,
}

// ---------------------------------------------------------------------------
// ReserveDetails
// ---------------------------------------------------------------------------

#[derive(Clone, Default, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct ReserveDetails {
	/// Admin-configured maximum reserve. (total reserve cannot exceed this amount)
	pub max: U256,
	/// Total reserve balance.
	pub total: U256,
	/// Available reserve to be used for borrowers.
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
// EpochInfo — block-number-based epoch tracking
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
}

impl EpochInfo {
	pub fn new(epoch_length: u32, start_block: u32) -> Self {
		EpochInfo { current_epoch: 0, epoch_start_block: start_block, epoch_length }
	}

	/// True when `now` has passed the end of the current epoch.
	pub fn should_advance(&self, now: u32) -> bool {
		now.saturating_sub(self.epoch_start_block) >= self.epoch_length
	}

	/// Advance to the next epoch starting at `now`.
	pub fn advance(&mut self, now: u32) {
		self.current_epoch = self.current_epoch.saturating_add(1);
		self.epoch_start_block = now;
	}
}

// ---------------------------------------------------------------------------
// PoolDetails
// ---------------------------------------------------------------------------

#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct PoolDetails<AccountId> {
	/// Pool admin account address.
	pub admin: AccountId,
	/// Accepted currency contract address (on Bifrost EVM).
	/// For example, if the pool accepts USDC, the currency will be the UnifiedUSDC contract address.
	pub currency: H160,
	/// Reserve details.
	pub reserve: ReserveDetails,
	/// Ordered tranches list: index 0 = most senior, last = residual (junior).
	/// Each tranche is identified by its ERC-7540 `vault_address` on the external chain.
	pub tranches: sp_std::vec::Vec<Tranche>,
	/// Block-number-based epoch tracking.
	pub epoch: EpochInfo,
	/// Maximum age (in blocks) of the NAV before closing an epoch is blocked.
	pub max_nav_age: u32,
	/// Most recent NAV reported by pallet-loans.
	pub last_nav: U256,
	/// Block number when `last_nav` was recorded.
	pub last_nav_update: u32,
}

// ---------------------------------------------------------------------------
// Traits
// ---------------------------------------------------------------------------

use frame_support::pallet_prelude::DispatchError;

/// Implemented by pallet-pools. Called by pallet-investments / pallet-loans to
/// validate pool/tranche existence and resolve the pool admin for auth checks.
pub trait PoolInspect<AccountId> {
	fn pool_exists(pool_id: PoolId) -> bool;
	fn pool_admin(pool_id: PoolId) -> Option<AccountId>;
	fn tranche_exists(pool_id: PoolId, tranche_id: TrancheId) -> bool;
}

/// Implemented by pallet-loans. Called by pallet-pools to fetch or refresh the
/// current NAV (net asset value = total loan AUM) for a pool.
pub trait PoolNAV<PoolId, Balance> {
	/// Returns `(nav, block_number)` of the last recorded NAV without recomputing.
	fn nav(pool_id: PoolId) -> Option<(Balance, u32)>;

	/// Triggers a fresh NAV computation across all active loans and returns the result.
	fn update_nav(pool_id: PoolId) -> Result<Balance, DispatchError>;
}

/// Implemented by pallet-pools. Called by pallet-loans when disbursing or receiving
/// loan repayments so that pool reserve accounting stays consistent.
pub trait PoolReserve<Balance> {
	/// Decrease available reserve by `amount` (loan disbursement).
	/// Returns `Err` if insufficient available reserve.
	fn withdraw(pool_id: PoolId, amount: Balance) -> frame_support::dispatch::DispatchResult;

	/// Increase total and available reserve by `amount` (loan repayment or invest settlement).
	fn deposit(pool_id: PoolId, amount: Balance) -> frame_support::dispatch::DispatchResult;

	/// Read available reserve for a pool.
	fn available_reserve(pool_id: PoolId) -> Balance;
}
