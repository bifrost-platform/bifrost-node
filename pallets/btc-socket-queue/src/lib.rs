#![cfg_attr(not(feature = "std"), no_std)]

mod pallet;
pub use pallet::pallet::*;

pub mod weights;
use weights::WeightInfo;

use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

use bp_multi_sig::MULTI_SIG_MAX_ACCOUNTS;
use sp_core::{ConstU32, RuntimeDebug};
use sp_runtime::BoundedBTreeMap;
use sp_std::vec::Vec;

pub type ReqId = u128;

#[derive(Decode, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct OutboundRequests<AccountId> {
	pub origin_psbt: Vec<u8>,
	pub signed_psbts: BoundedBTreeMap<AccountId, Vec<u8>, ConstU32<MULTI_SIG_MAX_ACCOUNTS>>,
}

impl<AccountId: PartialEq + Clone + Ord> OutboundRequests<AccountId> {
	pub fn new(origin_psbt: Vec<u8>) -> Self {
		Self { origin_psbt, signed_psbts: BoundedBTreeMap::default() }
	}
}

#[derive(Decode, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct UnsignedPsbtMessage<AccountId> {
	pub submitter: AccountId,
	pub req_id: ReqId,
	pub psbt: Vec<u8>,
}

#[derive(Decode, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct SignedPsbtMessage<AccountId> {
	pub authority_id: AccountId,
	pub req_id: ReqId,
	pub origin_psbt: Vec<u8>,
	pub signed_psbt: Vec<u8>,
}
