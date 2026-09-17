use crate::{
	history, Attempt, ChainId, FlowDescriptor, FlowId, FlowInstance, HistoryPage, InstanceKey,
	Lane, PagedInvestorHistory, ProductId, SlotId, TrackKey, MAX_ATTEMPT_METADATA,
	MAX_SLOT_METADATA,
};
use bp_tranche::TxRecord;
use core::marker::PhantomData;

use super::pallet::*;
use frame_support::{
	ensure,
	pallet_prelude::{BoundedVec, DispatchResult},
};
use sp_core::{ConstU32, H160, H256};
use sp_std::vec::Vec;

/// Where a `slot_id` sits in a validated descriptor, plus the facts the close
/// counter (§8 step 7) needs about that lane.
struct Resolved {
	lane: Lane,
	/// `true` ⇒ `Lane::Main`.
	is_main: bool,
	/// `SlotDef::optional` of this step.
	slot_optional: bool,
	/// `TrackChain::optional` of this lane's chain — always `false` for the main
	/// lane.
	chain_optional: bool,
	/// The lane's required-slot count: `FlowLaneRecorded` reaching this means the
	/// lane is `done`. `main_track.required_count` for the main lane, the
	/// chain's `required_count` for a sub lane.
	target_count: u16,
}

// Private, non-extrinsic helpers — kept in their own `impl` block, separate from
// `#[pallet::call]`, so they don't become part of the `Call` enum.
impl<T: Config> Pallet<T> {
	// ---------------------------------------------------------------------
	// set_flow_descriptor
	// ---------------------------------------------------------------------

	/// Check `descriptor`'s structure and fill in its derived count fields
	/// (`main_track.required_count`, `non_optional_lane_count`, each
	/// `TrackChain::required_count`), overwriting whatever the caller passed.
	pub(crate) fn validate_and_finalize_descriptor(
		mut descriptor: FlowDescriptor,
	) -> Result<FlowDescriptor, Error<T>> {
		let main_slots = &descriptor.main_track.slots;
		ensure!(!main_slots.is_empty(), Error::<T>::MainTrackEmpty);
		ensure!(
			strictly_ascending(main_slots.iter().map(|slot| slot.id)),
			Error::<T>::BadSlotOrder
		);

		let main_required_count = main_slots.iter().filter(|slot| !slot.optional).count() as u16;
		ensure!(main_required_count >= 1, Error::<T>::NoRequiredMainSlot);
		descriptor.main_track.required_count = main_required_count;

		// slot_id disjointness across { main } ∪ { each sub track }.
		let mut seen_slot_ids: Vec<SlotId> =
			descriptor.main_track.slots.iter().map(|slot| slot.id).collect();

		// pending_lanes seed: main lane + every `!optional` sub-track chain lane.
		let mut non_optional_lane_count: u16 = 1;

		for sub_track_idx in 0..descriptor.sub_tracks.len() {
			let sub_track = &descriptor.sub_tracks[sub_track_idx];
			ensure!(
				!sub_track.slots.is_empty() && !sub_track.chains.is_empty(),
				Error::<T>::SubTrackEmpty
			);
			ensure!(
				strictly_ascending(sub_track.slots.iter().map(|slot| slot.id)),
				Error::<T>::BadSlotOrder
			);

			let sub_track_slot_ids: Vec<SlotId> =
				sub_track.slots.iter().map(|slot| slot.id).collect();
			for slot_id in &sub_track_slot_ids {
				ensure!(!seen_slot_ids.contains(slot_id), Error::<T>::SlotIdsOverlap);
			}
			seen_slot_ids.extend_from_slice(&sub_track_slot_ids);

			let mut seen_chain_ids: Vec<ChainId> = Vec::new();
			for chain_idx in 0..descriptor.sub_tracks[sub_track_idx].chains.len() {
				let sub_track = &descriptor.sub_tracks[sub_track_idx];
				let track_chain = &sub_track.chains[chain_idx];

				ensure!(
					!seen_chain_ids.contains(&track_chain.chain_id),
					Error::<T>::DuplicateChainInTrack
				);
				seen_chain_ids.push(track_chain.chain_id);

				// skip_slots ⊆ this sub track's slot ids
				for skip_slot_id in track_chain.skip_slots.iter() {
					ensure!(
						sub_track_slot_ids.contains(skip_slot_id),
						Error::<T>::SkipSlotNotInTrack
					);
				}

				// this chain's required slots = the sub track's `!optional` slots
				// it doesn't skip.
				let chain_required_count = sub_track
					.slots
					.iter()
					.filter(|slot| !slot.optional && !track_chain.skip_slots.contains(&slot.id))
					.count() as u16;
				ensure!(chain_required_count >= 1, Error::<T>::ChainNoRequiredSlot);

				let chain_is_optional = track_chain.optional;

				// shared borrows above are done — safe to write back.
				descriptor.sub_tracks[sub_track_idx].chains[chain_idx].required_count =
					chain_required_count;
				if !chain_is_optional {
					non_optional_lane_count = non_optional_lane_count.saturating_add(1);
				}
			}
		}

		descriptor.non_optional_lane_count = non_optional_lane_count;
		Ok(descriptor)
	}

