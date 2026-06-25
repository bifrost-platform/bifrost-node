#![cfg_attr(not(feature = "std"), no_std)]

mod pallet;

pub use pallet::pallet::*;

use pallet_pools::{PoolId, TrancheId};
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;

// ---------------------------------------------------------------------------
// Role
// ---------------------------------------------------------------------------

/// A permission role scoped to a pool.
///
/// Permission hierarchy:
/// - `sudo` → grant/revoke `PoolAdmin` (pre-granted before the pool is created)
/// - `PoolAdmin` → grant/revoke `Borrower` | `OracleFeeder` | `TrancheInvestor`
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
pub enum Role {
	/// May create the pool and manage sub-roles for it.
	/// Granted by sudo before the pool is created.
	PoolAdmin,
	/// May approve deposit/redeem orders, borrow, and repay on behalf of the institution.
	Borrower,
	/// May submit NAV updates for the pool's collateral assets.
	OracleFeeder,
	/// May submit deposit and redeem orders for a specific tranche.
	TrancheInvestor(TrancheId),
}
