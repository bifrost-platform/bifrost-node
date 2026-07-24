#![cfg_attr(not(feature = "std"), no_std)]

mod pallet;

pub use pallet::pallet::*;

use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::{ConstU32, H160, U256};
use sp_runtime::{BoundedBTreeMap, BoundedVec, RuntimeDebug};

// ---------------------------------------------------------------------------
// Primitive type aliases / constants
// ---------------------------------------------------------------------------

/// Product identifier. Same convention as the old pallet-pools' `PoolId`.
pub type ProductId = u64;

/// Maximum number of tranches per product. Carried over from pallet-pools' `MAX_TRANCHES`
/// (each tranche entry there mapped 1:1 to what's now a `Tranche` here).
pub const MAX_TRANCHES: u32 = 10;

/// Maximum number of individual (single-yield-source) Adapters per product.
pub const MAX_ADAPTERS: u32 = 20;

/// Maximum number of MultichainAdapter routing entries per product.
pub const MAX_MULTICHAIN_ADAPTERS: u32 = 20;

/// Maximum number of collateral NFTs per (OffchainSource) Adapter.
/// Carried over from pallet-pools' `MAX_COLLATERALS`, now scoped per-adapter
/// instead of per-pool since a product can mix multiple offchain sources.
pub const MAX_COLLATERALS: u32 = 10;

// ---------------------------------------------------------------------------
// VaultId — tranche identity
// ---------------------------------------------------------------------------

/// Identifies a tranche within a product: the EVM chain where its ERC-7540 vault
/// is deployed, paired with the vault contract address on that chain.
/// Globally unique across ALL products, mirroring pallet-pools' `TrancheId`.
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
pub struct VaultId {
	/// EVM chain ID of the chain where the vault contract is deployed.
	pub chain_id: u64,
	/// ERC-7540 vault contract address on that chain.
	pub vault_address: H160,
}

// ---------------------------------------------------------------------------
// AdapterKey — adapter / multichain-adapter identity
// ---------------------------------------------------------------------------

/// Identifies an Adapter or MultichainAdapter entry: its own address paired with
/// the EVM chain it lives on. Same shape used for both registries (see
/// `ProductDetails`) — they're separate namespaces, but the key shape is
/// identical, so one type covers both. Globally unique across ALL products,
/// mirroring pallet-pools' `CollateralAsset` uniqueness convention.
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
pub struct AdapterKey {
	/// The adapter's (or MultichainAdapter's) own contract/source address.
	pub address: H160,
	/// EVM chain ID where that address lives.
	pub chain_id: u64,
}

// ---------------------------------------------------------------------------
// TrancheType
// ---------------------------------------------------------------------------

/// Unlike pallet-pools' `TrancheType`, this carries no derived on-chain accrual
/// rate — pricing/waterfall math is never computed on-node anymore (the Hub
/// Valuation Contract does it off-chain), so `apr` here is stored config only,
/// not a basis for on-chain compounding.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen,
)]
pub enum TrancheType {
	/// Residual (junior) tranche — no fixed APR, receives the waterfall residual.
	Junior,
	/// Senior tranche — fixed APR entitlement.
	Senior {
		/// Nominal annual rate as a FixedU128 inner value (1e18 = 100%).
		apr: U256,
	},
}

// ---------------------------------------------------------------------------
// Tranche
// ---------------------------------------------------------------------------

/// A single tranche within a product.
///
/// Deliberately has NO explicit `priority` field — priority is represented by
/// this entry's position within `ProductDetails::tranches` (a `BoundedVec`).
/// `set_tranche`'s insert-and-shift semantics (see interface.sol) map directly
/// onto `Vec::insert`/`Vec::remove` at the target position, so there's no
/// separate ordering value to keep in sync.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen,
)]
pub struct Tranche {
	pub tranche_type: TrancheType,
	pub vault: VaultId,
}

// ---------------------------------------------------------------------------
// SourceType / AdapterInfo
// ---------------------------------------------------------------------------

/// Generic over `AccountId` because `OffchainSource` carries `borrower` — unlike
/// `product_admin` (owned entirely by the permissions pallet, see
/// `PermissionInspect`), `borrower` lives directly on the adapter here. A
/// product can have multiple OffchainSource adapters, each potentially a
/// different institution, so there's no single product-scoped "Borrower" role
/// to delegate to — this is the source of truth instead.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen,
)]
pub enum SourceType<AccountId> {
	/// Backed by an off-chain RWA loan book. Borrow/repay bookkeeping for
	/// that loan book is NOT tracked on-chain (2026-07-24) — it lives
	/// entirely in the adapter itself off-chain, since nothing on-chain
	/// consumes it once NAV stopped being computed on-node. `borrower` here
	/// is adapter metadata only (identity, not a ledger).
	OffchainSource {
		borrower: AccountId,
		collaterals: BoundedVec<CollateralAsset, ConstU32<MAX_COLLATERALS>>,
	},
	/// Backed by an on-chain yield protocol (e.g. Compound, Morpho, Aave).
	OnchainSource,
}