	// ---------------------------------------------------------------------
	// record_flow_tx
	// ---------------------------------------------------------------------

	pub(crate) fn do_record_flow_tx(
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
		// step 2 — descriptor
		let descriptor =
			FlowDescriptors::<T>::get(product_id, flow_id).ok_or(Error::<T>::UnknownFlow)?;

		// step 3 — slot_id + track_key → Lane
		let resolved = Self::resolve_lane(&descriptor, slot_id, track_key)?;

		// step 4 — attestation format (checked before touching storage)
		ensure!(!tx_hash.is_zero(), Error::<T>::ZeroTxHash);

		// step 5 — instance open / exist. There is no `open` flag: the instance
		// is created by the first record of the first main slot, and every other
		// record requires it to already exist. The recorder only learns
		// `instance_key` from that opening event, so it cannot legitimately
		// record anything else first.
		let existing_instance = FlowInstances::<T>::get((product_id, flow_id, &instance_key));
		let was_closed =
			existing_instance.as_ref().map(|instance| instance.closed).unwrap_or(false);
		let now = frame_system::Pallet::<T>::block_number();

		let is_open = existing_instance.is_none();
		let mut instance = match existing_instance {
			Some(instance) => instance,
			None => {
				let opens_here = resolved.is_main
					&& descriptor.main_track.slots.first().map_or(false, |slot| slot.id == slot_id);
				ensure!(opens_here, Error::<T>::InstanceNotOpened);

				let stored_investor = if descriptor.investor_scoped {
					ensure!(investor.is_some(), Error::<T>::MissingInvestor);
					investor
				} else {
					None
				};
				FlowInstance {
					opened_at: now,
					investor: stored_investor,
					pending_lanes: descriptor.non_optional_lane_count,
					closed: false,
				}
			},
		};

		// step 6 — append
		let newly_satisfied = FlowSlots::<T>::try_mutate(
			(product_id, flow_id, &instance_key, resolved.lane, slot_id),
			|slot_record| -> Result<bool, Error<T>> {
				// Reject a re-submission of a tx already recorded for this exact
				// step. The close counter is immune to it (only the first
				// `success: true` moves `satisfied`), but a duplicate still
				// consumes an `attempts` slot toward `MAX_ATTEMPTS` and pollutes
				// the audit trail. Scoped to this `(instance, lane, slot)` — the
				// same `tx_hash` legitimately never satisfies two steps.
				ensure!(
					!slot_record.attempts.iter().any(|attempt| attempt.tx.tx_hash == tx_hash),
					Error::<T>::DuplicateAttestation
				);
				if let Some(new_slot_metadata) = slot_metadata {
					slot_record.metadata = new_slot_metadata;
				}
				slot_record
					.attempts
					.try_push(Attempt {
						tx: TxRecord { chain_id, tx_hash, recorded_at: now },
						success,
						metadata: attempt_metadata.unwrap_or_default(),
					})
					.map_err(|_| Error::<T>::TooManyAttempts)?;
				let newly_satisfied = success && !slot_record.satisfied;
				if newly_satisfied {
					slot_record.satisfied = true;
				}
				Ok(newly_satisfied)
			},
		)?;

		// step 7 — close counter
		let mut just_closed = false;
		if !instance.closed && newly_satisfied && !resolved.slot_optional {
			let lane_key = (product_id, flow_id, &instance_key, resolved.lane);
			let prev_count = FlowLaneRecorded::<T>::get(lane_key);
			if prev_count == 0
				&& matches!(resolved.lane, Lane::Sub { .. })
				&& resolved.chain_optional
			{
				instance.pending_lanes = instance.pending_lanes.saturating_add(1);
			}
			FlowLaneRecorded::<T>::insert(lane_key, prev_count.saturating_add(1));
			if prev_count.saturating_add(1) == resolved.target_count {
				instance.pending_lanes = instance.pending_lanes.saturating_sub(1);
			}
			if instance.pending_lanes == 0 {
				instance.closed = true;
				just_closed = true;
			}
		}

		// step 8 — persist instance + open-count + investor index placement
		FlowInstances::<T>::insert((product_id, flow_id, &instance_key), &instance);

		// `OpenInstanceCount` gates descriptor replacement (§7). Same lifecycle
		// edges as the `FlowOpened` / `FlowClosed` events below — an
		// open-and-immediately-close call does `+1` then `-1`, netting to 0.
		if is_open {
			OpenInstanceCount::<T>::mutate(product_id, flow_id, |n| *n = n.saturating_add(1));
		}
		if instance.closed && !was_closed {
			OpenInstanceCount::<T>::mutate(product_id, flow_id, |n| *n = n.saturating_sub(1));
		}

		if let Some(investor_addr) = instance.investor {
			if is_open {
				if instance.closed {
					Self::push_flow_history(investor_addr, product_id, flow_id, instance_key);
				} else {
					InvestorActiveFlows::<T>::mutate(investor_addr, |active| {
						active.push((product_id, flow_id, instance_key))
					});
				}
			} else if just_closed && !was_closed {
				InvestorActiveFlows::<T>::mutate(investor_addr, |active| {
					active.retain(|entry| {
						!(entry.0 == product_id && entry.1 == flow_id && entry.2 == instance_key)
					})
				});
				Self::push_flow_history(investor_addr, product_id, flow_id, instance_key);
			}
		}

		// step 9 — events
		if is_open {
			Self::deposit_event(Event::FlowOpened {
				product_id,
				flow_id,
				instance_key,
				investor: instance.investor,
			});
		}
		Self::deposit_event(Event::FlowTxRecorded {
			product_id,
			flow_id,
			instance_key,
			lane: resolved.lane,
			slot_id,
			chain_id,
			tx_hash,
			success,
		});
		if instance.closed && !was_closed {
			Self::deposit_event(Event::FlowClosed { product_id, flow_id, instance_key });
		}

		Ok(())
	}

