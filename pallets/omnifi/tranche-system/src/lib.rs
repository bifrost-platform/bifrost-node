#![cfg_attr(not(feature = "std"), no_std)]

pub mod migrations;
mod pallet;
pub mod weights;

pub use pallet::pallet::*;
pub use weights::WeightInfo;

use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::{ConstU32, H160, U256};
use sp_runtime::{BoundedBTreeMap, BoundedVec, RuntimeDebug};
use sp_std::marker::PhantomData;

// ---------------------------------------------------------------------------
// Primitive type aliases / constants
// ---------------------------------------------------------------------------

/// Product identifier. Defined in the shared `bp-tranche` crate (so the
/// tx-evidence pallets can key storage by it without depending on this pallet)
/// and re-exported here as the canonical `pallet_tranche_system::ProductId`.
/// The move is SCALE-encoding-neutral (crate path only affects `TypeInfo`
/// metadata) — this pallet's existing storage needs no migration.
pub use bp_tranche::ProductId;

/// Maximum number of tranches a single chain within a product can have.
/// Originally a flat, product-wide cap; rescoped (2026-08-20) to apply per
/// chain instead, once waterfall priority ordering (and the
/// Senior-before-Junior invariant) became chain-scoped rather than
/// product-wide — see `MultichainProductDetails::tranches`'
/// doc comment for why cross-chain tranche ordering was never a meaningful
/// comparison to begin with (each chain's tranches only ever compete against
/// each other for that chain's own waterfall).
pub const MAX_TRANCHES_PER_CHAIN: u32 = 10;

/// Maximum number of distinct chains a single product's tranches can span —
/// bounds `MultichainProductDetails::tranches`' outer map (one entry per
/// chain with at least one tranche). A `SingleChainProductDetails` needs no
/// equivalent bound — it's structurally already exactly one chain.
pub const MAX_TRANCHE_CHAINS: u32 = 10;

/// Maximum number of `TrancheInput` entries `create_product` accepts in one
/// call — the flat input array spans every chain's tranches at once (grouped
/// by `vault.chain_id` during validation, see `create_product`'s doc
/// comment), so this must cover the worst case of every one of a product's
/// chains (`MAX_TRANCHE_CHAINS`) each at its own per-chain cap
/// (`MAX_TRANCHES_PER_CHAIN`). `create_single_chain_product`'s own `tranches` input
/// stays bounded by `MAX_TRANCHES_PER_CHAIN` alone — inherently one chain, no fan-out.
pub const MAX_TRANCHE_INPUTS: u32 = MAX_TRANCHES_PER_CHAIN * MAX_TRANCHE_CHAINS;

/// Maximum number of MultichainAdapter routing entries per product.
pub const MAX_MULTICHAIN_ADAPTERS: u32 = 10;

/// Maximum number of individual (single-yield-source) Adapters per
/// MultichainAdapter. Rescoped from per-product to per-MultichainAdapter
/// (2026-07-27): adapters now live nested under their parent MultichainAdapter
/// (see `MultichainAdapterInfo`) instead of in a flat, product-wide registry.
pub const MAX_ADAPTERS_PER_MULTICHAIN_ADAPTER: u32 = 10;

/// Worst-case total number of individual Adapters a Multichain product can
/// have, summed across every one of its MultichainAdapter entries — every one
/// of up to `MAX_MULTICHAIN_ADAPTERS` entries nesting up to
/// `MAX_ADAPTERS_PER_MULTICHAIN_ADAPTER` individual Adapters each. Same
/// "per-group cap times group count" pattern as `MAX_TRANCHE_INPUTS`. For
/// anything that needs a single product-wide bound on individual Adapters
/// (flattened across all of a product's MultichainAdapters) —
/// `MAX_MULTICHAIN_ADAPTERS` alone underbounds this, since it only counts the
/// top-level routing entries, not what's nested inside each one.
pub const MAX_TOTAL_ADAPTERS: u32 = MAX_MULTICHAIN_ADAPTERS * MAX_ADAPTERS_PER_MULTICHAIN_ADAPTER;

/// Maximum number of per-chain TrancheManager bindings per product (see
/// `MultichainProductDetails::multichain_tranche_managers`) — one entry per
/// chain that has at least one tranche, so this is bounded by the same
/// `MAX_TRANCHE_CHAINS` a product's tranches themselves are (not an
/// independent cap — a `multichain_tranche_managers` entry can never outnumber
/// the distinct tranche chains it's binding TrancheManagers for, since
/// `set_multichain_tranche_managers` rejects any `chain_id` without a tranche
/// on it).
pub const MAX_TRANCHE_MANAGERS: u32 = MAX_TRANCHE_CHAINS;

