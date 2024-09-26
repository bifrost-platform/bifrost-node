#![cfg_attr(not(feature = "std"), no_std)]

pub mod migrations;
mod pallet;
pub mod weights;

pub use pallet::pallet::*;
use weights::WeightInfo;

use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_core::RuntimeDebug;
use sp_std::vec::Vec;

use bp_multi_sig::{BoundedBitcoinAddress, MultiSigAccount, Public};

pub const ADDRESS_U64: u64 = 256;

pub(crate) const LOG_TARGET: &'static str = "runtime::registration-pool";

// syntactic sugar for logging.
#[macro_export]
macro_rules! log {
	($level:tt, $patter:expr $(, $values:expr)* $(,)?) => {
		log::$level!(
			target: crate::LOG_TARGET,
			concat!("[{:?}] 💸 ", $patter), <frame_system::Pallet<T>>::block_number() $(, $values)*
		)
	};
}

/// The round of the registration pool.
pub type PoolRound = u32;

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
	/// The pool round.
	pub pool_round: PoolRound,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
/// The payload used for public key submission.
pub struct VaultKeyPreSubmission<AccountId> {
	/// The authority Ethereum address. (Relay executive)
	pub authority_id: AccountId,
	/// The public keys. (all in 33 bytes)
	pub pub_keys: Vec<Public>,
	/// The pool round.
	pub pool_round: PoolRound,
}
