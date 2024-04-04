#![cfg_attr(not(feature = "std"), no_std)]

pub mod traits;

pub use miniscript::bitcoin::{Address, Network, Psbt, PublicKey, Script};

use sp_core::{ConstU32, RuntimeDebug};
use sp_runtime::{BoundedBTreeMap, BoundedVec};
use sp_std::vec::Vec;

use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

/// The maximum amount of accounts a multi-sig account can consist.
pub const MULTI_SIG_MAX_ACCOUNTS: u32 = 16;

/// The maximum length of a valid Bitcoin address in characters (~64 alphanumeric characters).
pub const ADDRESS_MAX_LENGTH: u32 = 64;

/// The maximum length of a valid public key in characters (~66 alphanumeric characters).
pub const PUBLIC_KEY_MAX_LENGTH: u32 = 66;

/// The Bitcoin address type (length bounded).
pub type BoundedBitcoinAddress = BoundedVec<u8, ConstU32<ADDRESS_MAX_LENGTH>>;

/// Length unbounded bytes type.
pub type UnboundedBytes = Vec<u8>;

#[derive(
	Clone,
	Copy,
	Decode,
	Encode,
	Eq,
	Ord,
	PartialOrd,
	PartialEq,
	TypeInfo,
	MaxEncodedLen,
	RuntimeDebug,
)]
/// A 33 byte length public key.
pub struct Public(pub [u8; 33]);

impl AsRef<[u8]> for Public {
	fn as_ref(&self) -> &[u8] {
		&self.0
	}
}

#[derive(Decode, Encode, TypeInfo, MaxEncodedLen)]
/// A m-of-n multi signature based Bitcoin address.
pub struct MultiSigAccount<AccountId> {
	/// The vault address.
	pub address: AddressState,
	/// Public keys that the vault address contains.
	pub pub_keys: BoundedBTreeMap<AccountId, Public, ConstU32<MULTI_SIG_MAX_ACCOUNTS>>,
	/// The m value of the multi-sig address.
	pub m: u8,
	/// The n value of the multi-sig address.
	pub n: u8,
}

impl<AccountId: PartialEq + Clone + Ord> MultiSigAccount<AccountId> {
	pub fn new(m: u8, n: u8) -> Self {
		Self { address: AddressState::Pending, pub_keys: BoundedBTreeMap::new(), m, n }
	}

	pub fn is_pending(&self) -> bool {
		matches!(self.address, AddressState::Pending)
	}

	pub fn is_address(&self, address: &BoundedBitcoinAddress) -> bool {
		match &self.address {
			AddressState::Pending => false,
			AddressState::Generated(a) => a == address,
		}
	}

	pub fn is_key_generation_ready(&self) -> bool {
		self.n as usize == self.pub_keys.len()
	}

	pub fn is_key_submitted(&self, pub_key: &Public) -> bool {
		self.pub_keys.values().cloned().collect::<Vec<Public>>().contains(pub_key)
	}

	pub fn is_authority_submitted(&self, authority_id: &AccountId) -> bool {
		self.pub_keys.contains_key(authority_id)
	}

	pub fn set_address(&mut self, address: BoundedBitcoinAddress) {
		self.address = AddressState::Generated(address)
	}

	pub fn pub_keys(&self) -> Vec<Public> {
		self.pub_keys.values().cloned().collect()
	}
}

#[derive(Decode, Encode, TypeInfo, MaxEncodedLen)]
/// The vault address state.
pub enum AddressState {
	/// Required number of public keys has not been submitted yet.
	Pending,
	/// n public keys has been submitted and address generation done.
	Generated(BoundedBitcoinAddress),
}
