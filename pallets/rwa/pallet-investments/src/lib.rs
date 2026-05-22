#![cfg_attr(not(feature = "std"), no_std)]

mod pallet;

pub use pallet::pallet::*;
pub use pallet_pools::{DepositSettlement, EpochId, PoolId, PoolInspect, TrancheId, TrancheMutate};

use frame_support::traits::EnsureOrigin;
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::U256;
use sp_runtime::RuntimeDebug;

// ---------------------------------------------------------------------------
// Order structs
// ---------------------------------------------------------------------------

/// A pending deposit order held in `PendingDepositOrders` storage.
///
/// `epoch_id` and `submitted_at` are set on first insertion and preserved across
/// top-up submissions within the same epoch (additional calls to
/// `submit_deposit_order` by the same investor just increment `amount`).
/// Partial-fill remainders carried forward from a prior epoch also retain the
/// original metadata so the full order history is traceable via events.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen,
)]
pub struct PendingDepositOrder {
	/// Cumulative USDC amount pending settlement.
	pub amount: U256,
	/// Epoch index during which the order was first submitted.
	pub epoch_id: EpochId,
	/// Block number when the order was first submitted.
	pub submitted_at: u32,
}

/// An approved deposit order held in `ApprovedDepositOrders` storage.
///
/// Written by `approve_deposit_orders` (Approval mode) or `settle_deposit_orders`
/// (Automatic mode). Entries accumulate across epochs until the off-chain mint
/// completes and the claim flow clears the entry.
/// `epoch_id` and `approved_at` always reflect the **most recent** approval.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen,
)]
pub struct ApprovedDepositOrder {
	/// Cumulative asset amount confirmed across all approvals (cost basis).
	pub amount: U256,
	/// Cumulative tranche shares to be minted on the Spoke chain.
	pub shares_to_mint: U256,
	/// Epoch index during which the most recent approval occurred.
	pub epoch_id: EpochId,
	/// Block number of the most recent approval.
	pub approved_at: u32,
}

/// A pending redeem order held in `PendingRedeemOrders` storage.
///
/// `epoch_id` and `submitted_at` are set on first insertion and preserved across
/// top-up submissions. Tokens are burned on the Spoke chain at submission time so
/// the Hub reflects the burn immediately via `sub_token_supply`.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen,
)]
pub struct PendingRedeemOrder {
	/// Cumulative tranche token amount pending settlement.
	pub amount: U256,
	/// Epoch index during which the order was first submitted.
	pub epoch_id: EpochId,
	/// Block number when the order was first submitted.
	pub submitted_at: u32,
}

/// An approved redeem order held in `ApprovedRedeemOrders` storage.
///
/// Written by `approve_redeem_orders` (Approval mode).
/// `epoch_id` and `approved_at` always reflect the **most recent** approval.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen,
)]
pub struct ApprovedRedeemOrder {
	/// Cumulative tranche shares surrendered for redemption.
	pub shares_redeemed: U256,
	/// Cumulative asset amount to be paid out to the investor.
	pub payout: U256,
	/// Epoch index during which the most recent approval occurred.
	pub epoch_id: EpochId,
	/// Block number of the most recent approval.
	pub approved_at: u32,
}

// ---------------------------------------------------------------------------
// Gateway origin
// ---------------------------------------------------------------------------

/// `EnsureOrigin` that accepts only the `Gateway` pallet origin.
/// The investments precompile creates this origin before dispatching.
/// Wire as `type GatewayOrigin = pallet_investments::EnsureGateway` in the runtime.
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
