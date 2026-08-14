#![cfg_attr(not(feature = "std"), no_std)]

pub mod migrations;
mod pallet;
pub mod weights;

pub use pallet::pallet::*;
pub use weights::WeightInfo;

use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::{ConstU32, H160, H256, U256};
use sp_runtime::{BoundedBTreeMap, BoundedVec, RuntimeDebug};
use sp_std::{marker::PhantomData, vec::Vec};

// ---------------------------------------------------------------------------
// Primitive type aliases / constants
// ---------------------------------------------------------------------------

/// Product identifier. Same convention as the old pallet-pools' `PoolId`.
pub type ProductId = u64;

/// Maximum number of tranches per product. Carried over from pallet-pools' `MAX_TRANCHES`
/// (each tranche entry there mapped 1:1 to what's now a `Tranche` here).
pub const MAX_TRANCHES: u32 = 10;

/// Maximum number of MultichainAdapter routing entries per product.
pub const MAX_MULTICHAIN_ADAPTERS: u32 = 20;

/// Maximum number of individual (single-yield-source) Adapters per
/// MultichainAdapter. Rescoped from per-product to per-MultichainAdapter
/// (2026-07-27): adapters now live nested under their parent MultichainAdapter
/// (see `MultichainAdapterInfo`) instead of in a flat, product-wide registry.
pub const MAX_ADAPTERS_PER_MULTICHAIN_ADAPTER: u32 = 20;

/// Maximum number of per-Spoke-chain TrancheManager bindings per product (see
/// `ProductDetails::multichain_tranche_managers`).
pub const MAX_TRANCHE_MANAGERS: u32 = 20;

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

/// Identifies a MultichainAdapter entry: its own address paired with the EVM
/// chain it lives on. Globally unique across ALL products, mirroring
/// pallet-pools' `CollateralAsset` uniqueness convention.
///
/// Nested Adapters (see `AdapterInfo`) are NOT keyed by this type — they carry
/// no `chain_id` of their own (removed 2026-07-27; a nested adapter always
/// lives on its parent MultichainAdapter's chain), so `ProductDetails`'s nested
/// `adapters` map is keyed by plain `H160` instead. `AdapterIndex`'s reverse-index
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
/// set into `ProductDetails::tranches`' final order — `priority` itself is
/// never persisted (see `Tranche`'s doc comment for why position alone
/// suffices after that). Sorting by `priority` must yield all `Senior`
/// tranches before all `Junior` ones — `create_product` reverts otherwise, so
/// a product's waterfall always pays Seniors before Juniors by construction,
/// not just by whatever order a caller happened to submit them in. Mirrors
/// interface.sol's `TrancheInput`.
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
/// nested here rather than living in a separate flat `ProductDetails::adapters`
/// map — a MultichainAdapter's internal split across the protocols it manages
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
// ProductDetails
// ---------------------------------------------------------------------------

/// Generic over `AccountId` (via `SourceType`, see its doc comment) — unlike the
/// old design, this pallet now stores adapter `borrower`s directly rather than
/// delegating them to the permissions pallet. `product_admin` is still NOT
/// stored here, though — it remains fully owned by pallet-tranche-permissions'
/// own `ProductAdmins` storage, since there's exactly one ProductAdmin per
/// product and no ambiguity about where it belongs, unlike `borrower` which
/// only makes sense attached to a specific adapter.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen,
)]
pub struct ProductDetails<AccountId> {
	pub valuation: ValuationInfo,
	/// Ordered by waterfall priority — index 0 is highest priority. See
	/// `Tranche`'s doc comment for why there's no separate `priority` field.
	pub tranches: BoundedVec<Tranche, ConstU32<MAX_TRANCHES>>,
	/// Always replaced wholesale by `set_multichain_adapters` — see that
	/// function's doc comment in interface.sol for why (100%-sum invariant).
	/// Each entry now owns its own nested `adapters` (see `MultichainAdapterInfo`)
	/// — there is no separate top-level adapters map anymore.
	pub multichain_adapters: BoundedBTreeMap<
		AdapterKey,
		MultichainAdapterInfo<AccountId>,
		ConstU32<MAX_MULTICHAIN_ADAPTERS>,
	>,
	/// This product's TrancheManager contract address on each Spoke chain one
	/// of its vaults is deployed on — keyed by `chain_id`, one entry per
	/// chain (never the Hub chain: `create_product` reverts if any entry's
	/// `chain_id` equals the Hub's own EVM chain ID, since a Hub-vault
	/// request reaches the Valuation Contract directly with no separate
	/// TrancheManager hop). Independent per product — two products sharing a
	/// Spoke chain each bind their own TrancheManager instance there.
	pub multichain_tranche_managers: BoundedBTreeMap<u64, H160, ConstU32<MAX_TRANCHE_MANAGERS>>,
}