/// Maximum number of individual Adapters per single-chain product (see
/// `SingleChainProductDetails::adapters`) — a flat list, unlike
/// `MAX_ADAPTERS_PER_MULTICHAIN_ADAPTER`'s per-MultichainAdapter nesting,
/// since a single-chain product has no MultichainAdapter wrapper at all.
/// Same bound as `MAX_ADAPTERS_PER_MULTICHAIN_ADAPTER` — no reason to differ.
pub const MAX_ADAPTERS_PER_SINGLE_CHAIN_PRODUCT: u32 = 10;

/// Maximum number of collateral NFTs per (OffchainSource) Adapter. Scoped
/// per-adapter since a product can mix multiple offchain sources.
pub const MAX_COLLATERALS: u32 = 10;

/// Maximum number of requests that can be approved into a single
/// (product_id, settlement_id) cycle — bounds both
/// `pallet_tranche_investments`'s `record_investment_approval` batch input and
/// `pallet_tranche_tx_registry`'s `record_settlement_tx` `RequestsApproved`
/// attestation batch (the two pallets track the same settlement cycle
/// independently — see `pallet_tranche_tx_registry::RequestId`'s doc comment
/// for why tx-registry has no hard dependency on tranche-investments). Hosted
/// here, in the common dependency both pallets already share, rather than in
/// either sibling, so there's exactly one definition instead of two
/// independently-maintained copies that happen to agree. No equivalent bound
/// exists elsewhere in this pallet family — sized generously since it caps
/// "investors settled together in one cycle", not a per-product structural
/// count like `MAX_ALLOCATIONS`.
pub const MAX_SETTLEMENT_REQUESTS: u32 = 1_000;

// ---------------------------------------------------------------------------
// VaultId — tranche identity
// ---------------------------------------------------------------------------

/// Identifies a tranche within a product: the EVM chain where its ERC-7540 vault
/// is deployed, paired with the vault contract address on that chain. Once a
/// `VaultId` is ever registered to a product (see `Vaults`/`VaultRegistration`),
/// it's bound to that product permanently — removing it only tombstones the
/// registration, it never frees the `VaultId` for a *different* product to
/// claim.
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
// VaultRegistration
// ---------------------------------------------------------------------------

/// `Vaults`' value type (2026-08-26, `v6`) — a permanent record of which
/// product a `VaultId` was ever registered to, plus whether it's currently an
/// active tranche. `set_tranche(Remove)` only flips `removed` to `true`; it
/// never removes the storage entry itself, so `product_id` stays discoverable
/// (and, crucially, still occupies the key) forever. `set_tranche(Add)` on a
/// `VaultId` already in this map only succeeds if `product_id` matches — the
/// same product re-registering a vault it previously removed clears `removed`
/// back to `false`; any other product attempting to claim it reverts with
/// `Error::VaultBoundToDifferentProduct`. See `Vaults`' own storage doc
/// comment (pallet/mod.rs) for the full rationale.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen,
)]
pub struct VaultRegistration {
	/// The product this `VaultId` is permanently bound to — set once, at
	/// first registration, and never changed afterward.
	pub product_id: ProductId,
	/// `true` if this vault was removed from its product's tranche list
	/// (`set_tranche(Remove)`) and hasn't been re-added since.
	pub removed: bool,
}

// ---------------------------------------------------------------------------
// AdapterKey — adapter / multichain-adapter identity
// ---------------------------------------------------------------------------

/// Identifies a MultichainAdapter entry: its own address paired with the EVM
/// chain it lives on. Globally unique across ALL products.
///
/// Nested Adapters (see `AdapterInfo`) are NOT keyed by this type — they carry
/// no `chain_id` of their own (removed 2026-07-27; a nested adapter always
/// lives on its parent MultichainAdapter's chain), so `MultichainProductDetails`'s
/// nested `adapters` map is keyed by plain `H160` instead. `AdapterIndex`'s reverse-index
/// (see pallet/mod.rs) still uses this type, though — global adapter uniqueness
/// stays chain-aware (some on-chain protocols share the same contract address
/// across different chains via CREATE2), it's just derived from the parent
/// MultichainAdapter's `chain_id` at write time rather than stored on the
/// adapter itself.
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
// CrudAction
// ---------------------------------------------------------------------------

/// Discriminant for `set_tranche`'s unified add/remove/update mutation.
/// Mirrors interface.sol's `CrudAction`.
#[derive(
	Clone,
	Copy,
	Encode,
	Decode,
	DecodeWithMemTracking,
	PartialEq,
	Eq,
	RuntimeDebug,
	TypeInfo,
	MaxEncodedLen,
)]
pub enum CrudAction {
	Add,
	Remove,
	Update,
}

