//! # pallet-tranche-tx-registry-v3
//!
//! A template-driven, append-only registry of off-chain tx evidence for the
//! OmniFi tranche-system. Every pipeline — deposit/redeem requests,
//! settlements, whitelists, receives, and any product-specific flow (what
//! `pallet-tranche-custom-flows` used to hold) — is recorded through the same
//! two recorder calls, as `(slot, lane, chain_id, tx_hash)` attestations.
//!
//! Design (see `docs/tranche-tx-registry-v3/design.md`):
//! - **The recorder is the single source of truth.** The pallet does not enforce step ordering,
//!   per-step parameter shapes, chunk counts or bridge statuses — it only checks that a slot exists
//!   in the instance's template, that the tx hash is non-zero, and that the same tx isn't attested
//!   twice to the same slot. Everything else is the recorder's call.
//! - **Step topology is data, not code.** A [`Template`] lists a `main` track (steps that happen
//!   once per instance) and zero or more sub-`tracks` (steps repeated once per *lane*, i.e. per
//!   chain). Templates are immutable and versioned ([`TemplateId`]); changing a flow means
//!   registering a new version and reassigning products to it.
//! - **Products get default templates at registration.** Governance keeps one default `FlowId ->
//!   TemplateId` map per [`ProductModel`]; the runtime calls [`Pallet::assign_default_flows`] from
//!   pallet-tranche-system's `OnProductCreated` hook. An open instance stays pinned to the template
//!   it was opened with, so reassignment only affects instances opened afterwards.
//! - **Progress is derived by counting.** A slot is *completed* once the recorder attests it with
//!   `completes = true` — or, for a multi-part step (chunked settlement legs, several payout moves
//!   from one chain), once every distinct part index `0..count` has a completing attempt. An
//!   instance completes once every required main slot is completed and every lane it has (declared
//!   by the recorder, or implicitly by recording into it) is done. A closed instance still accepts
//!   evidence, but its progress counters stay frozen at close.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
pub mod defaults;
pub mod migrations;
pub mod weights;

#[cfg(test)]
mod mock;
mod pallet;
#[cfg(test)]
mod tests;

pub use pallet::pallet::*;
pub use weights::WeightInfo;

pub use bp_tranche::{
	history::{self, HistoryPage, PagedInvestorHistory},
	is_valid_tx_hash, ChainAddress, ChainId, ProductId, TxHash, TxRecord, MAX_TX_HASH_LEN,
};
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::{ConstU32, H256};
use sp_runtime::{BoundedVec, RuntimeDebug};

/// `EnsureOrigin` that accepts only a signed origin from this pallet's own
/// [`pallet::pallet::TxRecorder`]. For a runtime that carries v3 alone (no v1
/// registry to share a recorder identity with): `type RecorderOrigin =
/// pallet_tranche_tx_registry_v3::EnsureTxRecorder<Runtime>`, set the account
/// with `set_tx_recorder` or the genesis `tx_recorder`.
pub struct EnsureTxRecorder<T>(sp_std::marker::PhantomData<T>);

impl<OuterOrigin, T> frame_support::traits::EnsureOrigin<OuterOrigin> for EnsureTxRecorder<T>
where
	T: pallet::pallet::Config,
	OuterOrigin: Into<Result<frame_system::RawOrigin<T::AccountId>, OuterOrigin>>
		+ From<frame_system::RawOrigin<T::AccountId>>,
{
	type Success = T::AccountId;

	fn try_origin(o: OuterOrigin) -> Result<Self::Success, OuterOrigin> {
		o.into().and_then(|raw| match raw {
			frame_system::RawOrigin::Signed(who)
				if pallet::pallet::TxRecorder::<T>::get().as_ref() == Some(&who) =>
				Ok(who),
			raw => Err(OuterOrigin::from(raw)),
		})
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin() -> Result<OuterOrigin, ()> {
		let recorder = pallet::pallet::TxRecorder::<T>::get().ok_or(())?;
		Ok(OuterOrigin::from(frame_system::RawOrigin::Signed(recorder)))
	}
}

#[cfg(feature = "runtime-benchmarks")]
pub trait BenchmarkHelper {
	/// Seeds the shared recorder identity `RecorderOrigin` checks.
	fn seed_recorder();
}

#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkHelper for () {
	fn seed_recorder() {}
}

// ---------------------------------------------------------------------------
// Identifiers
// ---------------------------------------------------------------------------

/// A product-scoped flow slug (`"deposit"`, `"settlement"`, `"reward-claim"`,
/// …), ASCII, zero-padded. Solidity `bytes16`.
pub type FlowId = [u8; 16];

/// A template's name, same encoding as [`FlowId`] (`"mc-deposit"`, …).
pub type TemplateName = [u8; 16];

pub type TemplateVersion = u16;

