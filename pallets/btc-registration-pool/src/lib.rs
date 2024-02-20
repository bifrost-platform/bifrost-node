#![cfg_attr(not(feature = "std"), no_std)]

mod pallet;
pub mod weights;

pub use pallet::pallet::*;

use sp_core::ConstU32;
use sp_runtime::BoundedVec;
use weights::WeightInfo;

use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

pub const ADDRESS_MAX_BYTE_LENGTH: u32 = 62;

pub type BoundedBitcoinAddress = BoundedVec<u8, ConstU32<ADDRESS_MAX_BYTE_LENGTH>>;

#[derive(Decode, Encode, TypeInfo)]
pub struct BitcoinAddressPair {
	pub vault_address: BoundedBitcoinAddress,
	pub refund_address: BoundedBitcoinAddress,
}

impl BitcoinAddressPair {
	pub fn new(
		vault_address: BoundedBitcoinAddress,
		refund_address: BoundedBitcoinAddress,
	) -> Self {
		Self { vault_address, refund_address }
	}
}