// ---------------------------------------------------------------------------
// TrancheType
// ---------------------------------------------------------------------------

/// Carries no derived on-chain accrual rate — pricing/waterfall math is never
/// computed on-node (the Hub Valuation Contract does it off-chain), so `apr`
/// here is stored config only, not a basis for on-chain compounding.
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
/// this entry's position within the owning product's *own chain's* tranche
/// list: `MultichainProductDetails::tranches[vault.chain_id]` (a per-chain
/// `BoundedVec`, one entry per chain that has at least one tranche) or
/// `SingleChainProductDetails::tranches` directly (already exactly one chain,
/// so no per-chain grouping needed there).
/// `set_tranche`'s insert-and-shift semantics (see interface.sol) map directly
/// onto `Vec::insert`/`Vec::remove` at the target position within that one
/// chain's own list, so there's no separate ordering value to keep in sync.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen,
)]
pub struct Tranche {
	pub tranche_type: TrancheType,
	pub vault: VaultId,
	/// The asset investors deposit when depositing into this tranche's vault —
	/// a token address on `vault.chain_id` (not necessarily the Hub chain, and
	/// not necessarily the same asset across different tranches of the same
	/// product). Distinct from `ValuationInfo::base_asset`, which is the
	/// Hub-chain asset NAV/pricing is denominated in.
	pub asset: H160,
	/// This tranche's own share-token contract address — the ERC-7540 vault's
	/// share token investors receive/burn on deposit/redeem, on `vault.chain_id`.
	/// Distinct from `asset` (what's deposited in) and `ValuationInfo::base_asset`
	/// (the Hub-chain pricing denomination).
	pub shares: H160,
}

/// One entry of `create_product`'s `tranches` input. Carries an explicit
/// `priority` (0 = highest) used once, at creation time, to sort the incoming
/// set into the owning product's `tranches`' final order — `priority` itself is
/// never persisted (see `Tranche`'s doc comment for why position alone
/// suffices after that).
///
/// `priority` is scoped to `vault.chain_id` — two entries on *different*
/// chains may freely share the same `priority` (each chain gets its own,
/// independent 0-indexed ordering), but two entries on the *same* chain may
/// not. `create_product` groups the incoming set by `vault.chain_id` first,
/// then, within each chain's own group, sorts by `priority` and requires all
/// `Senior` tranches to precede all `Junior` ones — reverts otherwise. This
/// invariant is deliberately per-chain, not product-wide: cross-chain tranche
/// comparison isn't meaningful (a Junior tranche on one chain and a Senior
/// tranche on another chain don't compete in the same waterfall), so there's
/// no product-wide Senior/Junior ordering to enforce in the first place.
/// Mirrors interface.sol's `TrancheInput`.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen,
)]
pub struct TrancheInput {
	pub priority: u8,
	pub tranche_type: TrancheType,
	pub vault: VaultId,
	/// See `Tranche::asset`'s doc comment.
	pub asset: H160,
	/// See `Tranche::shares`' doc comment.
	pub shares: H160,
}

// ---------------------------------------------------------------------------
// SourceType / AdapterInfo
// ---------------------------------------------------------------------------

/// Generic over `AccountId` because `OffchainSource` carries `borrower` — unlike
/// `product_admin` (owned entirely by pallet-tranche-permissions), `borrower`
/// lives directly on the adapter here. A product can have multiple
/// OffchainSource adapters, each potentially a different institution, so
/// there's no single product-scoped "Borrower" role to delegate to — this is
/// the source of truth instead.
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

/// NFT collateral backing an OffchainSource adapter's loan book, scoped
/// per-adapter.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen,
)]
pub struct CollateralAsset {
	/// The EVM chain `nft_contract` is deployed on — not necessarily Bifrost.
	pub chain_id: u64,
	/// ERC-721 / ERC-1155 contract address on `chain_id`.
	pub nft_contract: H160,
	/// Token ID identifying the specific NFT.
	pub nft_token_id: U256,
}

// ---------------------------------------------------------------------------
// AdapterInfo
// ---------------------------------------------------------------------------

