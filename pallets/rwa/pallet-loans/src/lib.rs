#![cfg_attr(not(feature = "std"), no_std)]

mod pallet;

pub use pallet::pallet::*;
pub use pallet_pools::{PoolId, PoolInspect, PoolNAV, PoolReserve, Rate};

use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::{H256, U256};
use sp_runtime::{traits::One, FixedPointNumber, FixedU128, RuntimeDebug, Saturating};

#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Primitive type aliases
// ---------------------------------------------------------------------------

/// Loan identifier (unique within a pool).
pub type LoanId = u64;

// ---------------------------------------------------------------------------
// LoanStatus
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
pub enum LoanStatus {
	/// Loan is open — borrower can draw and repay.
	Active,
	/// Loan is fully repaid and closed.
	Closed,
}

// ---------------------------------------------------------------------------
// LoanDetails
// ---------------------------------------------------------------------------

#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct LoanDetails<AccountId> {
	/// Borrower account authorized to draw and repay this loan.
	pub borrower: AccountId,
	/// Hash of the off-chain RWA collateral documents.
	pub collateral: H256,
	/// Maximum lifetime amount the borrower can draw.
	pub ceiling: U256,
	/// Per-second compound interest rate, stored as `1 + r`.
	pub rate_per_sec: Rate,
	/// Outstanding principal — increases on borrow, decreases on principal-portion repayment.
	pub principal: U256,
	/// Accrued interest since `last_accrued` — increases via `accrue()`,
	/// decreases when repayment is applied to interest first.
	pub interest: U256,
	/// Lifetime gross amount drawn (used to enforce ceiling).
	pub total_borrowed: U256,
	/// Lifetime gross amount repaid (tracking metric).
	pub total_repaid: U256,
	/// Block number when interest was last accrued.
	pub last_accrued: u32,
	/// Loan lifecycle status.
	pub status: LoanStatus,
}

impl<AccountId> LoanDetails<AccountId> {
	/// Outstanding obligation: principal + accrued interest.
	pub fn debt(&self) -> U256 {
		self.principal.saturating_add(self.interest)
	}

	/// Compound interest on the outstanding debt up to block `now`.
	pub fn accrue(&mut self, now: u32) {
		let delta = now.saturating_sub(self.last_accrued);
		if delta > 0 && !self.principal.is_zero() {
			let current_debt: u128 = self.debt().try_into().unwrap_or(u128::MAX);
			let new_debt = compound(self.rate_per_sec, current_debt, delta as u64);
			let principal: u128 = self.principal.try_into().unwrap_or(u128::MAX);
			self.interest = U256::from(new_debt.saturating_sub(principal));
		}
		self.last_accrued = now;
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