	/// §8 step 3: locate `slot_id` in `descriptor`, validate `track_key` against
	/// it, and return the [`Lane`] + the close-counter facts about that lane.
	fn resolve_lane(
		descriptor: &FlowDescriptor,
		slot_id: SlotId,
		track_key: TrackKey,
	) -> Result<Resolved, Error<T>> {
		// main track?
		if let Some(slot) = descriptor.main_track.slots.iter().find(|slot| slot.id == slot_id) {
			ensure!(track_key.is_none(), Error::<T>::TrackKeyForMainSlot);
			return Ok(Resolved {
				lane: Lane::Main,
				is_main: true,
				slot_optional: slot.optional,
				chain_optional: false,
				target_count: descriptor.main_track.required_count,
			});
		}

		// which sub track owns this slot_id?
		let (sub_track_idx, slot_optional) = descriptor
			.sub_tracks
			.iter()
			.enumerate()
			.find_map(|(sub_track_idx, sub_track)| {
				sub_track
					.slots
					.iter()
					.find(|slot| slot.id == slot_id)
					.map(|slot| (sub_track_idx, slot.optional))
			})
			.ok_or(Error::<T>::UnknownSlot)?;

		let chain_id = track_key.ok_or(Error::<T>::MissingTrackKey)?;

		let track_chain = descriptor.sub_tracks[sub_track_idx]
			.chains
			.iter()
			.find(|track_chain| track_chain.chain_id == chain_id)
			.ok_or(Error::<T>::UndeclaredChainLane)?;
		ensure!(!track_chain.skip_slots.contains(&slot_id), Error::<T>::SlotSkippedForChain);

		Ok(Resolved {
			lane: Lane::Sub { track: sub_track_idx as u8, chain: chain_id },
			is_main: false,
			slot_optional,
			chain_optional: track_chain.optional,
			target_count: track_chain.required_count,
		})
	}

