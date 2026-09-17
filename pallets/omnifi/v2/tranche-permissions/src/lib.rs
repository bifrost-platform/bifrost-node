#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(all(test, feature = "runtime-benchmarks"))]
mod mock;
mod pallet;
pub mod weights;

pub use pallet::pallet::*;
pub use weights::WeightInfo;

#[cfg(feature = "runtime-benchmarks")]
use pallet_tranche_system::ProductId;
use pallet_tranche_system::VaultId;
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;

/// Benchmark-only setup hook.
///
/// The `TrancheInvestor` grant/revoke benchmarks need `T::Vaults`/`T::Products`
/// (pallet-tranche-system, reached through trait objects — this pallet has no
/// direct `pallet_tranche_system::Config` bound) to report the benchmark's
/// vault as belonging to a `Multichain` product, so the extrinsic runs its
/// full body instead of failing early in
/// `ensure_tranche_investor_vault_registered`.
///
/// The runtime wires this to an implementation that writes
/// pallet-tranche-system's `Vaults` map directly; `()` is a no-op, which is all
/// the mock needs (its `VaultInspect` already returns `true` unconditionally).
#[cfg(feature = "runtime-benchmarks")]
pub trait BenchmarkHelper {
	/// Register `vault` under `product_id` as an active tranche of a
	/// `Multichain` product.
	fn setup_multichain_vault(product_id: ProductId, vault: VaultId);
}

#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkHelper for () {
	fn setup_multichain_vault(_: ProductId, _: VaultId) {}
}

// ---------------------------------------------------------------------------
// Role
// ---------------------------------------------------------------------------

/// A permission role scoped to a product.
///
/// Cardinality per product (confirmed 2026-07-24):
/// - `ProductAdmin` — exactly one. Pre-granted by sudo before `create_product` is ever called (see
///   pallet-tranche-system's `create_product` flow).
/// - `OracleFeeder` — many.
/// - `TrancheInvestor` — many, scoped to a specific tranche (`VaultId`), not the whole product.
///
/// No `Borrower` variant: a product can have multiple OffchainSource
/// adapters, each potentially a different institution, so there's no single
/// product-scoped borrower to represent here. Borrower identity lives
/// directly on each adapter instead — see
/// `pallet_tranche_system::SourceType::OffchainSource { borrower, .. }`.
#[derive(
	Clone,
	Encode,
	Decode,
	DecodeWithMemTracking,
	PartialEq,
	Eq,
	Ord,
	PartialOrd,
	RuntimeDebug,
	TypeInfo,
	MaxEncodedLen,
)]
pub enum Role {
	/// May manage this product: its tranches, adapters, and sub-roles.
	/// Granted by sudo before the product is created.
	ProductAdmin,
	/// May submit NAV updates for the product. Not currently gated by any
	/// on-chain extrinsic — reserved for a future on-chain NAV-feeding
	/// mechanism; today NAV reaches Valuation entirely off-chain/externally.
	OracleFeeder,
	/// May submit deposit/redeem requests for a specific tranche.
	TrancheInvestor(VaultId),
}
