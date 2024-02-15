#![cfg_attr(not(feature = "std"), no_std)]

mod pallet;
pub use pallet::pallet::*;

use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

pub type VerifiedBitcoinAddress = String;

#[derive(Decode, Encode, TypeInfo)]
pub struct PoolMember {
	pub vault_btc_address: VerifiedBitcoinAddress,
	pub user_btc_address: VerifiedBitcoinAddress,
}