/// An individual yield-source Adapter, nested under its parent MultichainAdapter
/// (see `MultichainAdapterInfo`) rather than living in a flat, product-wide map.
/// Wraps `SourceType` together with this adapter's own sub-allocation weight —
/// a bare `SourceType` was enough before `weightBps` existed on the interface,
/// but now needs a second field alongside it. Keyed by plain `H160` (its own
/// address) in its parent's `adapters` map — no `chain_id` of its own, since a
/// nested adapter always lives on its parent MultichainAdapter's chain.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen,
)]
pub struct AdapterInfo<AccountId> {
	pub source_type: SourceType<AccountId>,
	/// Sub-allocation weight within this adapter's parent MultichainAdapter,
	/// basis points (10_000 = 100%). Across one MultichainAdapter's nested
	/// `adapters`, these must always sum to exactly 10_000 (2026-07-27, same
	/// invariant as `MultichainAdapterInfo::weight_bps`) — mirrors interface.sol's
	/// `AdapterInput.weightBps` and `set_adapters`' full-array-replace rationale.
	pub weight_bps: u16,
}

// ---------------------------------------------------------------------------
// MultichainAdapterInfo
// ---------------------------------------------------------------------------

/// A single MultichainAdapter routing entry: its top-level allocation weight,
/// plus the individual Adapters it internally manages/routes to (2026-07-27:
/// nested here rather than living in a separate flat
/// `MultichainProductDetails::adapters` map — a MultichainAdapter's internal
/// split across the protocols it manages
/// is a parent-child relationship, not two independent registries).
/// `weight_bps` is basis points (10_000 = 100%) — across one product's full
/// `multichain_adapters` list, these must always sum to exactly 10_000. See
/// `set_multichain_adapters` in interface.sol for why this is a full-array
/// replace rather than single-entity add/remove/update.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen,
)]
pub struct MultichainAdapterInfo<AccountId> {
	pub weight_bps: u16,
	/// Keyed by the adapter's own address (`H160`) — not `AdapterKey`, since a
	/// nested adapter carries no `chain_id` of its own; it's always this parent's
	/// `chain_id` (implied, not duplicated in the key).
	pub adapters: BoundedBTreeMap<
		H160,
		AdapterInfo<AccountId>,
		ConstU32<MAX_ADAPTERS_PER_MULTICHAIN_ADAPTER>,
	>,
}

// ---------------------------------------------------------------------------
// ValuationInfo
// ---------------------------------------------------------------------------

#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen,
)]
pub struct ValuationInfo {
	/// The product's denomination asset — its token address on the Hub chain.
	/// Admin-set at `create_product`, immutable afterward (same as
	/// `valuation_address`). `tranche_nav`/`product_nav`/etc. throughout
	/// pallet-tranche-investments are all denominated in this asset.
	pub base_asset: H160,
	/// Hub-chain Valuation contract address for this product. This pallet's
	/// authorization check for pallet-tranche-investments-style calls compares
	/// the caller against this address (see interface.sol notes — no Gateway
	/// in that call path).
	pub valuation_address: H160,
	/// Unix timestamp (seconds) the first settlement cycle begins. Admin-set
	/// at `create_product` — must be strictly after the block time `create_product`
	/// executes in (`create_product` reverts otherwise), letting a product's
	/// settlement schedule be set up before it goes live but never backdated.
	/// Every later cycle starts at `settlement_start_timestamp + k *
	/// settlement_length_secs` for integer `k`.
	///
	/// Purely configuration: this pallet takes no action on its own when the
	/// time arrives. Settlement (calling `Valuation.tryUpdateNav()`) is
	/// triggered by an off-chain bot reading this schedule — not by
	/// `on_initialize` — since an internal EVM call from a Substrate hook
	/// leaves no Ethereum transaction/receipt for anything to look up.
	pub settlement_start_timestamp: u64,
	/// Length of one settlement cycle, in seconds, counted from
	/// `settlement_start_timestamp`. Admin-set; recommended to be at least the
	/// GCD of the underlying yield sources' cycles. Must be strictly greater
	/// than `settlement_offset_secs` (`create_product` reverts otherwise) —
	/// see that field's doc comment for why.
	pub settlement_length_secs: u64,
	/// Width, in seconds, of the settlement window at the *end* of each
	/// cycle — e.g. `3600` for a 1-hour window (not "seconds since the cycle
	/// started"). Order submission closes ("market close") when the window
	/// opens. Must be strictly less than `settlement_length_secs`
	/// (`create_product` reverts otherwise) — otherwise the window would
	/// swallow the whole cycle (or more), leaving no room for order
	/// submission at all.
	pub settlement_offset_secs: u64,
}

// ---------------------------------------------------------------------------
// SettlementMode
// ---------------------------------------------------------------------------

