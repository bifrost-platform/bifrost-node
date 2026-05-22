#![cfg_attr(not(feature = "std"), no_std)]

mod pallet;

pub use pallet::pallet::*;
pub use pallet_pools::{DepositSettlement, PoolId, PoolInspect, TrancheId, TrancheMutate};

use frame_support::traits::EnsureOrigin;

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