/// NFT collateral backing an OffchainSource adapter's loan book.
/// Same shape as pallet-pools' `CollateralAsset`, now scoped per-adapter.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen,
)]
pub struct CollateralAsset {
	/// ERC-721 / ERC-1155 contract address on Bifrost EVM.
	pub nft_contract: H160,
	/// Token ID identifying the specific NFT.
	pub nft_token_id: U256,
}

// ---------------------------------------------------------------------------
// MultichainAdapterInfo
// ---------------------------------------------------------------------------

/// A single MultichainAdapter routing entry: just its allocation weight (as a
/// FixedU128 inner value, 1e18 = 100%) — address/chain_id already live in the
/// `AdapterKey` this is mapped from, so a tuple struct is enough. Across one
/// product's full `multichain_adapters` list, these must always sum to exactly
/// 1e18 — see `set_multichain_adapters` in interface.sol for why this is a
/// full-array replace rather than single-entity add/remove/update.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen,
)]
pub struct MultichainAdapterInfo(pub U256);

// ---------------------------------------------------------------------------
// ValuationInfo
// ---------------------------------------------------------------------------

#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen,
)]
pub struct ValuationInfo {
	/// Hub-chain Valuation contract address for this product. This pallet's
	/// authorization check for pallet-tranche-investments-style calls compares
	/// the caller against this address (see interface.sol notes — no Gateway
	/// in that call path).
	pub valuation_address: H160,
	/// Settlement interval length in seconds. Admin-set; recommended to be at
	/// least the GCD of the underlying yield sources' epochs. Read by
	/// pallet-auto-pilot to schedule `Valuation.tryUpdateNAV()` calls.
	pub settlement_length_secs: u64,
	/// Window (in seconds) within each interval during which pallet-auto-pilot
	/// repeatedly retries `Valuation.tryUpdateNAV()` to trigger settlement.
	pub settlement_offset_secs: u64,
}

// ---------------------------------------------------------------------------
// ProductDetails
// ---------------------------------------------------------------------------

/// Generic over `AccountId` (via `SourceType`, see its doc comment) — unlike the
/// old design, this pallet now stores adapter `borrower`s directly rather than
/// delegating them to the permissions pallet. `product_admin` is still NOT
/// stored here, though — it remains fully owned by pallet-tranche-permissions
/// (see `PermissionInspect::is_product_admin`), since there's exactly one
/// ProductAdmin per product and no ambiguity about where it belongs, unlike
/// `borrower` which only makes sense attached to a specific adapter.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen,
)]
pub struct ProductDetails<AccountId> {
	pub valuation: ValuationInfo,
	/// Ordered by waterfall priority — index 0 is highest priority. See
	/// `Tranche`'s doc comment for why there's no separate `priority` field.
	pub tranches: BoundedVec<Tranche, ConstU32<MAX_TRANCHES>>,
	pub adapters: BoundedBTreeMap<AdapterKey, SourceType<AccountId>, ConstU32<MAX_ADAPTERS>>,
	/// Always replaced wholesale by `set_multichain_adapters` — see that
	/// function's doc comment in interface.sol for why (100%-sum invariant).
	pub multichain_adapters:
		BoundedBTreeMap<AdapterKey, MultichainAdapterInfo, ConstU32<MAX_MULTICHAIN_ADAPTERS>>,
}

// ---------------------------------------------------------------------------
// Traits
// ---------------------------------------------------------------------------

/// Implemented by pallet-tranche-permissions. Called by pallet-tranche-system
/// to gate-check ProductAdmin (mirrors pallet-pools' `PermissionInspect`, minus
/// `grant_borrower` — there's no product-scoped Borrower role anymore; borrower
/// identity lives directly on each OffchainSource adapter instead, see
/// `SourceType`).
pub trait PermissionInspect<AccountId> {
	/// Returns `true` if `who` holds the ProductAdmin role for `product_id`.
	fn is_product_admin(product_id: ProductId, who: &AccountId) -> bool;
}

// ---------------------------------------------------------------------------
// ProductAdmin origin
// ---------------------------------------------------------------------------

/// `EnsureOrigin` that accepts only the `ProductAdmin` pallet origin.
/// The tranche-system precompile creates this origin before dispatching to
/// `create_product`, guaranteeing it can't be called via a plain signed
/// extrinsic — mirrors pallet-pools' `EnsurePoolAdmin`.
/// Wire as `type ProductAdminOrigin = pallet_tranche_system::EnsureProductAdmin`
/// in the runtime.
pub struct EnsureProductAdmin;

impl<OuterOrigin> frame_support::traits::EnsureOrigin<OuterOrigin> for EnsureProductAdmin
where
	OuterOrigin: Into<Result<Origin, OuterOrigin>> + From<Origin>,
{
	type Success = ();
	fn try_origin(o: OuterOrigin) -> Result<Self::Success, OuterOrigin> {
		match o.into() {
			Ok(Origin::ProductAdmin) => Ok(()),
			Err(o) => Err(o),
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin() -> Result<OuterOrigin, ()> {
		Ok(OuterOrigin::from(Origin::ProductAdmin))
	}
}
