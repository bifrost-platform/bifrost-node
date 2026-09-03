#![cfg_attr(not(feature = "std"), no_std)]

//! # pallet-tranche-custom-flows
//!
//! An append-only evidence log for **product-specific extra tx flows** in the
//! OmniFi tranche-system — reward-pool claims, strategy swap+bridge sequences,
//! and anything else that doesn't belong in `pallet-tranche-tx-registry`'s four
//! universal pipelines. The step topology of each flow is defined by an
//! on-chain [`FlowDescriptor`] (registered by governance), not by pallet code,
//! so a new flow costs one `set_flow_descriptor` call, not a pallet release.
//!
//! Trust model: identical to `pallet-tranche-tx-registry` — the single
//! `TxRecorder` account's attestations are taken at face value. The pallet
//! records what the recorder submits and computes flow completion structurally
//! (counting satisfied required slots); it never re-verifies txs, parses
//! `metadata`, or enforces step ordering. See
//! `docs/tranche-custom-flows/design-minimal.md` for the full rationale.

pub mod migrations;
pub mod weights;

mod pallet;

pub use pallet::pallet::*;
pub use weights::WeightInfo;

pub use bp_tranche::{
	history::{self, HistoryPage, PagedInvestorHistory},
	ChainId, ProductId, TxRecord,
};
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::{ConstU32, H160, H256};
use sp_runtime::{BoundedVec, RuntimeDebug};

// ---------------------------------------------------------------------------
// Primitive type aliases
// ---------------------------------------------------------------------------

// `ProductId` is re-exported from `bp-tranche` (above) — same type as
// `pallet_tranche_system::ProductId`, taken from the shared crate rather than
// tranche-system so this pallet keeps no hard dependency on it (it validates
// nothing against it — descriptor registration is governance-gated).

/// A step's index within its track's slot sequence — unique across the whole
/// flow (main slots + every sub track's slots are pairwise disjoint), so
/// `slot_id` alone tells the pallet which track a step belongs to.
pub type SlotId = u8;

/// Index into [`FlowDescriptor::sub_tracks`].
pub type TrackIndex = u8;

/// A flow's identifier — a short ASCII slug, unique within a product. Solidity
/// `bytes16`. Governance picks it at `set_flow_descriptor`; no counter storage.
pub type FlowId = [u8; 16];

/// One flow-execution's identifier — the correlation id minted by the opening
/// main-slot event (`claim_id`, `request_id`, or the single-chain claim
/// `tx_hash`). Fixed 32 bytes / Solidity `bytes32`; a shorter native id is
/// left-padded by the recorder. The pallet keys storage by it but never
/// interprets it.
pub type InstanceKey = H256;

/// `record_flow_tx`'s wire form of a [`Lane`]. `None` ⇒ the main lane;
/// `Some(chain)` ⇒ that chain's lane within whichever sub track owns the
/// `slot_id` (the pallet derives the sub-track index from `slot_id`). The
/// recorder sets it from the event's lane context (e.g.
/// `SocketMessage.spoke_chain_id`), not from where the tx ran — §8 step 3.
pub type TrackKey = Option<ChainId>;

// ---------------------------------------------------------------------------
// Bounds
// ---------------------------------------------------------------------------

/// Max steps in `main_track.slots` or in one sub track's `slots`.
pub const MAX_SLOTS: u32 = 32;
/// Max sub tracks (parallel-branch kinds) per flow.
pub const MAX_SUB_TRACKS: u32 = 10;
/// Max chains allowed in one sub track.
pub const MAX_TRACK_CHAINS: u32 = 10;
/// Max bytes of `SlotRecord::metadata` (per-slot, last-write-wins). Ceiling
/// sized to hold a full CCCP socket message (~20 KB) plus overhead — typical
/// entries are far smaller. `FlowSlots` is `#[pallet::unbounded]`, so this bound
/// is a safety cap only and does not inflate PoV weight (benchmarks measure the
/// real size).
pub const MAX_SLOT_METADATA: u32 = 30 * 1024; // 30 KB
/// Max bytes of `Attempt::metadata` (per-attempt, never pruned). Same ceiling as
/// [`MAX_SLOT_METADATA`] — a full CCCP socket message fits. Recorders should
/// still keep this small in the common case and store only a hash of any large
/// payload that a reference would serve (see design doc §5).
pub const MAX_ATTEMPT_METADATA: u32 = 30 * 1024; // 30 KB
/// Max attempts recorded for one `(instance, lane, slot)`.
pub const MAX_ATTEMPTS: u32 = 10;

// ---------------------------------------------------------------------------
// Lane
// ---------------------------------------------------------------------------

/// The runtime realisation of a track for one instance — the storage-key form
/// of the wire [`TrackKey`]. `Main` is the single main-track lane (`TrackKey`
/// `None`); `Sub { track, chain }` is one chain's lane within sub track `track`
/// (`TrackKey` `Some(chain)`). A chain that appears in two sub tracks is two
/// distinct lanes — hence `track` is part of the key, not just `chain` (which
/// is all the wire `TrackKey` carries; the pallet fills in `track` from the
/// `slot_id`).
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
pub enum Lane {
	Main,
	Sub { track: TrackIndex, chain: ChainId },
}

// ---------------------------------------------------------------------------
// Descriptor
// ---------------------------------------------------------------------------

