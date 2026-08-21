#![cfg_attr(not(feature = "std"), no_std)]

mod pallet;
pub mod weights;

pub use pallet::pallet::*;
pub use weights::WeightInfo;

use pallet_tranche_system::VaultId;
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;

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