/// How a single-chain product settles requests — see `SingleChainProductDetails`'s
/// doc comment for why this doesn't exist for multichain products (they're
/// always `Async`, implicitly, via `MultichainProductDetails::valuation`'s
/// settlement fields — a cross-chain leg always takes more than one block, so
/// there's structurally no such thing as multichain `Sync`).
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen,
)]
pub enum SettlementMode {
	/// Request and settlement happen atomically, in the same transaction —
	/// there's no settlement cycle at all. Only structurally possible for a
	/// single-chain product: with every contract (Vault, TrancheManager,
	/// Valuation, Adapters, Ledger) on one chain and no bridging leg to wait
	/// on, a deposit can be applied and settled in one call. A single-chain
	/// product need not use this — it may still opt into `Async` if its
	/// underlying yield sources need a batching cycle.
	Sync,
	/// Requests accumulate and settle on a fixed cycle. Same three fields and
	/// semantics as the ones `ValuationInfo` carries for multichain products
	/// (see those fields' doc comments) — duplicated here per-variant, rather
	/// than shared, so `Sync` mode isn't forced to fake values for fields
	/// that don't apply to it at all.
	Async {
		settlement_start_timestamp: u64,
		settlement_length_secs: u64,
		settlement_offset_secs: u64,
	},
}

// ---------------------------------------------------------------------------
// MultichainProductDetails
// ---------------------------------------------------------------------------

/// One chain's own ordered slice of a product's tranches — index 0 is that
/// chain's own highest priority. Used only within `MultichainProductDetails::tranches`
/// (`SingleChainProductDetails::tranches` uses this same underlying shape
/// directly, unwrapped, since it's already exactly one chain).
pub type ChainTranches = BoundedVec<Tranche, ConstU32<MAX_TRANCHES_PER_CHAIN>>;

/// Generic over `AccountId` (via `SourceType`, see its doc comment) — unlike the
/// old design, this pallet now stores adapter `borrower`s directly rather than
/// delegating them to the permissions pallet. `product_admin` is still NOT
/// stored here, though — it remains fully owned by pallet-tranche-permissions'
/// own `ProductAdmins` storage, since there's exactly one ProductAdmin per
/// product and no ambiguity about where it belongs, unlike `borrower` which
/// only makes sense attached to a specific adapter.
///
/// The hub-spoke multichain model: `valuation` (and its Hub-chain Valuation
/// contract) is the single source of truth other chains' vaults ultimately
/// settle against via cross-chain bridging — see `SingleChainProductDetails`
/// for the alternative, entirely-single-chain model.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen,
)]
pub struct MultichainProductDetails<AccountId> {
	pub valuation: ValuationInfo,
	/// Keyed by `chain_id` — one entry per chain that has at least one
	/// tranche (an entry is removed entirely once its last tranche is, via
	/// `set_tranche`'s `Remove`; never left around empty). Each chain's own
	/// `ChainTranches` is independently ordered by waterfall priority (index 0
	/// = that chain's own highest) — priority is deliberately NOT comparable
	/// *across* different chains' entries here; see `TrancheInput`'s doc
	/// comment for why (2026-08-20 — rescoped from a single flat, product-wide
	/// `BoundedVec<Tranche, ...>` to this per-chain shape).
	pub tranches: BoundedBTreeMap<u64, ChainTranches, ConstU32<MAX_TRANCHE_CHAINS>>,
	/// Always replaced wholesale by `set_multichain_adapters` — see that
	/// function's doc comment in interface.sol for why (100%-sum invariant).
	/// Each entry now owns its own nested `adapters` (see `MultichainAdapterInfo`)
	/// — there is no separate top-level adapters map anymore.
	pub multichain_adapters: BoundedBTreeMap<
		AdapterKey,
		MultichainAdapterInfo<AccountId>,
		ConstU32<MAX_MULTICHAIN_ADAPTERS>,
	>,
	/// This product's TrancheManager contract address on each chain one of
	/// its vaults is deployed on — keyed by `chain_id`, one entry per chain.
	/// Hub included: a Hub-chain entry is required if (and only if) the
	/// product has a Hub-deployed vault, same as any Spoke chain. Independent
	/// per product — two products sharing a chain each bind their own
	/// TrancheManager instance there. `set_multichain_tranche_managers`
	/// enforces that every key here already has at least one tranche on that
	/// chain (`Error::TrancheManagerChainHasNoTranche` otherwise) — this map
	/// can never have more entries than `tranches` has chain groups, hence
	/// reusing `MAX_TRANCHE_CHAINS` as this map's own bound.
	pub multichain_tranche_managers: BoundedBTreeMap<u64, H160, ConstU32<MAX_TRANCHE_MANAGERS>>,
}

// ---------------------------------------------------------------------------
// SingleChainValuationInfo
// ---------------------------------------------------------------------------