	// ---------------------------------------------------------------------
	// investor flow history (paged — see `bp_tranche::history`)
	// ---------------------------------------------------------------------

	/// Append one completed instance to `(investor, product_id, flow_id)`'s
	/// paged history.
	pub fn push_flow_history(
		investor: H160,
		product_id: ProductId,
		flow_id: FlowId,
		instance_key: InstanceKey,
	) {
		history::history_push::<FlowHistoryIndex<T>>((investor, product_id, flow_id), instance_key);
	}

	/// Read a page of `(investor, product_id, flow_id)`'s completed-instance
	/// history, most-recent-first: up to `limit` entries after skipping the
	/// newest `offset`, plus the full history length. `offset >= total` ⇒ empty.
	pub fn read_flow_history(
		investor: H160,
		product_id: ProductId,
		flow_id: FlowId,
		offset: u32,
		limit: u32,
	) -> (Vec<InstanceKey>, u32) {
		history::history_read::<FlowHistoryIndex<T>>((investor, product_id, flow_id), offset, limit)
	}
}

/// Wires this pallet's `InvestorFlowHistoryLen` / `InvestorFlowHistoryPage`
/// storage onto the shared paged-history logic in [`bp_tranche::history`].
pub struct FlowHistoryIndex<T>(PhantomData<T>);

impl<T: Config> PagedInvestorHistory for FlowHistoryIndex<T> {
	type Key = (H160, ProductId, FlowId);
	type Entry = InstanceKey;

	fn len((investor, product_id, flow_id): Self::Key) -> u32 {
		InvestorFlowHistoryLen::<T>::get((investor, product_id, flow_id))
	}

	fn set_len((investor, product_id, flow_id): Self::Key, len: u32) {
		InvestorFlowHistoryLen::<T>::insert((investor, product_id, flow_id), len);
	}

	fn page((investor, product_id, flow_id): Self::Key, page: u32) -> HistoryPage<Self::Entry> {
		InvestorFlowHistoryPage::<T>::get((investor, product_id, flow_id, page))
	}

	fn append_to_page((investor, product_id, flow_id): Self::Key, page: u32, entry: Self::Entry) {
		InvestorFlowHistoryPage::<T>::mutate((investor, product_id, flow_id, page), |entries| {
			// Caller guarantees `page` is the tail page and not full.
			let _ = entries.try_push(entry);
		});
	}
}

/// `true` iff `slot_ids` yields a strictly ascending sequence.
fn strictly_ascending(slot_ids: impl Iterator<Item = SlotId>) -> bool {
	let mut prev: Option<SlotId> = None;
	for slot_id in slot_ids {
		if let Some(prev_id) = prev {
			if slot_id <= prev_id {
				return false;
			}
		}
		prev = Some(slot_id);
	}
	true
}
