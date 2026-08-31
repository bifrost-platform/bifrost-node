#![cfg_attr(not(feature = "std"), no_std)]

//! Primitives shared across the OmniFi tranche-system pallets
//! (`pallet-tranche-system`, `pallet-tranche-tx-registry`,
//! `pallet-tranche-custom-flows`).
//!
//! `TxRecord` / `ChainId` were originally declared inside
//! `pallet-tranche-tx-registry`, and `ProductId` inside `pallet-tranche-system`;
//! each was lifted here verbatim (same underlying type / field order / derives)
//! when a second pallet came to need it, so every consumer reads one
//! definition. The SCALE encoding is byte-identical to the pre-move layout —
//! moving a type between crates changes only its `TypeInfo` metadata path
//! (runtime metadata, not consensus), never its `Encode`/`Decode` bytes — so
//! the originating pallets' existing storage needs no migration. Each
//! originating pallet re-exports its moved type via `pub use bp_tranche::{...}`
//! so every in-crate reference keeps working.

use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::H256;
use sp_runtime::RuntimeDebug;

/// EVM chain ID. Bare `u64`, matching `pallet_tranche_system::VaultId::chain_id`.
pub type ChainId = u64;

/// Tranche-system product identifier. Bare `u64`; re-exported by
/// `pallet-tranche-system` (its canonical home) and used directly by the
/// tx-evidence pallets, which key storage by it but validate nothing against
/// tranche-system.
pub type ProductId = u64;

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
	/// EVM chain ID the tx occurred on.
	pub chain_id: ChainId,
	/// Transaction hash on that chain.
	pub tx_hash: H256,
	/// This chain's own block number when the attestation was accepted.
	pub recorded_at: BlockNumber,
}