/// A single-chain product's Valuation binding + settlement mode — mirrors
/// `ValuationInfo`'s role for `MultichainProductDetails`, as its own type
/// rather than reusing `ValuationInfo` directly: `ValuationInfo`'s three flat
/// settlement fields are already live on-chain via `create_product`
/// (deployed), so changing its shape to carry `SettlementMode` instead would
/// break that already-shipped interface just to share code with this new,
/// unrelated path. A parallel type costs nothing and touches none of that.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen,
)]
pub struct SingleChainValuationInfo {
	/// The product's denomination asset, on the product's `chain_id`. Same
	/// role as `ValuationInfo::base_asset`.
	pub base_asset: H160,
	/// The Valuation contract address, on the product's `chain_id` — NOT
	/// necessarily the Hub chain, unlike `ValuationInfo::valuation_address`.
	pub valuation_address: H160,
	pub settlement_mode: SettlementMode,
}

// ---------------------------------------------------------------------------
// SingleChainProductDetails
// ---------------------------------------------------------------------------

/// A product whose entire stack — Vault(s), TrancheManager, Valuation,
/// Adapters, and a Ledger contract — lives on one EVM chain, which need not
/// be the Hub. No hub-spoke routing, no cross-chain bridging: `tranches`,
/// `tranche_manager`, and `adapters` all implicitly live on `chain_id`
/// (enforced at `create_single_chain_product`), unlike `MultichainProductDetails`
/// where each of those can span a different chain.
///
/// No `pallet-tranche-investments` interaction: since `valuation` isn't
/// necessarily Hub-deployed, this pallet's usual Hub-side settlement
/// recording path doesn't apply. Instead, a `ledger` contract on `chain_id`
/// mirrors `pallet-tranche-investments`' interface and plays that role
/// locally — this pallet only stores its address, it does not talk to it.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen,
)]
pub struct SingleChainProductDetails<AccountId> {
	pub valuation: SingleChainValuationInfo,
	/// The single EVM chain every contract in this product lives on.
	pub chain_id: u64,
	/// This product's waterfall, ordered by priority (index 0 = highest) — a
	/// flat `ChainTranches` directly, unlike `MultichainProductDetails::tranches`'
	/// per-chain-keyed map, since this product structurally has only the one
	/// chain to begin with (every entry's `vault.chain_id` must equal
	/// `chain_id`, enforced at `create_single_chain_product`) — no grouping
	/// needed when there's nothing to group by.
	pub tranches: ChainTranches,
	/// The single TrancheManager contract address, on `chain_id`. Unlike
	/// `MultichainProductDetails::multichain_tranche_managers`, there's only
	/// ever one — no per-chain table, since there's only one chain.
	pub tranche_manager: H160,
	/// Flat individual-Adapter registry — unlike
	/// `MultichainProductDetails::multichain_adapters`, there's no
	/// MultichainAdapter routing layer above these (nothing to route between,
	/// with only one chain), so this is a single, ungrouped level, keyed by
	/// each adapter's own address the same way `MultichainAdapterInfo::adapters`
	/// is. `weight_bps` across the whole map must sum to exactly 10_000.
	pub adapters: BoundedBTreeMap<
		H160,
		AdapterInfo<AccountId>,
		ConstU32<MAX_ADAPTERS_PER_SINGLE_CHAIN_PRODUCT>,
	>,
	/// The Ledger contract address, on `chain_id` — mirrors
	/// `pallet-tranche-investments`' interface locally for this product. See
	/// this struct's doc comment.
	pub ledger: H160,
}

// ---------------------------------------------------------------------------
// ProductDetails
// ---------------------------------------------------------------------------

/// Every registered product, whichever model it follows. One flat `Products`
/// storage map / `ProductId` namespace covers both variants — `product_id`
/// must stay globally unique regardless of model, since e.g.
/// pallet-tranche-tx-registry keys everything by `product_id` alone. Callers
/// (extrinsics, precompile view functions) match on the variant they expect
/// and reject the other with `Error::WrongProductType`.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen,
)]
pub enum ProductDetails<AccountId> {
	Multichain(MultichainProductDetails<AccountId>),
	SingleChain(SingleChainProductDetails<AccountId>),
}

// ---------------------------------------------------------------------------
// FlowVersion
// ---------------------------------------------------------------------------