/// One flow execution's correlation id — `request_id`, `settlement_id`,
/// whitelist nonce, … (left-padded to 32 bytes when shorter).
pub type InstanceKey = H256;
/// Optional recorder-defined sub-key of an investor-scoped instance (e.g. the
/// vault a whitelist action or request targets). `LatestByScope` keeps the
/// most recently opened instance per `(investor, product, flow, scope)`.
pub type InstanceScope = H256;

/// A step's id within its template. Unique across the whole template (main and
/// every track), ascending in progression order within each track.
pub type SlotId = u8;

/// Index into [`Template::tracks`].
pub type TrackIndex = u8;

/// `(index, count)` of one part of a multi-part step — a settlement leg chunk
/// (`chunkIndex`, `chunkCount`) or one of several payout moves from a chain.
/// The bridge-tx and hook-tx attempts of the same part share its index.
pub type Part = (u16, u16);

/// An instance that references another one (a request → the settlement it was
/// approved into, …).
pub type InstanceRef = (FlowId, InstanceKey);

/// `const`-friendly [`FlowId`]/[`TemplateName`] constructor: copies `s`'s
/// bytes, zero-padding (or truncating) to 16.
pub const fn slug(s: &str) -> [u8; 16] {
	let bytes = s.as_bytes();
	let mut out = [0u8; 16];
	let mut i = 0;
	while i < bytes.len() && i < 16 {
		out[i] = bytes[i];
		i += 1;
	}
	out
}

// ---------------------------------------------------------------------------
// Bounds
// ---------------------------------------------------------------------------

pub const MAX_SLOT_NAME_LEN: u32 = 32;
pub const MAX_MAIN_SLOTS: u32 = 16;
/// `mc-settlement` alone uses 7 (collect-response, finalize, and the five
/// round-close ROUTE / CCTP tracks).
pub const MAX_TRACKS: u32 = 8;
pub const MAX_TRACK_SLOTS: u32 = 16;
/// Ceiling on `main` + every track's slots — the `u8` slot-id space is far
/// larger, this is what bounds a template's encoded size.
pub const MAX_TEMPLATE_SLOTS: u32 = MAX_MAIN_SLOTS + MAX_TRACKS * MAX_TRACK_SLOTS;
/// Distinct lanes (`(track, chain)` pairs) one instance can accumulate.
pub const MAX_INSTANCE_LANES: u16 = 64;
/// Lanes one `record_tx` call can declare.
pub const MAX_LANE_DECLARATIONS: u32 = 32;
/// Attestations one slot can hold — chunked legs (≤ 10 chunks) plus retries.
pub const MAX_ATTEMPTS: u32 = 32;
/// Largest `count` a [`Part`] may carry (parts are tracked in a `u64` bitmap).
pub const MAX_PART_COUNT: u16 = 64;
pub const MAX_ATTEMPT_METADATA: u32 = 2048;
pub const MAX_INSTANCE_METADATA: u32 = 1024;
/// Outgoing links per instance (a request normally has exactly one: its
/// settlement).
pub const MAX_LINKS: u32 = 8;
/// Instances one `record_tx_batch` call can attest (`MAX_SETTLEMENT_REQUESTS`).
pub const MAX_BATCH_INSTANCES: u32 = 1000;
/// Default flows per [`ProductModel`].
pub const MAX_DEFAULT_FLOWS: u32 = 16;

pub type SlotName = BoundedVec<u8, ConstU32<MAX_SLOT_NAME_LEN>>;
pub type AttemptMetadata = BoundedVec<u8, ConstU32<MAX_ATTEMPT_METADATA>>;
pub type InstanceMetadata = BoundedVec<u8, ConstU32<MAX_INSTANCE_METADATA>>;
pub type LaneDeclarations = BoundedVec<(TrackIndex, ChainId), ConstU32<MAX_LANE_DECLARATIONS>>;
pub type BatchKeys = BoundedVec<InstanceKey, ConstU32<MAX_BATCH_INSTANCES>>;

// ---------------------------------------------------------------------------
// Templates
// ---------------------------------------------------------------------------

#[derive(
	Clone,
	Copy,
	Encode,
	Decode,
	DecodeWithMemTracking,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	RuntimeDebug,
	TypeInfo,
	MaxEncodedLen,
)]
pub struct TemplateId {
	pub name: TemplateName,
	pub version: TemplateVersion,
}

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
	/// Human-readable step name (usually the observed contract event —
	/// `"DepositQueued"`), returned by the precompile so consumers can label
	/// progress without an off-chain table.
	pub name: SlotName,
	/// Whether this slot must complete for its lane (or, on `main`, the
	/// instance) to be done. Optional slots are recorded and shown, never
	/// waited on.
	pub required: bool,
}

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
pub struct TrackDef {
	pub name: SlotName,
	pub slots: BoundedVec<SlotDef, ConstU32<MAX_TRACK_SLOTS>>,
	/// Derived at registration — number of `required` slots.
	pub required_count: u16,
}

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
pub struct Template {
	/// Whether an instance belongs to an investor (indexed in
	/// `InvestorActiveInstances`/history). The opening attestation must then
	/// carry `investor`.
	pub investor_scoped: bool,
	pub main: BoundedVec<SlotDef, ConstU32<MAX_MAIN_SLOTS>>,
	/// Derived at registration — number of `required` main slots.
	pub main_required_count: u16,
	/// Per-lane step sequences. Lanes are keyed by chain and declared per
	/// instance, so a template never lists chains — the same template serves
	/// every product regardless of its topology.
	pub tracks: BoundedVec<TrackDef, ConstU32<MAX_TRACKS>>,
}