/// One step in a track's sequence. `optional` steps don't count toward
/// completion — a lane is `done` once every `!optional`, non-skipped step is
/// satisfied. The pallet reads `optional` (for `required_count`); it does not
/// interpret what the step means.
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
pub struct SlotDef {
	pub id: SlotId,
	pub optional: bool,
}

/// One chain's participation in a sub track.
///
/// - `optional`: this chain may not appear in a given instance (e.g. a
///   weight-zero adapter chain in a settlement cycle). If it satisfies a
///   required slot, it must then complete before the instance can close.
/// - `skip_slots`: steps in the track's sequence this one chain doesn't do.
/// - `required_count`: derived — `slots` that are `!optional` and not in
///   `skip_slots`. Filled by `set_flow_descriptor`, never a governance input.
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
pub struct TrackChain {
	pub chain_id: ChainId,
	pub optional: bool,
	pub skip_slots: BoundedVec<SlotId, ConstU32<MAX_SLOTS>>,
	pub required_count: u16,
}

/// The main track — the step sequence every flow has, realised as the single
/// `Lane::Main` lane. Sibling of [`SubTrack`]; the shape differs because the main
/// track is always exactly one lane, so it carries no chain list — just the one
/// chain its steps occur on.
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
pub struct MainTrack {
	/// The chain `slots` occur on (self-describing; the pallet stores but does
	/// not validate it).
	pub chain_id: ChainId,
	pub slots: BoundedVec<SlotDef, ConstU32<MAX_SLOTS>>,
	/// Derived at `set_flow_descriptor`: count of `!optional` `slots`. Never a
	/// governance input — overwritten on registration.
	pub required_count: u16,
}

/// A sub track — one parallel-branch kind (e.g. "per adapter chain"). One step
/// sequence shared by all its chains, plus the list of allowed chains. Sub
/// tracks are siblings of the main track, not nested.
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
pub struct SubTrack {
	pub slots: BoundedVec<SlotDef, ConstU32<MAX_SLOTS>>,
	pub chains: BoundedVec<TrackChain, ConstU32<MAX_TRACK_CHAINS>>,
}

/// The step topology of a flow, keyed by `(ProductId, FlowId)`.
///
/// `main_track.required_count` / `non_optional_lane_count` / each
/// `TrackChain::required_count` are **derived** at `set_flow_descriptor` and
/// stored alongside the governance-supplied fields, so the per-record close
/// check is O(1). Editing fields that change those counts while instances are
/// open can strand or prematurely close them — see §7.
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
pub struct FlowDescriptor {
	pub version: u16,
	pub main_track: MainTrack,
	/// Empty ⇒ a flow with no fan-out (linear, all steps on the main lane).
	pub sub_tracks: BoundedVec<SubTrack, ConstU32<MAX_SUB_TRACKS>>,
	/// `true` ⇒ instances are indexed per-investor (reward claims); `false` ⇒
	/// not (settlement-style flows).
	pub investor_scoped: bool,

	// ---- derived at registration (not a governance input) ----
	/// `1` (main) + Σ over sub tracks of that track's `!optional` chain count —
	/// the initial `FlowInstance::pending_lanes`.
	pub non_optional_lane_count: u16,
}

// ---------------------------------------------------------------------------
// Records
// ---------------------------------------------------------------------------

/// One observation of a step. `success` is the recorder's flag for "this
/// observation satisfies the slot" (`true` for e.g. a bridge `Executed`,
/// `false` for a `Reverted`/`FAILURE` observation kept for audit); it is the
/// only field the pallet reads — for the close counter. The kind of success
/// event, gas, message ids etc. go in `metadata`, which the pallet never
/// inspects.
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
pub struct Attempt<BlockNumber> {
	pub tx: TxRecord<BlockNumber>,
	pub success: bool,
	pub metadata: BoundedVec<u8, ConstU32<MAX_ATTEMPT_METADATA>>,
}

/// The value stored at `FlowSlots[(product, flow, instance, lane, slot)]`.
///
/// - `satisfied`: cached — has any `success: true` attempt landed. The close
///   counter (§8) triggers on its `false → true` transition; also a read hint.
/// - `metadata`: slot-level bytes, last-write-wins, independent of retries
///   (resolved pool address, confirmed amount, …). Consumer-interpreted.
/// - `attempts`: append-only, in observation order.
#[derive(
	Clone,
	Encode,
	Decode,
	DecodeWithMemTracking,
	Default,
	PartialEq,
	Eq,
	RuntimeDebug,
	TypeInfo,
	MaxEncodedLen,
)]
pub struct SlotRecord<BlockNumber> {
	pub satisfied: bool,
	pub metadata: BoundedVec<u8, ConstU32<MAX_SLOT_METADATA>>,
	pub attempts: BoundedVec<Attempt<BlockNumber>, ConstU32<MAX_ATTEMPTS>>,
}

/// One flow-execution's header, keyed by `(ProductId, FlowId, InstanceKey)`.
/// Created by the first `record_flow_tx` for the key — necessarily its first
/// main slot; `closed` flips when the last required lane completes.
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
pub struct FlowInstance<BlockNumber> {
	pub opened_at: BlockNumber,
	/// `Some` only when `descriptor.investor_scoped`.
	pub investor: Option<H160>,
	/// Lanes that still need to reach `done`. Init = `non_optional_lane_count`;
	/// `0` ⇒ `closed`.
	pub pending_lanes: u16,
	pub closed: bool,
}
