#![cfg_attr(not(feature = "std"), no_std)]

pub mod blaze;
pub mod traits;
pub mod utils;

pub use miniscript::{
	bitcoin::{
		hashes::Hash, key::FromSliceError, secp256k1::Secp256k1, Address, Amount, Network, Psbt,
		PublicKey, Script, Txid,
	},
	psbt::PsbtExt,
	Descriptor,
};

use sp_core::{ConstU32, RuntimeDebug};
use sp_runtime::{BoundedBTreeMap, BoundedVec};
use sp_std::vec::Vec;

use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

/// The maximum amount of accounts a multi-sig account can consist.
pub const MULTI_SIG_MAX_ACCOUNTS: u32 = 20;

/// The maximum length of a valid Bitcoin address in characters (~90 alphanumeric characters).
pub const ADDRESS_MAX_LENGTH: u32 = 90;

/// The maximum length of a valid public key in bytes (33 bytes).
pub const PUBLIC_KEY_LENGTH: u32 = 33;

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

#[derive(Decode, Encode, TypeInfo)]
/// An m-of-n multi signature based Bitcoin address.
pub struct MultiSigAccount<AccountId> {
	/// The vault address.
	pub address: AddressState,
	/// The descriptor of the script.
	pub descriptor: UnboundedBytes,
	/// Public keys that the vault address contains.
	pub pub_keys: BoundedBTreeMap<AccountId, Public, ConstU32<MULTI_SIG_MAX_ACCOUNTS>>,
	/// The m value of the multi-sig address.
	pub m: u32,
	/// The n value of the multi-sig address.
	pub n: u32,
}

impl<AccountId: PartialEq + Clone + Ord + sp_std::fmt::Debug> MultiSigAccount<AccountId> {
	pub fn new(m: u32, n: u32) -> Self {
		Self {
			address: AddressState::Pending,
			descriptor: UnboundedBytes::default(),
			pub_keys: BoundedBTreeMap::new(),
			m,
			n,
		}
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
		self.pub_keys.values().into_iter().any(|x| x == pub_key)
	}

	pub fn is_authority_submitted(&self, authority_id: &AccountId) -> bool {
		self.pub_keys.contains_key(authority_id)
	}

	pub fn set_address(&mut self, address: BoundedBitcoinAddress) {
		self.address = AddressState::Generated(address)
	}

	pub fn set_descriptor(&mut self, descriptor: UnboundedBytes) {
		self.descriptor = descriptor
	}

	pub fn pub_keys(&self) -> Vec<Public> {
		self.pub_keys.values().cloned().collect()
	}

	pub fn clear_pub_keys(&mut self) {
		self.pub_keys = BoundedBTreeMap::new();
	}

	pub fn replace_authority(&mut self, old: &AccountId, new: &AccountId) {
		if let Some(key) = self.pub_keys.remove(old) {
			self.pub_keys
				.try_insert(new.clone(), key)
				.expect("Should not fail as we just removed an element");
		}
	}
}

#[derive(Eq, PartialEq, Decode, Encode, TypeInfo, MaxEncodedLen)]
/// The vault address state.
pub enum AddressState {
	/// Required number of public keys has not been submitted yet.
	Pending,
	/// n public keys has been submitted and address generation done.
	Generated(BoundedBitcoinAddress),
}

#[derive(
	Eq,
	PartialEq,
	Ord,
	PartialOrd,
	Copy,
	Clone,
	Encode,
	Decode,
	RuntimeDebug,
	TypeInfo,
	MaxEncodedLen,
	Default,
)]
/// Sequence of migrating registration pool.
pub enum MigrationSequence {
	/// Normal sequence.
	#[default]
	Normal,
	/// Progress relay executive member update (if required).
	SetExecutiveMembers,
	/// Prepare next system vault.
	PrepareNextSystemVault,
	/// Wait till all UTXOs transferred to the new system vault.
	UTXOTransfer,
}
