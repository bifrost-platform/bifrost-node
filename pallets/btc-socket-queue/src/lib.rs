#![cfg_attr(not(feature = "std"), no_std)]

mod pallet;
pub use pallet::pallet::*;

pub mod weights;
use weights::WeightInfo;

use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_std::vec::Vec;

pub const MAX_QUEUE_SIZE: u32 = 1_000;

/// The maximum amount of accounts a multi-sig account can consist.
pub const MULTI_SIG_MAX_ACCOUNTS: u32 = 16;

pub type ReqId = u128;

#[derive(Decode, Encode, TypeInfo)]
pub struct UnsignedPsbtMessage {
	pub req_id: ReqId,
	pub psbt: Vec<u8>,
}

#[derive(Decode, Encode, TypeInfo)]
pub struct SignedPsbtMessage<AccountId> {
	pub psbt: Vec<u8>,
	pub authority_id: AccountId,
	pub status: SignStatus,
}

#[derive(Decode, Encode, TypeInfo)]
pub enum SignStatus {
	Accepted,
	Rejected,
}
