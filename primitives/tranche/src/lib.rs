#![cfg_attr(not(feature = "std"), no_std)]

//! Primitives shared across the OmniFi tranche-system pallets
//! (`pallet-tranche-system`, `pallet-tranche-tx-registry`,
//! `pallet-tranche-custom-flows`).
//!
//! `TxRecord` / `ChainId` were originally declared inside
//! `pallet-tranche-tx-registry`, and `ProductId` inside `pallet-tranche-system`;
//! each was lifted here verbatim (same underlying type / field order / derives)
//! when a second pallet came to need it, so every consumer reads one
//! definition. Each originating pallet re-exports its moved type via
//! `pub use bp_tranche::{...}` so every in-crate reference keeps working.
//!
//! # Chain-agnostic addresses and tx hashes
//!
//! Products can span non-EVM chains (e.g. Solana, Stellar), so every address that
//! lives on a product/spoke chain is a [`ChainAddress`] (32 bytes, EVM addresses
//! left-padded — the same "universal address" convention CCTP/Wormhole/LayerZero
//! use), and every foreign-chain tx identifier is a [`TxHash`] (variable length,
//! up to 64 bytes — a Solana tx signature is 64 bytes, EVM/Stellar hashes 32).
//! Addresses local to the Hub (Bifrost EVM) stay `H160`/`AccountId`.
//!
//! The pre-change shapes live on in [`legacy`] for the frozen, EVM-only v1 pallets.

use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::{ConstU32, H160, H256};
use sp_runtime::{BoundedVec, RuntimeDebug};

pub mod history;

/// Chain identifier. EVM chains use their EVM chain ID; non-EVM chains use the
/// numbering agreed off-chain (the pallets never interpret it beyond equality).
/// Bare `u64`, matching `pallet_tranche_system::VaultId::chain_id`.
pub type ChainId = u64;

/// Tranche-system product identifier. Bare `u64`; re-exported by
/// `pallet-tranche-system` (its canonical home) and used directly by the
/// tx-evidence pallets, which key storage by it but validate nothing against
/// tranche-system.
pub type ProductId = u64;

/// An address on a product/spoke chain, as 32 bytes. EVM addresses are stored
/// left-padded with 12 zero bytes (see [`from_evm`]); 32-byte native addresses
/// (Solana, Stellar `G…`/`C…`, Aptos, Sui, …) are stored as-is. Always paired
/// with a [`ChainId`] that tells the reader how to interpret it.
pub type ChainAddress = H256;

/// Maximum length, in bytes, of a [`TxHash`] — a Solana tx signature (64 bytes)
/// is the longest identifier among the supported chains.
pub const MAX_TX_HASH_LEN: u32 = 64;

/// A tx identifier on some chain, in that chain's native byte form (32 bytes
/// for EVM/Stellar, 64 for Solana). Validate with [`is_valid_tx_hash`].
pub type TxHash = BoundedVec<u8, ConstU32<MAX_TX_HASH_LEN>>;

/// Left-pads an EVM address into a [`ChainAddress`].
pub fn from_evm(address: H160) -> ChainAddress {
	address.into()
}

/// The EVM address `address` encodes, if it's EVM-shaped (top 12 bytes zero).
pub fn as_evm(address: &ChainAddress) -> Option<H160> {
	address.as_bytes()[..12]
		.iter()
		.all(|b| *b == 0)
		.then(|| H160::from_slice(&address.as_bytes()[12..]))
}

/// A recorded tx hash must be non-empty and not all zeros (the zero value is the
/// precompile-side "unset" sentinel).
pub fn is_valid_tx_hash(tx_hash: &TxHash) -> bool {
	tx_hash.iter().any(|b| *b != 0)
}

/// Stored off-chain tx evidence, accepted from a trusted recorder's attestation.
///
/// `recorded_at` is **this** chain's own block number when the attestation was
/// accepted — not the block number on the chain where the tx actually occurred.
/// `pallet-tranche-tx-registry` additionally treats `recorded_at == 0` as an
/// "unset" sentinel only at its Solidity precompile boundary; on the Rust side
/// "not yet recorded" is `Option<TxRecord<_>>` at every storage/query site.
#[derive(
	Clone,
	Encode,
	Decode,
	DecodeWithMemTracking,
	PartialEq,
	Eq,
	RuntimeDebug,
	TypeInfo,
	MaxEncodedLen,
)]
pub struct TxRecord<BlockNumber> {
	/// Chain the tx occurred on.
	pub chain_id: ChainId,
	/// Tx identifier on that chain, in its native byte form.
	pub tx_hash: TxHash,
	/// This chain's own block number when the attestation was accepted.
	pub recorded_at: BlockNumber,
}

/// Pre-non-EVM shapes, kept byte-identical (SCALE) to what the frozen, EVM-only
/// v1 pallets (`pallet-tranche-investments`, `pallet-tranche-tx-registry`) have
/// in storage, so those pallets need no migration.
pub mod legacy {
	use super::*;

	/// The original `TxRecord` (EVM-only, 32-byte hash).
	#[derive(
		Clone,
		Encode,
		Decode,
		DecodeWithMemTracking,
		PartialEq,
		Eq,
		RuntimeDebug,
		TypeInfo,
		MaxEncodedLen,
	)]
	pub struct EvmTxRecord<BlockNumber> {
		/// EVM chain ID the tx occurred on.
		pub chain_id: ChainId,
		/// Transaction hash on that chain.
		pub tx_hash: H256,
		/// This chain's own block number when the attestation was accepted.
		pub recorded_at: BlockNumber,
	}

	impl<BlockNumber> From<EvmTxRecord<BlockNumber>> for TxRecord<BlockNumber> {
		fn from(record: EvmTxRecord<BlockNumber>) -> Self {
			TxRecord {
				chain_id: record.chain_id,
				tx_hash: BoundedVec::truncate_from(record.tx_hash.as_bytes().to_vec()),
				recorded_at: record.recorded_at,
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn evm_address_round_trips() {
		let evm = H160::repeat_byte(0xab);
		let addr = from_evm(evm);
		assert_eq!(&addr.as_bytes()[..12], &[0u8; 12]);
		assert_eq!(as_evm(&addr), Some(evm));
		assert_eq!(as_evm(&H256::repeat_byte(0x01)), None);
	}

	#[test]
	fn tx_hash_validity() {
		assert!(!is_valid_tx_hash(&TxHash::default()));
		assert!(!is_valid_tx_hash(&BoundedVec::truncate_from(vec![0u8; 32])));
		assert!(is_valid_tx_hash(&BoundedVec::truncate_from(vec![1u8; 64])));
	}

	#[test]
	fn legacy_tx_record_converts() {
		let legacy =
			legacy::EvmTxRecord { chain_id: 1, tx_hash: H256::repeat_byte(7), recorded_at: 5u32 };
		let record: TxRecord<u32> = legacy.into();
		assert_eq!(record.tx_hash.as_slice(), H256::repeat_byte(7).as_bytes());
	}
}