/// Which tx flow one of a product's pipelines (request or settlement — see
/// `RequestFlowVersion`/`SettlementFlowVersion`, versioned independently of
/// each other) follows. Fixed at product creation (`create_product`/
/// `create_single_chain_product` register `V1` for both pipelines
/// unconditionally — see those extrinsics' own notes) and never expected to
/// change for that product's lifetime.
///
/// Owned by this pallet (not pallet-tranche-tx-registry, which only ever
/// *reads* it, via `ProductInspect::request_flow_version`/
/// `settlement_flow_version`) because it's fundamentally a product-level
/// attribute, same category as `ProductDetails` itself — and because setting
/// it is meant to be a `ProductAdminOrigin`-gated action, the same
/// authorization model every other product-management call in this pallet
/// already uses, rather than the Root-gated model pallet-tranche-tx-registry
/// used for its own, unrelated `TxRecorder` config.
///
/// `V1` is every product created before this axis existed, and remains the
/// only flow `RequestStep`'s/`SettlementStep`'s fixed core steps ever need to
/// serve. `V2` (and any version after it) is reserved for a flow not yet
/// designed — nothing currently emits or expects `V2` evidence;
/// `set_request_flow_version`/`set_settlement_flow_version` exist as the
/// eventual, `ProductAdminOrigin`-gated way to register a product under `V2`,
/// but are deliberately stubbed to always fail for now (see those
/// extrinsics' own doc comments) until a real `V2` pipeline is designed.
#[derive(
	Clone,
	Copy,
	Encode,
	Decode,
	DecodeWithMemTracking,
	PartialEq,
	Eq,
	RuntimeDebug,
	TypeInfo,
	MaxEncodedLen,
)]
pub enum FlowVersion {
	V1,
	V2,
}

// ---------------------------------------------------------------------------
// Traits
// ---------------------------------------------------------------------------

/// Implemented by pallet-tranche-system itself (it owns the `Vaults` reverse
/// index). Consumed by pallet-tranche-permissions to reject granting
/// `Role::TrancheInvestor(vault)` for a vault that doesn't belong to the
/// product the caller is ProductAdmin for.
pub trait VaultInspect {
	/// Returns `true` if `vault` is permanently bound to `product_id` (see
	/// `VaultRegistration`) — regardless of whether it's currently an active
	/// tranche or has been removed (`removed: true`). Ownership, once
	/// established, never changes, so callers checking "does this vault
	/// belong to this product" don't need to separately ask whether it's
	/// still active.
	fn vault_belongs_to_product(product_id: ProductId, vault: &VaultId) -> bool;
	/// Returns `true` if every id in `chain_ids` is a chain where `product_id` has at
	/// least one tranche vault. Takes a slice (rather than one `chain_id` at a time) so
	/// an implementation can fetch `product_id`'s details once and check the whole batch
	/// against it. Consumed by pallet-tranche-tx-registry to validate a settlement
	/// Trigger's declared `finalize_chain_ids` — the chains needing a Finalize leg,
	/// since Finalize delivers a settlement's result to a vault, never an Adapter.
	fn vault_chains_belong_to_product(product_id: ProductId, chain_ids: &[u64]) -> bool;
	/// Reverse lookup: which product `vault` belongs to, if any — unlike
	/// `vault_belongs_to_product`, the caller doesn't need to already know
	/// `product_id`. Consumed by pallet-tranche-tx-registry's
	/// `record_whitelist_tx`, whose only chain-observed evidence (vault, who,
	/// grant, nonce) never carries `product_id` itself the way
	/// `DepositRequested`/`DepositReceived` do — `product_id` is resolved
	/// once, at that pipeline's own Trigger step, via this instead.
	fn product_id_for_vault(vault: &VaultId) -> Option<ProductId>;
}

/// Implemented by pallet-tranche-system itself (it owns the `MultichainAdapterIndex`/
/// `AdapterIndex` reverse indexes). Consumed by pallet-tranche-investments to
/// reject recording an allocation/valuation against a MultichainAdapter or
/// individual Adapter that doesn't actually belong to the product in
/// question — same rationale as `VaultInspect`.
pub trait AdapterInspect {
	/// Returns `true` if `key` is registered as one of `product_id`'s
	/// top-level MultichainAdapters (see `MultichainProductDetails::multichain_adapters`).
	fn multichain_adapter_belongs_to_product(product_id: ProductId, key: &AdapterKey) -> bool;
	/// Returns `true` if `key` is registered as one of `product_id`'s nested
	/// individual Adapters (see `MultichainAdapterInfo::adapters`), under any
	/// parent.
	fn adapter_belongs_to_product(product_id: ProductId, key: &AdapterKey) -> bool;
	/// Returns `true` if every id in `chain_ids` is a chain where `product_id` has a
	/// top-level MultichainAdapter registered — *regardless* of that adapter's current
	/// `weight_bps` (a chain that's since been de-weighted to 0 can still hold capital
	/// allocated to it while its weight was non-zero, so it can't be dropped from this
	/// check just because its weight is 0 now). Takes a slice (rather than one
	/// `chain_id` at a time) so an implementation can fetch `product_id`'s details once
	/// and check the whole batch against it. Consumed by pallet-tranche-tx-registry to
	/// validate a request's declared `adapter_chain_ids` (an Adapter leg always
	/// targets a yield-source chain, never a vault) and a settlement Trigger's declared
	/// `collect_response_chain_ids` (Collect/Response query NAV from an Adapter, never a
	/// vault).
	fn adapter_chains_belong_to_product(product_id: ProductId, chain_ids: &[u64]) -> bool;
}

