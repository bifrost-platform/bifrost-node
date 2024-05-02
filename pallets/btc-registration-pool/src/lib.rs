#![cfg_attr(not(feature = "std"), no_std)]

mod pallet;
pub mod weights;

pub use pallet::pallet::*;
use weights::WeightInfo;

use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_core::RuntimeDebug;

use bp_multi_sig::{BoundedBitcoinAddress, MultiSigAccount, Public};

pub const ADDRESS_U64: u64 = 256;

#[derive(Decode, Encode, TypeInfo)]
/// The registered Bitcoin relay target information.
pub struct BitcoinRelayTarget<AccountId> {
	/// For outbound.
	pub refund_address: BoundedBitcoinAddress,
	/// For inbound.
	pub vault: MultiSigAccount<AccountId>,
}

impl<AccountId: PartialEq + Clone + Ord> BitcoinRelayTarget<AccountId> {
	pub fn new<T: Config>(refund_address: BoundedBitcoinAddress, m: u32, n: u32) -> Self {
		Self { refund_address, vault: MultiSigAccount::new(m, n) }
	}

	pub fn set_vault_address(&mut self, address: BoundedBitcoinAddress) {
		self.vault.set_address(address)
	}

	pub fn set_refund_address(&mut self, address: BoundedBitcoinAddress) {
		self.refund_address = address;
	}
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
/// The payload used for public key submission.
pub struct VaultKeySubmission<AccountId> {
	/// The authority Ethereum address. (Relay executive)
	pub authority_id: AccountId,
	/// The target Ethereum address.
	pub who: AccountId,
	/// The generated public key. (33 bytes)
	pub pub_key: Public,
}
