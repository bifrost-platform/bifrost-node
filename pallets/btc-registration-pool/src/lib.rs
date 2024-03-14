#![cfg_attr(not(feature = "std"), no_std)]

mod pallet;
pub mod weights;

pub use pallet::pallet::*;
use weights::WeightInfo;

use sp_core::{ConstU32, RuntimeDebug};
use sp_runtime::BoundedVec;
use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

use frame_support::traits::SortedMembers;
use parity_scale_codec::{Decode, Encode};
use scale_info::{prelude::string::String, TypeInfo};

/// The maximum length of a valid Bitcoin address in characters (~62 alphanumeric characters).
pub const ADDRESS_MAX_LENGTH: u32 = 62;

/// The Bitcoin address type (length bounded).
pub type BoundedBitcoinAddress = BoundedVec<u8, ConstU32<ADDRESS_MAX_LENGTH>>;

#[derive(Decode, Encode, TypeInfo)]
/// A m-of-n multi signature based Bitcoin address.
pub struct MultiSigAddress {
	pub address: BoundedBitcoinAddress,
	pub m: u8,
	pub n: u8,
}

impl MultiSigAddress {
	pub fn new<T: Config>(address: BoundedBitcoinAddress) -> Self {
		Self { address, m: <RequiredSignatures<T>>::get(), n: <RequiredPubKeys<T>>::get() }
	}
}

#[derive(Decode, Encode, TypeInfo)]
/// The vault address.
pub enum VaultAddress {
	/// Required number of public keys has not been submitted yet.
	Pending,
	/// n public keys has been submitted and address generation done.
	Generated(MultiSigAddress),
}

#[derive(Decode, Encode, TypeInfo)]
/// The registered Bitcoin address pair.
pub struct BitcoinAddressPair<AccountId> {
	/// For outbound.
	pub refund_address: BoundedBitcoinAddress,
	/// For inbound.
	pub vault_address: VaultAddress,
	/// Public keys that the vault address contains.
	pub pub_keys: BTreeMap<AccountId, String>,
}

impl<AccountId: PartialEq + Clone + Ord> BitcoinAddressPair<AccountId> {
	pub fn new(refund_address: BoundedBitcoinAddress) -> Self {
		Self { refund_address, vault_address: VaultAddress::Pending, pub_keys: Default::default() }
	}

	pub fn is_pending(&self) -> bool {
		matches!(self.vault_address, VaultAddress::Pending)
	}

	pub fn is_generation_ready<T: Config>(&self) -> bool {
		T::Executives::count() == self.pub_keys.len()
	}

	pub fn is_key_submitted(&self, pub_key: &String) -> bool {
		self.pub_keys.values().cloned().collect::<Vec<String>>().contains(pub_key)
	}

	pub fn insert_pub_key(&mut self, authority_id: AccountId, pub_key: String) {
		self.pub_keys.insert(authority_id, pub_key);
	}
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
/// The payload used for public key submission.
pub struct KeySubmission<AccountId> {
	/// The authority Ethereum address. (Relay executive)
	pub authority_id: AccountId,
	/// The target Ethereum address.
	pub who: AccountId,
	/// The generated public key. (33 bytes)
	pub pub_key: String,
}