/// Implemented by pallet-tranche-system itself (it owns `Products`). Consumed by
/// pallet-tranche-tx-registry to tell a `SingleChain` product apart from a
/// `Multichain` one — the two models need different "does this chain need a
/// bridge leg" logic. A `Multichain` product's Hub-vault requests are bridge-free
/// because they're colocated with the Hub Valuation Contract (a single, fixed
/// chain, global across every multichain product); a `SingleChain` product's
/// requests are bridge-free for the same underlying reason — everything
/// (Vault, TrancheManager, Valuation, Adapters) is colocated — except the chain
/// they're colocated on is that *specific product's own* `chain_id`, not
/// necessarily Hub. `pallet-tranche-tx-registry` uses this to compute a
/// per-product "local chain" (`single_chain_id(product_id)`, falling back to
/// this chain's own Hub `ChainId` if `None`) everywhere it used to hardcode the
/// literal Hub chain ID, rather than adding a second, parallel set of
/// single-chain-only branches.
pub trait ProductInspect {
	/// `true` iff `product_id` refers to a product that actually exists (either
	/// model). Distinct from `single_chain_id`, which returns `None` for both a
	/// `Multichain` product *and* an unregistered `product_id` — a caller that
	/// needs to reject an unknown `product_id` outright (e.g.
	/// pallet-tranche-tx-registry's `SettlementStep::SettleStarted`, whose other
	/// validation is vacuously satisfied when both chain sets are empty) can't
	/// tell the two apart from `single_chain_id` alone.
	fn is_registered(product_id: ProductId) -> bool;
	/// `Some(chain_id)` if `product_id` is a `SingleChain` product — the one
	/// chain its entire stack lives on. `None` if it's `Multichain` (there's no
	/// single answer to "which chain" for that model).
	fn single_chain_id(product_id: ProductId) -> Option<u64>;
	/// `product_id`'s registered request-pipeline `FlowVersion` — see
	/// `RequestFlowVersion`'s storage doc comment. `None` iff `product_id` isn't
	/// registered at all (every registered product gets `Some(FlowVersion::V1)`
	/// here at creation time — see `create_product`/`create_single_chain_product`).
	/// Consumed by pallet-tranche-tx-registry's `record_request_tx`, for
	/// `RequestStep::Extended` only — every other step is identical across every
	/// `FlowVersion`.
	fn request_flow_version(product_id: ProductId) -> Option<FlowVersion>;
	/// `product_id`'s registered settlement-pipeline `FlowVersion` — independent
	/// of `request_flow_version` (see `SettlementFlowVersion`'s storage doc
	/// comment for why each pipeline versions separately). Same `None`
	/// convention and same `record_settlement_tx`/`SettlementStep::Extended`
	/// consumer as `request_flow_version`.
	fn settlement_flow_version(product_id: ProductId) -> Option<FlowVersion>;
}

// ---------------------------------------------------------------------------
// ProductAdmin origin
// ---------------------------------------------------------------------------

/// `EnsureOrigin` that accepts only the `ProductAdmin` pallet origin, yielding
/// the verified admin's `AccountId` as its `Success` value — mirrors
/// `frame_system::EnsureSigned`, which does the same for a plain signed origin.
/// The tranche-system precompile creates this origin before dispatching to
/// `create_product`, guaranteeing it can't be called via a plain signed
/// extrinsic.
/// Wire as `type ProductAdminOrigin = pallet_tranche_system::EnsureProductAdmin<Runtime>`
/// in the runtime.
pub struct EnsureProductAdmin<T>(PhantomData<T>);

impl<OuterOrigin, T> frame_support::traits::EnsureOrigin<OuterOrigin> for EnsureProductAdmin<T>
where
	T: Config,
	T::AccountId: Default,
	OuterOrigin: Into<Result<Origin<T>, OuterOrigin>> + From<Origin<T>>,
{
	type Success = T::AccountId;
	fn try_origin(o: OuterOrigin) -> Result<Self::Success, OuterOrigin> {
		match o.into() {
			Ok(Origin::ProductAdmin(who)) => Ok(who),
			Err(o) => Err(o),
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin() -> Result<OuterOrigin, ()> {
		Ok(OuterOrigin::from(Origin::ProductAdmin(T::AccountId::default())))
	}
}
