#![cfg_attr(not(feature = "std"), no_std)]

mod pallet;
pub mod weights;

pub use pallet::pallet::*;

use sp_core::ConstU32;
use sp_runtime::BoundedVec;
use weights::WeightInfo;

use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

/// The maximum length of a valid signature in bytes (~65 bytes).
pub const SIGNATURE_MAX_BYTE_LENGTH: u32 = 65;

/// The maximum length of a valid Bitcoin address in bytes (~32 bytes).
pub const ADDRESS_MAX_BYTE_LENGTH: u32 = 32;

/// The Bitcoin address type (length bounded).
pub type BoundedBitcoinAddress = BoundedVec<u8, ConstU32<ADDRESS_MAX_BYTE_LENGTH>>;

#[derive(Decode, Encode, TypeInfo)]
/// The registered Bitcoin address pair.
pub struct BitcoinAddressPair {
	/// For outbound.
	pub refund_address: BoundedBitcoinAddress,
	/// For inbound.
	pub vault_address: BoundedBitcoinAddress,
}

impl BitcoinAddressPair {
	pub fn new(
		refund_address: BoundedBitcoinAddress,
		vault_address: BoundedBitcoinAddress,
	) -> Self {
		Self { refund_address, vault_address }
	}
}
