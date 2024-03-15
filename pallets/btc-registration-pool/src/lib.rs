#![cfg_attr(not(feature = "std"), no_std)]

mod pallet;
pub mod weights;

pub use pallet::pallet::*;
use weights::WeightInfo;

use sp_core::{ConstU32, RuntimeDebug};
use sp_runtime::{BoundedBTreeMap, BoundedVec, DispatchError};
use sp_std::vec::Vec;

use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

/// The maximum length of a valid Bitcoin address in characters (~62 alphanumeric characters).
pub const ADDRESS_MAX_LENGTH: u32 = 62;

/// The maximum amount of accounts a multi-sig account can consist.
pub const MULTI_SIG_MAX_ACCOUNTS: u32 = 16;

/// The Bitcoin address type (length bounded).
pub type BoundedBitcoinAddress = BoundedVec<u8, ConstU32<ADDRESS_MAX_LENGTH>>;

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
	pub fn new<T: Config>() -> Self {
		Self {
			address: AddressState::Pending,
			pub_keys: BoundedBTreeMap::new(),
			m: <RequiredM<T>>::get(),
			n: <RequiredN<T>>::get(),
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

	pub fn is_generation_ready<T: Config>(&self) -> bool {
		<RequiredN<T>>::get() as usize == self.pub_keys.len()
	}

	pub fn is_key_submitted(&self, pub_key: &Public) -> bool {
		self.pub_keys.values().cloned().collect::<Vec<Public>>().contains(pub_key)
	}

	pub fn is_authority_submitted(&self, authority_id: &AccountId) -> bool {
		self.pub_keys.contains_key(authority_id)
	}

	pub fn insert_pub_key<T: Config>(
		&mut self,
		authority_id: AccountId,
		pub_key: Public,
	) -> Result<(), DispatchError> {
		self.pub_keys
			.try_insert(authority_id, pub_key)
			.map_err(|_| Error::<T>::OutOfRange)?;
		Ok(())
	}

	pub fn set_address(&mut self, address: BoundedBitcoinAddress) {
		self.address = AddressState::Generated(address)
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

	pub fn set_vault_address(&mut self, address: BoundedBitcoinAddress) {
		self.vault.set_address(address)
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
	pub pub_key: Public,
}
