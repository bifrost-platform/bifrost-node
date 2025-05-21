#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]

mod pallet;
pub use pallet::pallet::*;

pub mod weights;
use weights::WeightInfo;

use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_core::{RuntimeDebug, U256};
use sp_std::vec::Vec;

#[derive(Decode, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct ScheduledCall<AccountId, BlockNumber> {
	pub info: CallInfo<AccountId>,

	// TODO: handle the following fields
	pub banned: bool,
	pub failed_count: u32,
	pub last_executed: Option<BlockNumber>,
}

impl<AccountId: PartialEq + Clone, BlockNumber: PartialEq + Clone>
	ScheduledCall<AccountId, BlockNumber>
{
	pub fn new(info: CallInfo<AccountId>) -> Self {
		Self { info, banned: false, failed_count: 0, last_executed: None }
	}
}

#[derive(Decode, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct CallInfo<AccountId> {
	/// The gas payer. (Unique in the system)
	pub from: AccountId,
	/// The contract to call.
	pub to: AccountId,
	/// The data to send to the contract.
	pub data: Vec<u8>,
	/// The value to send to the contract.
	pub value: U256,
	/// The gas limit for the call.
	pub gas: U256,
	/// The interval at which to call the contract. (In blocks)
	pub interval: u32,
}
