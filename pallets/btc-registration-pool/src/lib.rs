#![cfg_attr(not(feature = "std"), no_std)]

mod pallet;
pub mod weights;

pub use pallet::pallet::*;
use weights::WeightInfo;

use sp_core::{ConstU32, RuntimeDebug};
use sp_runtime::BoundedVec;
use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

/// The maximum length of a valid Bitcoin address in characters (~62 alphanumeric characters).
pub const ADDRESS_MAX_LENGTH: u32 = 62;

/// The Bitcoin address type (length bounded).
pub type BoundedBitcoinAddress = BoundedVec<u8, ConstU32<ADDRESS_MAX_LENGTH>>;

#[derive(Decode, Encode, TypeInfo)]
/// A m-of-n multi signature based Bitcoin address.
pub struct MultiSigAccount<AccountId> {
	/// The vault address.
	pub address: AddressState,
	/// Public keys that the vault address contains.
	pub pub_keys: BTreeMap<AccountId, [u8; 33]>,
	/// The m value of the multi-sig address.
	pub m: u8,
	/// The n value of the multi-sig address.
	pub n: u8,
}

impl<AccountId: PartialEq + Clone + Ord> MultiSigAccount<AccountId> {
	pub fn new<T: Config>() -> Self {
		Self {
			address: AddressState::Pending,
			pub_keys: Default::default(),
			m: <RequiredM<T>>::get(),
			n: <RequiredN<T>>::get(),
		}
	}
}

#[derive(Decode, Encode, TypeInfo)]
/// The vault address state.
pub enum AddressState {
	/// Required number of public keys has not been submitted yet.
	Pending,
	/// n public keys has been submitted and address generation done.
	Generated(BoundedBitcoinAddress),
}

#[derive(Decode, Encode, TypeInfo)]
/// The registered Bitcoin relay target information.
pub struct BitcoinRelayTarget<AccountId> {
	/// For outbound.
	pub refund_address: BoundedBitcoinAddress,
	/// For inbound.
	pub vault: MultiSigAccount<AccountId>,
}

impl<AccountId: PartialEq + Clone + Ord> BitcoinRelayTarget<AccountId> {
	pub fn new<T: Config>(refund_address: BoundedBitcoinAddress) -> Self {
		Self { refund_address, vault: MultiSigAccount::new::<T>() }
	}

	pub fn is_pending(&self) -> bool {
		matches!(self.vault.address, AddressState::Pending)
	}

	pub fn is_generation_ready<T: Config>(&self) -> bool {
		<RequiredN<T>>::get() as usize == self.vault.pub_keys.len()
	}

	pub fn is_key_submitted(&self, pub_key: &[u8; 33]) -> bool {
		self.vault
			.pub_keys
			.values()
			.cloned()
			.collect::<Vec<[u8; 33]>>()
			.contains(pub_key)
	}

	pub fn is_authority_submitted(&self, authority_id: &AccountId) -> bool {
		self.vault.pub_keys.contains_key(authority_id)
	}

	pub fn insert_pub_key(&mut self, authority_id: AccountId, pub_key: [u8; 33]) {
		self.vault.pub_keys.insert(authority_id, pub_key);
	}

	pub fn set_vault_address(&mut self, address: BoundedBitcoinAddress) {
		self.vault.address = AddressState::Generated(address);
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
	pub pub_key: [u8; 33],
}