// ---------------------------------------------------------------------------
// Traits
// ---------------------------------------------------------------------------

/// Implemented by pallet-tranche-system itself (it owns the `Vaults` reverse
/// index). Consumed by pallet-tranche-permissions to reject granting
/// `Role::TrancheInvestor(vault)` for a vault that doesn't belong to the
/// product the caller is ProductAdmin for — mirrors pallet-pools'
/// `PoolInspect::tranche_exists` check in the old `grant_permission`.
pub trait VaultInspect {
	/// Returns `true` if `vault` is registered as one of `product_id`'s
	/// tranches.
	fn vault_belongs_to_product(product_id: ProductId, vault: &VaultId) -> bool;
	/// Returns `true` if every id in `chain_ids` is a chain where `product_id` has at
	/// least one tranche vault. Takes a slice (rather than one `chain_id` at a time) so
	/// an implementation can fetch `product_id`'s details once and check the whole batch
	/// against it. Consumed by pallet-tranche-tx-registry to validate a settlement
	/// Trigger's declared `finalize_chain_ids` — the chains needing a Finalize leg,
	/// since Finalize delivers a settlement's result to a vault, never an Adapter.
	fn vault_chains_belong_to_product(product_id: ProductId, chain_ids: &[u64]) -> bool;
}

/// Implemented by pallet-tranche-system itself (it owns the `MultichainAdapterIndex`/
/// `AdapterIndex` reverse indexes). Consumed by pallet-tranche-investments to
/// reject recording an allocation/valuation against a MultichainAdapter or
/// individual Adapter that doesn't actually belong to the product in
/// question — same rationale as `VaultInspect`.
pub trait AdapterInspect {
	/// Returns `true` if `key` is registered as one of `product_id`'s
	/// top-level MultichainAdapters (see `ProductDetails::multichain_adapters`).
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

/// Unlike `VaultInspect`/`AdapterInspect` above, this is implemented by
/// pallet-tranche-investments, not by pallet-tranche-system itself — it's hosted in
/// this crate only because both pallet-tranche-investments and
/// pallet-tranche-tx-registry already depend on it, and pallet-tranche-tx-registry
/// deliberately has no dependency on pallet-tranche-investments directly (see that
/// crate's module docs / interface.sol's DRAFT note on why the two precompiles were
/// split apart). Consumed by pallet-tranche-tx-registry's `record_settlement_tx` to
/// automatically close out `InvestorActiveRequests` entries when a settlement's
/// Finalize-Hooks leg lands, without needing that dependency or an off-chain-attested
/// request list from the recorder.
pub trait RequestSettlementInspect {
	/// Returns every request_id `record_investment_approval` has linked to
	/// `(product_id, settlement_id)` — i.e. every request approved into that
	/// settlement cycle, regardless of which chain each one originated on (the
	/// caller is expected to filter by origin chain itself, since this trait has
	/// no notion of chains). Empty if no request has been approved into this
	/// settlement (yet, or ever).
	fn settlement_requests(product_id: ProductId, settlement_id: U256) -> Vec<H256>;
}

// ---------------------------------------------------------------------------
// ProductAdmin origin
// ---------------------------------------------------------------------------

/// `EnsureOrigin` that accepts only the `ProductAdmin` pallet origin, yielding
/// the verified admin's `AccountId` as its `Success` value — mirrors
/// `frame_system::EnsureSigned`, which does the same for a plain signed origin.
/// The tranche-system precompile creates this origin before dispatching to
/// `create_product`, guaranteeing it can't be called via a plain signed
/// extrinsic — mirrors pallet-pools' `EnsurePoolAdmin`.
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
