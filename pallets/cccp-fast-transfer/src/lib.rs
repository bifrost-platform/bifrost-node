#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]

mod pallet;
pub mod weights;

pub use pallet::pallet::*;
pub use weights::WeightInfo;

use bp_staking::MAX_AUTHORITIES;
use frame_support::traits::Currency;
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_core::{ConstU32, RuntimeDebug, H160};
use sp_runtime::BoundedVec;

/// Length unbounded bytes type.
pub type UnboundedBytes = Vec<u8>;

/// Asset address type.
pub type AssetId = H160;

pub type BalanceOf<T> =
	<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

#[derive(Decode, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct AssetCapInfo<Balance> {
	/// The maximum on-flight cap of the asset.
	pub max_on_flight_cap: Balance,
	/// The current on-flight cap of the asset.
	pub on_flight_cap: Balance,
}

#[derive(Decode, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct FastTransfer<AccountId, Balance> {
	/// The amount of the fast transfer.
	pub amount: Balance,
	/// The socket message of the fast transfer. (status: REQUESTED|EXECUTED)
	pub socket_message: UnboundedBytes,
	/// The voters of the fast transfer.
	pub voters: BoundedVec<AccountId, ConstU32<MAX_AUTHORITIES>>,
}
