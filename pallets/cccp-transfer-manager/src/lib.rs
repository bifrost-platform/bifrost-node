#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]

mod pallet;
pub mod weights;

pub use pallet::pallet::*;
pub use weights::WeightInfo;

use bp_cccp::UnboundedBytes;
use bp_staking::MAX_AUTHORITIES;
use frame_support::traits::Currency;
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode};
use scale_info::TypeInfo;
use sp_core::{ConstU32, RuntimeDebug, H160, H256};
use sp_runtime::BoundedVec;

/// Asset address type.
pub type AssetId = H160;

/// Asset index hash type.
pub type AssetIndexHash = H256;

#[derive(
	Decode, Encode, TypeInfo, Clone, Copy, PartialEq, Eq, RuntimeDebug, DecodeWithMemTracking,
)]
pub enum TransferOption {
	Fast,
	Standard,
}

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
pub struct TransferInfo<Balance, AccountId> {
	/// The amount of the transfer.
	pub amount: Balance,
	/// The option of the transfer.
	pub option: TransferOption,
	/// The initial socket message of the transfer. (status: REQUESTED)
	pub socket_message: UnboundedBytes,
	/// Voters of the transfer. Voting are only required for inbound requests since the source chain are non-bifrost chains.
	/// Socket messages originated by outbound requests are internally validated by the pallet itself. (=immediately approved)
	pub voters: BoundedVec<AccountId, ConstU32<MAX_AUTHORITIES>>,
	/// The voting status of the transfer.
	/// It'll only be approved when the majority of relayers voted for the request. (for inbound requests)
	pub is_approved: bool,
}

#[derive(Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
/// A submission of Socket message.
pub struct SocketMessageSubmission<AccountId> {
	/// The authority id.
	pub authority_id: AccountId,
	/// The Socket message.
	pub message: UnboundedBytes,
}
