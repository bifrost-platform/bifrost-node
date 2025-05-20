#![cfg_attr(not(feature = "std"), no_std)]

mod pallet;
pub use pallet::pallet::*;

pub mod weights;
use weights::WeightInfo;

use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_core::{RuntimeDebug, U256};
use sp_std::vec::Vec;

#[derive(Decode, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct ScheduledCallInfo<AccountId> {
	pub from: AccountId,
	pub to: AccountId,
	pub data: Vec<u8>,
	pub value: U256,
	pub interval: u32,
}