impl Template {
	pub fn main_slot(&self, slot_id: SlotId) -> Option<&SlotDef> {
		self.main.iter().find(|slot| slot.id == slot_id)
	}

	/// The track holding `slot_id`, if any.
	pub fn track_slot(&self, slot_id: SlotId) -> Option<(TrackIndex, &SlotDef)> {
		self.tracks.iter().enumerate().find_map(|(index, track)| {
			track
				.slots
				.iter()
				.find(|slot| slot.id == slot_id)
				.map(|slot| (index as u8, slot))
		})
	}

	pub fn slot_name(&self, slot_id: SlotId) -> Option<&SlotName> {
		self.main_slot(slot_id)
			.or_else(|| self.track_slot(slot_id).map(|(_, slot)| slot))
			.map(|slot| &slot.name)
	}

	pub fn total_slots(&self) -> u32 {
		(self.main.len() + self.tracks.iter().map(|track| track.slots.len()).sum::<usize>()) as u32
	}
}

/// Which default-flow set a newly registered product receives.
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
pub enum ProductModel {
	Multichain,
	SingleChain,
}

// ---------------------------------------------------------------------------
// Instances
// ---------------------------------------------------------------------------

#[derive(
	Clone,
	Copy,
	Encode,
	Decode,
	DecodeWithMemTracking,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	RuntimeDebug,
	TypeInfo,
	MaxEncodedLen,
)]
pub enum Lane {
	Main,
	Sub { track: TrackIndex, chain: ChainId },
}

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
pub enum InstanceStatus {
	Open,
	Completed,
	/// Closed by the recorder as permanently failed (abandoned, refunded, …).
	Failed,
}

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
pub enum CloseOutcome {
	Completed,
	Failed,
}

impl From<CloseOutcome> for InstanceStatus {
	fn from(outcome: CloseOutcome) -> Self {
		match outcome {
			CloseOutcome::Completed => InstanceStatus::Completed,
			CloseOutcome::Failed => InstanceStatus::Failed,
		}
	}
}

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
pub struct Instance<BlockNumber> {
	/// Pinned at open — later reassignment of the product's flow doesn't
	/// affect this instance.
	pub template: TemplateId,
	pub investor: Option<ChainAddress>,
	/// Opaque, recorder-defined (e.g. vault/amount/order type for a request);
	/// last write wins.
	pub metadata: InstanceMetadata,
	pub status: InstanceStatus,
	pub opened_at: BlockNumber,
	pub closed_at: Option<BlockNumber>,
	/// Completed required main slots.
	pub main_done: u16,
	/// Highest completed main slot — the instance's "current step".
	pub main_stage: Option<SlotId>,
	/// Lanes this instance has.
	pub lane_count: u16,
	/// Lanes not yet done.
	pub pending_lanes: u16,
	pub links: BoundedVec<InstanceRef, ConstU32<MAX_LINKS>>,
}

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
	Default,
)]
pub struct LaneState {
	/// Completed required slots of this lane's track.
	pub done_required: u16,
	/// Highest completed slot in this lane.
	pub stage: Option<SlotId>,
	pub done: bool,
}

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
	/// Where the attested tx actually ran — may differ from the lane's chain
	/// (a CCTP burn on chain A attested to chain B's lane).
	pub tx: TxRecord<BlockNumber>,
	/// The recorder's verdict that this attestation succeeded — `false` for a
	/// `Reverted` bridge tx, a failed hook tx, an ignored message.
	pub completes: bool,
	/// Which part of a multi-part step this attempt is; `None` for a
	/// single-part step.
	pub part: Option<Part>,
	/// Opaque, recorder-defined (raw bridge status, …).
	pub metadata: AttemptMetadata,
}

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
	Default,
)]
pub struct SlotRecord<BlockNumber> {
	pub completed: bool,
	/// Set by the slot's first attempt: `Some(count)` if it is a multi-part
	/// step, `None` otherwise. Every later attempt must agree.
	pub part_count: Option<u16>,
	/// Bit `i` set once part `i` has a completing attempt.
	pub parts_done: u64,
	pub attempts: BoundedVec<Attempt<BlockNumber>, ConstU32<MAX_ATTEMPTS>>,
}
