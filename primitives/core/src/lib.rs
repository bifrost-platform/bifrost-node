#![cfg_attr(not(feature = "std"), no_std)]

use fp_account::EthereumSignature;

pub use sp_runtime::OpaqueExtrinsic;
use sp_runtime::{
	generic,
	traits::{BlakeTwo256, IdentifyAccount, Verify},
};

/// Block type as expected by this runtime.
pub type Block = generic::Block<Header, OpaqueExtrinsic>;

/// Alias to 512-bit hash when used in the context of a transaction signature on the chain.
pub type Signature = EthereumSignature;

/// Some way of identifying an account on the chain. We intentionally make it equivalent
/// to the public key of our transaction signing scheme.
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

/// Balance of an account.
pub type Balance = u128;

/// An index to a block.
pub type BlockNumber = u32;

/// A hash of some data used by the chain.
pub type Hash = sp_core::H256;

/// Block header type as expected by this runtime.
pub type Header = generic::Header<BlockNumber, BlakeTwo256>;

/// Index of a transaction in the chain.
pub type Nonce = u32;

/// The address format for describing accounts.
pub type Address = AccountId;
