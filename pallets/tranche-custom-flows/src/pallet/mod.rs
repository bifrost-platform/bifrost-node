mod impls;

use crate::{
	ChainId, FlowDescriptor, FlowId, FlowInstance, InstanceKey, Lane, ProductId, SlotId,
	SlotRecord, TrackKey, WeightInfo, MAX_ATTEMPT_METADATA, MAX_SLOTS, MAX_SLOT_METADATA,
};

use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;
use sp_core::{ConstU32, H160, H256};
use sp_std::vec::Vec;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	// No `#[pallet::storage_version]` / `on_runtime_upgrade` hook yet: this pallet
	// is added to a live chain (not genesis), so nothing would write an on-chain
	// version anyway — the in-code and on-chain versions are both the implicit 0.
	// The first storage migration adds `#[pallet::storage_version(V1)]` plus a
	// `VersionedMigration` and reconciles it then.
	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Origin allowed to register / replace flow descriptors. Wire as
		/// `EnsureRoot<AccountId>` (or a product-admin origin) in the runtime.
		type GovernanceOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Only accepted origin for `record_flow_tx`. Wire as
		/// `pallet_tranche_tx_registry::EnsureTxRecorder<Runtime>` — the two
		/// pallets share one recorder identity, so this pallet has no
		/// `set_recorder` of its own.
		type RecorderOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	// -----------------------------------------------------------------------
	// Errors
	// -----------------------------------------------------------------------

	#[pallet::error]
	pub enum Error<T> {
		// ---- set_flow_descriptor ----
		/// `main_track.slots` is empty.
		MainTrackEmpty,
		/// A slot list is not strictly ascending by `id`.
		BadSlotOrder,
		/// `main_track.slots` has no `!optional` slot (nothing would ever complete).
		NoRequiredMainSlot,
		/// A sub track's `slots` or `chains` is empty.
		SubTrackEmpty,
		/// A sub track lists the same `chain_id` twice.
		DuplicateChainInTrack,
		/// A `TrackChain::skip_slots` entry isn't one of that track's `slots`.
		SkipSlotNotInTrack,
		/// A `TrackChain`'s effective required-slot count is zero — if it
		/// participated it could never reach `done`, so the instance could
		/// never close.
		ChainNoRequiredSlot,
		/// Two tracks (main / sub) share a `slot_id`.
		SlotIdsOverlap,
		/// The `(product_id, flow_id)` has at least one open (not-yet-closed)
		/// instance, so its descriptor can't be replaced — the derived close
		/// counters seed each open instance's `pending_lanes` and aren't
		/// re-derived, so changing them mid-flight would strand or early-close it.
		/// Wait for every instance to close, or use a new `flow_id`.
		DescriptorHasOpenInstances,

		// ---- record_flow_tx ----
		/// No descriptor registered for this `(product_id, flow_id)`.
		UnknownFlow,
		/// `slot_id` is a main slot but `track_key` is `Some(_)`.
		TrackKeyForMainSlot,
		/// `slot_id` isn't in `main_track.slots` or any sub track's `slots`.
		UnknownSlot,
		/// `slot_id` is a sub-track slot but `track_key` is `None`.
		MissingTrackKey,
		/// The `track_key` chain isn't in that sub track's `chains` list.
		UndeclaredChainLane,
		/// The step is in this chain's `skip_slots`.
		SlotSkippedForChain,
		/// `descriptor.investor_scoped` but `investor` is `None` on the call that
		/// opens the instance.
		MissingInvestor,
		/// No instance exists for this `instance_key` and this record isn't the
		/// first main slot (`main_track.slots[0]`) — only that opens an instance.
		InstanceNotOpened,
		/// `tx_hash` is the zero hash.
		ZeroTxHash,
		/// This `tx_hash` is already recorded as an attempt for this exact
		/// `(instance, lane, slot)` — a re-submission.
		DuplicateAttestation,
		/// `MAX_ATTEMPTS` already recorded for this `(instance, lane, slot)`.
		TooManyAttempts,
	}

	// -----------------------------------------------------------------------
	// Events
	// -----------------------------------------------------------------------

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A flow descriptor was registered for the first time.
		FlowDescriptorSet { product_id: ProductId, flow_id: FlowId, version: u16 },
		/// An existing flow descriptor was replaced (only possible while the flow
		/// had no open instances — §7). Indexers / the recorder reload on this.
		FlowDescriptorUpdated { product_id: ProductId, flow_id: FlowId, version: u16 },
		/// An instance was opened — the first `record_flow_tx` for it (which is
		/// necessarily its first main slot).
		FlowOpened {
			product_id: ProductId,
			flow_id: FlowId,
			instance_key: InstanceKey,
			investor: Option<H160>,
		},
		/// One attestation was appended. `chain_id` / `tx_hash` are the attested tx
		/// (the chain it landed on + its hash), independent of `lane`.
		FlowTxRecorded {
			product_id: ProductId,
			flow_id: FlowId,
			instance_key: InstanceKey,
			lane: Lane,
			slot_id: SlotId,
			chain_id: ChainId,
			tx_hash: H256,
			success: bool,
		},
		/// Every required lane completed — the instance is now closed.
		FlowClosed { product_id: ProductId, flow_id: FlowId, instance_key: InstanceKey },
	}

	// -----------------------------------------------------------------------
	// Storage
	// -----------------------------------------------------------------------

	#[pallet::storage]
	/// A flow's step topology. `set_flow_descriptor` (governance) writes it, with
	/// the derived count fields filled in by the pallet. Replaceable only while
	/// `OpenInstanceCount` for the same `(product_id, flow_id)` is `0` (§7).
	pub type FlowDescriptors<T: Config> =
		StorageDoubleMap<_, Blake2_128Concat, ProductId, Blake2_128Concat, FlowId, FlowDescriptor>;

	#[pallet::storage]
	/// Number of not-yet-`closed` instances for a `(product_id, flow_id)` — `+1`
	/// on open, `-1` on close (an open-and-immediately-close nets to 0). O(1)
	/// gate for `set_flow_descriptor`'s "no open instances" rule (§7), so it
	/// needn't iterate `FlowInstances`. `ValueQuery` — `0` for an unregistered
	/// flow.
	pub type OpenInstanceCount<T: Config> =
		StorageDoubleMap<_, Blake2_128Concat, ProductId, Blake2_128Concat, FlowId, u32, ValueQuery>;

	#[pallet::storage]
	/// One flow-execution's header. Created by the first `record_flow_tx` for
	/// the `instance_key` (necessarily its first main slot).
	pub type FlowInstances<T: Config> = StorageNMap<
		_,
		(
			NMapKey<Blake2_128Concat, ProductId>,
			NMapKey<Blake2_128Concat, FlowId>,
			NMapKey<Blake2_128Concat, InstanceKey>,
		),
		FlowInstance<BlockNumberFor<T>>,
	>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// Per-`(instance, lane, slot)` evidence. `ValueQuery` — an empty
	/// `SlotRecord` (`satisfied: false`, no attempts) and "nothing recorded
	/// here" are the same state. `#[pallet::unbounded]`: `MAX_SLOT_METADATA` /
	/// `MAX_ATTEMPT_METADATA` are rare-case ceilings (a full CCCP socket
	/// message), so the ~330 KB `MaxEncodedLen` would badly over-charge PoV —
	/// benchmarks measure the real (typically tiny) size instead.
	pub type FlowSlots<T: Config> = StorageNMap<
		_,
		(
			NMapKey<Blake2_128Concat, ProductId>,
			NMapKey<Blake2_128Concat, FlowId>,
			NMapKey<Blake2_128Concat, InstanceKey>,
			NMapKey<Blake2_128Concat, Lane>,
			NMapKey<Blake2_128Concat, SlotId>,
		),
		SlotRecord<BlockNumberFor<T>>,
		ValueQuery,
	>;

	#[pallet::storage]
	/// Count of a lane's required slots that are `satisfied` — the O(1) close
	/// counter (§8). `0` ⇒ the lane is untouched. Keyed by `Lane` (not the wire
	/// `track_key`) so a chain that's in two sub tracks is two counters.
	pub type FlowLaneRecorded<T: Config> = StorageNMap<
		_,
		(
			NMapKey<Blake2_128Concat, ProductId>,
			NMapKey<Blake2_128Concat, FlowId>,
			NMapKey<Blake2_128Concat, InstanceKey>,
			NMapKey<Blake2_128Concat, Lane>,
		),
		u16,
		ValueQuery,
	>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// An investor's currently in-flight flows across all products — added on
	/// open, removed on close. Only populated for `investor_scoped` flows.
	/// Mirrors `pallet_tranche_tx_registry::InvestorActiveRequests`.
	pub type InvestorActiveFlows<T: Config> =
		StorageMap<_, Blake2_128Concat, H160, Vec<(ProductId, FlowId, InstanceKey)>, ValueQuery>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// An investor's completed flows for one product — append-only, never
	/// pruned. Mirrors `pallet_tranche_tx_registry::InvestorRequestHistory`.
	pub type InvestorFlowHistory<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		H160,
		Blake2_128Concat,
		ProductId,
		Vec<(FlowId, InstanceKey)>,
		ValueQuery,
	>;

	// -----------------------------------------------------------------------
	// Calls
	// -----------------------------------------------------------------------

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Register or replace a flow's descriptor.
		///
		/// The `main_track.required_count` / `non_optional_lane_count` /
		/// `TrackChain::required_count` fields of `descriptor` are **ignored on
		/// input** and recomputed from the rest before storage.
		///
		/// A replacement is rejected (`DescriptorHasOpenInstances`) while the
		/// `(product_id, flow_id)` has any not-yet-`closed` instance: the derived
		/// close counters seed each open instance's `pending_lanes` and aren't
		/// re-derived, so changing them mid-flight would strand or early-close it.
		/// Wait for every instance to close (a stuck one blocks this until it's
		/// dealt with — §14), or move the change under a new `flow_id`. First
		/// registration and edits made while no instance is open are unrestricted.
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::set_flow_descriptor(
			descriptor.main_track.slots.len().saturating_add(
				descriptor.sub_tracks.iter().map(|sub_track| sub_track.slots.len()).sum::<usize>()
			) as u32
		))]
		pub fn set_flow_descriptor(
			origin: OriginFor<T>,
			product_id: ProductId,
			flow_id: FlowId,
			descriptor: FlowDescriptor,
		) -> DispatchResult {
			T::GovernanceOrigin::ensure_origin(origin)?;

			let replaced = FlowDescriptors::<T>::contains_key(product_id, flow_id);
			if replaced {
				ensure!(
					OpenInstanceCount::<T>::get(product_id, flow_id) == 0,
					Error::<T>::DescriptorHasOpenInstances
				);
			}
			let finalized = Self::validate_and_finalize_descriptor(descriptor)?;
			let version = finalized.version;
			FlowDescriptors::<T>::insert(product_id, flow_id, finalized);

			Self::deposit_event(if replaced {
				Event::FlowDescriptorUpdated { product_id, flow_id, version }
			} else {
				Event::FlowDescriptorSet { product_id, flow_id, version }
			});
			Ok(())
		}

		/// Record one observed on-chain tx for a flow step. See
		/// `docs/tranche-custom-flows/design-minimal.md` §8 for the full check
		/// order. Purely observational — completion is computed by the pallet,
		/// not signalled by the recorder.
		///
		/// There is no `open` flag: recording the first main slot
		/// (`main_track.slots[0]`) for an `instance_key` that has no instance yet
		/// creates it; every other record requires the instance to exist.
		/// `investor` is consumed only on that opening call (required iff
		/// `descriptor.investor_scoped`), ignored otherwise.
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::record_flow_tx(
			2 * MAX_SLOTS,
			attempt_metadata.as_ref().map_or(0, |metadata| metadata.len() as u32),
		))]
		pub fn record_flow_tx(
			origin: OriginFor<T>,
			product_id: ProductId,
			flow_id: FlowId,
			instance_key: InstanceKey,
			track_key: TrackKey,
			slot_id: SlotId,
			chain_id: ChainId,
			tx_hash: H256,
			success: bool,
			attempt_metadata: Option<BoundedVec<u8, ConstU32<MAX_ATTEMPT_METADATA>>>,
			slot_metadata: Option<BoundedVec<u8, ConstU32<MAX_SLOT_METADATA>>>,
			investor: Option<H160>,
		) -> DispatchResult {
			T::RecorderOrigin::ensure_origin(origin)?;
			Self::do_record_flow_tx(
				product_id,
				flow_id,
				instance_key,
				track_key,
				slot_id,
				chain_id,
				tx_hash,
				success,
				attempt_metadata,
				slot_metadata,
				investor,
			)
		}
	}
}
