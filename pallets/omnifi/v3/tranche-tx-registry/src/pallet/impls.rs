use crate::{
	history, is_valid_tx_hash, Attempt, AttemptMetadata, ChainAddress, ChainId, CloseOutcome,
	FlowId, HistoryPage, Instance, InstanceKey, InstanceMetadata, InstanceRef, InstanceScope,
	InstanceStatus, Lane, LaneDeclarations, LaneState, PagedInvestorHistory, Part, ProductId,
	ProductModel, SlotId, Template, TemplateId, TxHash, TxRecord, MAX_INSTANCE_LANES,
	MAX_PART_COUNT,
};

use super::pallet::*;
use core::marker::PhantomData;
use frame_support::{ensure, pallet_prelude::DispatchResult};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_std::vec::Vec;

/// A lane an attestation or declaration resolves to.
struct ResolvedSlot {
	lane: Lane,
	required: bool,
	/// `Some(required_count)` of the slot's track for a sub-lane slot.
	track_required: Option<u16>,
}

impl<T: Config> Pallet<T> {
	// -----------------------------------------------------------------------
	// Templates
	// -----------------------------------------------------------------------

	/// Validates a template's structure and fills in its derived counts.
	pub fn finalize_template(mut template: Template) -> Result<Template, Error<T>> {
		let mut seen: Vec<SlotId> = Vec::new();
		let mut check_slots = |slots: &[crate::SlotDef]| -> Result<u16, Error<T>> {
			let mut prev: Option<SlotId> = None;
			let mut required = 0u16;
			for slot in slots {
				ensure!(!slot.name.is_empty(), Error::<T>::EmptySlotName);
				ensure!(prev.map_or(true, |p| slot.id > p), Error::<T>::SlotOrderNotAscending);
				ensure!(!seen.contains(&slot.id), Error::<T>::DuplicateSlotId);
				seen.push(slot.id);
				prev = Some(slot.id);
				if slot.required {
					required = required.saturating_add(1);
				}
			}
			Ok(required)
		};

		let main_required = check_slots(&template.main)?;
		ensure!(main_required >= 1, Error::<T>::NoRequiredMainSlot);
		template.main_required_count = main_required;

		for track in template.tracks.iter_mut() {
			ensure!(!track.name.is_empty(), Error::<T>::EmptySlotName);
			let required = check_slots(&track.slots)?;
			ensure!(required >= 1, Error::<T>::TrackWithoutRequiredSlot);
			track.required_count = required;
		}
		Ok(template)
	}

	pub(crate) fn do_register_template(id: TemplateId, template: Template) -> DispatchResult {
		ensure!(!Templates::<T>::contains_key(id), Error::<T>::TemplateExists);
		let template = Self::finalize_template(template)?;
		Templates::<T>::insert(id, template);
		Self::deposit_event(Event::TemplateRegistered { template: id });
		Ok(())
	}

	pub(crate) fn do_set_default_flow(
		model: ProductModel,
		flow_id: FlowId,
		template: Option<TemplateId>,
	) -> DispatchResult {
		DefaultFlows::<T>::try_mutate(model, |defaults| -> DispatchResult {
			match template {
				Some(template) => {
					ensure!(Templates::<T>::contains_key(template), Error::<T>::UnknownTemplate);
					defaults
						.try_insert(flow_id, template)
						.map_err(|_| Error::<T>::TooManyDefaultFlows)?;
				},
				None => {
					defaults.remove(&flow_id);
				},
			}
			Ok(())
		})?;
		Self::deposit_event(Event::DefaultFlowSet { model, flow_id, template });
		Ok(())
	}

	/// Gives `product_id` every default flow of `model` it doesn't have yet;
	/// returns how many were assigned. Called by the runtime's
	/// `pallet_tranche_system::OnProductCreated` glue at product registration.
	pub fn do_assign_default_flows(product_id: ProductId, model: ProductModel) -> u32 {
		let mut assigned = 0u32;
		for (flow_id, template) in DefaultFlows::<T>::get(model) {
			if ProductFlows::<T>::contains_key(product_id, flow_id) {
				continue;
			}
			ProductFlows::<T>::insert(product_id, flow_id, template);
			Self::deposit_event(Event::FlowAssigned {
				product_id,
				flow_id,
				template: Some(template),
			});
			assigned = assigned.saturating_add(1);
		}
		assigned
	}

	// -----------------------------------------------------------------------
	// Recording
	// -----------------------------------------------------------------------

	pub(crate) fn do_record_tx(
		product_id: ProductId,
		flow_id: FlowId,
		instance_key: InstanceKey,
		lane: Option<ChainId>,
		slot_id: SlotId,
		chain_id: ChainId,
		tx_hash: TxHash,
		completes: bool,
		part: Option<Part>,
		metadata: Option<AttemptMetadata>,
		investor: Option<ChainAddress>,
		instance_metadata: Option<InstanceMetadata>,
		scope: Option<InstanceScope>,
		declare_lanes: LaneDeclarations,
		link: Option<InstanceRef>,
	) -> DispatchResult {
		ensure!(is_valid_tx_hash(&tx_hash), Error::<T>::ZeroTxHash);
		if let Some((index, count)) = part {
			ensure!(
				count >= 1 && count <= MAX_PART_COUNT && index < count,
				Error::<T>::InvalidPart
			);
		}
		let now = frame_system::Pallet::<T>::block_number();
		let key = (product_id, flow_id, instance_key);

		// --- 1. instance (open on first main-track attestation) ---
		let existing = Instances::<T>::get(key);
		let opened = existing.is_none();
		let (mut instance, template) = match existing {
			Some(instance) => {
				let template =
					Templates::<T>::get(instance.template).ok_or(Error::<T>::UnknownTemplate)?;
				(instance, template)
			},
			None => {
				ensure!(lane.is_none(), Error::<T>::InstanceNotOpened);
				let template_id = ProductFlows::<T>::get(product_id, flow_id)
					.ok_or(Error::<T>::FlowNotAssigned)?;
				let template =
					Templates::<T>::get(template_id).ok_or(Error::<T>::UnknownTemplate)?;
				let investor = if template.investor_scoped {
					Some(investor.ok_or(Error::<T>::MissingInvestor)?)
				} else {
					ensure!(scope.is_none(), Error::<T>::ScopeWithoutInvestor);
					None
				};
				let instance = Instance {
					template: template_id,
					investor,
					metadata: Default::default(),
					status: InstanceStatus::Open,
					opened_at: now,
					closed_at: None,
					main_done: 0,
					main_stage: None,
					lane_count: 0,
					pending_lanes: 0,
					links: Default::default(),
				};
				(instance, template)
			},
		};
		// A closed instance keeps accepting evidence (slots, lanes), but its
		// progress counters — `main_done`, `main_stage`, `pending_lanes` — stay
		// as they were at close.
		let was_open = instance.status == InstanceStatus::Open;
		// `scope` only matters on the opening attestation (like `investor`);
		// the recorder may resend it on later records harmlessly.
		let scope = if opened { scope } else { None };

		// --- 2. resolve the attested slot ---
		let resolved = Self::resolve_slot(&template, lane, slot_id)?;

		// --- 3. lane declarations (explicit, then the attested lane itself) ---
		let mut declared: Vec<(u8, ChainId)> = Vec::new();
		let mut lane_states: Vec<(Lane, LaneState)> = Vec::new();
		for (track, chain) in declare_lanes.iter().copied() {
			ensure!((track as usize) < template.tracks.len(), Error::<T>::UnknownTrack);
			ensure!(chain != 0, Error::<T>::InvalidLaneChain);
			if Self::ensure_lane(
				key,
				&mut instance,
				&mut lane_states,
				Lane::Sub { track, chain },
				was_open,
			)? {
				declared.push((track, chain));
			}
		}
		if let Lane::Sub { track, chain } = resolved.lane {
			if Self::ensure_lane(key, &mut instance, &mut lane_states, resolved.lane, was_open)? {
				declared.push((track, chain));
			}
		}

		// --- 4. append the attempt ---
		let newly_completed = Slots::<T>::try_mutate(
			(product_id, flow_id, instance_key, resolved.lane, slot_id),
			|record| -> Result<bool, Error<T>> {
				ensure!(
					!record.attempts.iter().any(|a| a.tx.chain_id == chain_id &&
						a.tx.tx_hash == tx_hash &&
						a.part == part),
					Error::<T>::DuplicateAttestation
				);
				let part_count = part.map(|(_, count)| count);
				if record.attempts.is_empty() {
					record.part_count = part_count;
				} else {
					ensure!(record.part_count == part_count, Error::<T>::PartCountMismatch);
				}
				record
					.attempts
					.try_push(Attempt {
						tx: TxRecord { chain_id, tx_hash: tx_hash.clone(), recorded_at: now },
						completes,
						part,
						metadata: metadata.unwrap_or_default(),
					})
					.map_err(|_| Error::<T>::TooManyAttempts)?;
				let satisfied = match part {
					Some((index, count)) => {
						if completes {
							record.parts_done |= 1u64 << index;
						}
						record.parts_done.count_ones() >= count as u32
					},
					None => completes,
				};
				let newly_completed = satisfied && !record.completed;
				if newly_completed {
					record.completed = true;
				}
				Ok(newly_completed)
			},
		)?;

		// --- 5. progress ---
		if newly_completed {
			match resolved.lane {
				Lane::Main =>
					if was_open {
						instance.main_stage =
							Some(instance.main_stage.map_or(slot_id, |s| s.max(slot_id)));
						if resolved.required {
							instance.main_done = instance.main_done.saturating_add(1);
						}
					},
				Lane::Sub { .. } => {
					let state = lane_states
						.iter_mut()
						.find(|(lane, _)| *lane == resolved.lane)
						.map(|(_, state)| state)
						.expect("ensure_lane loaded the attested lane above; qed");
					state.stage = Some(state.stage.map_or(slot_id, |s| s.max(slot_id)));
					if resolved.required {
						state.done_required = state.done_required.saturating_add(1);
						let target = resolved.track_required.unwrap_or(u16::MAX);
						if !state.done && state.done_required >= target {
							state.done = true;
							if was_open {
								instance.pending_lanes = instance.pending_lanes.saturating_sub(1);
							}
						}
					}
				},
			}
		}

		// --- 6. instance metadata / link ---
		if let Some(instance_metadata) = instance_metadata {
			instance.metadata = instance_metadata;
		}
		let linked = match link {
			Some(target) if !instance.links.contains(&target) => {
				instance.links.try_push(target).map_err(|_| Error::<T>::TooManyLinks)?;
				history::history_push::<LinkedFromIndex<T>>(
					(product_id, target.0, target.1),
					(flow_id, instance_key),
				);
				Some(target)
			},
			_ => None,
		};

		// --- 7. completion ---
		let just_completed = was_open &&
			instance.main_done >= template.main_required_count &&
			instance.pending_lanes == 0;
		if just_completed {
			instance.status = InstanceStatus::Completed;
			instance.closed_at = Some(now);
		}

		// --- 8. writes ---
		for (lane, state) in lane_states {
			Lanes::<T>::insert((product_id, flow_id, instance_key, lane), state);
		}
		Instances::<T>::insert(key, &instance);
		if let Some(investor) = instance.investor {
			if let Some(scope) = scope {
				LatestByScope::<T>::insert((investor, product_id, flow_id, scope), instance_key);
			}
			if opened && !just_completed {
				Self::insert_active(investor, key);
			} else if just_completed {
				if !opened {
					Self::remove_active(investor, key);
				}
				Self::push_history(investor, product_id, flow_id, instance_key);
			}
		}

		// --- 9. events ---
		if opened {
			Self::deposit_event(Event::InstanceOpened {
				product_id,
				flow_id,
				instance_key,
				template: instance.template,
				investor: instance.investor,
				scope,
			});
		}
		for (track, chain) in declared {
			Self::deposit_event(Event::LaneDeclared {
				product_id,
				flow_id,
				instance_key,
				track,
				chain,
			});
		}
		Self::deposit_event(Event::TxRecorded {
			product_id,
			flow_id,
			instance_key,
			lane: resolved.lane,
			slot_id,
			chain_id,
			tx_hash,
			completes,
			part,
		});
		if let Some(target) = linked {
			Self::deposit_event(Event::InstanceLinked {
				product_id,
				flow_id,
				instance_key,
				target,
			});
		}
		if just_completed {
			Self::deposit_event(Event::InstanceClosed {
				product_id,
				flow_id,
				instance_key,
				outcome: CloseOutcome::Completed,
			});
		}
		Ok(())
	}

	pub(crate) fn do_close_instance(
		product_id: ProductId,
		flow_id: FlowId,
		instance_key: InstanceKey,
		outcome: CloseOutcome,
	) -> DispatchResult {
		let key = (product_id, flow_id, instance_key);
		Instances::<T>::try_mutate(key, |maybe| -> DispatchResult {
			let instance = maybe.as_mut().ok_or(Error::<T>::InstanceNotOpened)?;
			ensure!(instance.status == InstanceStatus::Open, Error::<T>::InstanceAlreadyClosed);
			instance.status = outcome.into();
			instance.closed_at = Some(frame_system::Pallet::<T>::block_number());
			if let Some(investor) = instance.investor {
				Self::remove_active(investor, key);
				Self::push_history(investor, product_id, flow_id, instance_key);
			}
			Ok(())
		})?;
		Self::deposit_event(Event::InstanceClosed { product_id, flow_id, instance_key, outcome });
		Ok(())
	}

	fn resolve_slot(
		template: &Template,
		lane: Option<ChainId>,
		slot_id: SlotId,
	) -> Result<ResolvedSlot, Error<T>> {
		if let Some(slot) = template.main_slot(slot_id) {
			ensure!(lane.is_none(), Error::<T>::LaneMismatch);
			return Ok(ResolvedSlot {
				lane: Lane::Main,
				required: slot.required,
				track_required: None,
			});
		}
		let (track, slot) = template.track_slot(slot_id).ok_or(Error::<T>::UnknownSlot)?;
		let chain = lane.ok_or(Error::<T>::LaneMismatch)?;
		ensure!(chain != 0, Error::<T>::InvalidLaneChain);
		Ok(ResolvedSlot {
			lane: Lane::Sub { track, chain },
			required: slot.required,
			track_required: Some(template.tracks[track as usize].required_count),
		})
	}

	/// Loads `lane` into `lane_states` (creating it if this instance doesn't
	/// have it yet). Returns whether it was newly created. A lane created on a
	/// closed instance (`counts == false`) doesn't become pending.
	fn ensure_lane(
		key: (ProductId, FlowId, InstanceKey),
		instance: &mut Instance<BlockNumberFor<T>>,
		lane_states: &mut Vec<(Lane, LaneState)>,
		lane: Lane,
		counts: bool,
	) -> Result<bool, Error<T>> {
		if lane_states.iter().any(|(l, _)| *l == lane) {
			return Ok(false);
		}
		match Lanes::<T>::get((key.0, key.1, key.2, lane)) {
			Some(state) => {
				lane_states.push((lane, state));
				Ok(false)
			},
			None => {
				ensure!(instance.lane_count < MAX_INSTANCE_LANES, Error::<T>::TooManyLanes);
				instance.lane_count = instance.lane_count.saturating_add(1);
				if counts {
					instance.pending_lanes = instance.pending_lanes.saturating_add(1);
				}
				lane_states.push((lane, LaneState::default()));
				Ok(true)
			},
		}
	}

	fn insert_active(
		investor: ChainAddress,
		(product_id, flow_id, instance_key): (ProductId, FlowId, InstanceKey),
	) {
		InvestorActiveInstances::<T>::insert((investor, product_id, flow_id, instance_key), ());
		InvestorActiveCount::<T>::mutate((investor, product_id, flow_id), |n| {
			*n = n.saturating_add(1)
		});
	}

	fn remove_active(
		investor: ChainAddress,
		(product_id, flow_id, instance_key): (ProductId, FlowId, InstanceKey),
	) {
		if InvestorActiveInstances::<T>::take((investor, product_id, flow_id, instance_key))
			.is_some()
		{
			InvestorActiveCount::<T>::mutate((investor, product_id, flow_id), |n| {
				*n = n.saturating_sub(1)
			});
		}
	}

	/// `investor`'s open instances of one flow — up to `limit` keys after
	/// skipping `offset`, in storage order — plus their total count.
	pub fn read_active(
		investor: ChainAddress,
		product_id: ProductId,
		flow_id: FlowId,
		offset: u32,
		limit: u32,
	) -> (Vec<InstanceKey>, u32) {
		let total = InvestorActiveCount::<T>::get((investor, product_id, flow_id));
		let keys = InvestorActiveInstances::<T>::iter_key_prefix((investor, product_id, flow_id))
			.skip(offset as usize)
			.take(limit as usize)
			.collect();
		(keys, total)
	}

	// -----------------------------------------------------------------------
	// Paged indices
	// -----------------------------------------------------------------------

	pub fn push_history(
		investor: ChainAddress,
		product_id: ProductId,
		flow_id: FlowId,
		instance_key: InstanceKey,
	) {
		history::history_push::<InstanceHistoryIndex<T>>(
			(investor, product_id, flow_id),
			instance_key,
		);
	}

	/// An investor's closed instances of one flow, most recent first.
	pub fn read_history(
		investor: ChainAddress,
		product_id: ProductId,
		flow_id: FlowId,
		offset: u32,
		limit: u32,
	) -> (Vec<InstanceKey>, u32) {
		history::history_read::<InstanceHistoryIndex<T>>(
			(investor, product_id, flow_id),
			offset,
			limit,
		)
	}

	/// Instances linking to `(product_id, flow_id, instance_key)` (a
	/// settlement's requests), most recent first.
	pub fn read_linked_from(
		product_id: ProductId,
		flow_id: FlowId,
		instance_key: InstanceKey,
		offset: u32,
		limit: u32,
	) -> (Vec<InstanceRef>, u32) {
		history::history_read::<LinkedFromIndex<T>>(
			(product_id, flow_id, instance_key),
			offset,
			limit,
		)
	}
}

pub struct InstanceHistoryIndex<T>(PhantomData<T>);

impl<T: Config> PagedInvestorHistory for InstanceHistoryIndex<T> {
	type Key = (ChainAddress, ProductId, FlowId);
	type Entry = InstanceKey;

	fn len(key: Self::Key) -> u32 {
		InvestorHistoryLen::<T>::get(key)
	}
	fn set_len(key: Self::Key, len: u32) {
		InvestorHistoryLen::<T>::insert(key, len);
	}
	fn page((investor, product_id, flow_id): Self::Key, page: u32) -> HistoryPage<Self::Entry> {
		InvestorHistoryPage::<T>::get((investor, product_id, flow_id, page))
	}
	fn append_to_page((investor, product_id, flow_id): Self::Key, page: u32, entry: Self::Entry) {
		InvestorHistoryPage::<T>::mutate((investor, product_id, flow_id, page), |entries| {
			let _ = entries.try_push(entry);
		});
	}
}

pub struct LinkedFromIndex<T>(PhantomData<T>);

impl<T: Config> PagedInvestorHistory for LinkedFromIndex<T> {
	type Key = (ProductId, FlowId, InstanceKey);
	type Entry = InstanceRef;

	fn len(key: Self::Key) -> u32 {
		LinkedFromLen::<T>::get(key)
	}
	fn set_len(key: Self::Key, len: u32) {
		LinkedFromLen::<T>::insert(key, len);
	}
	fn page((product_id, flow_id, instance_key): Self::Key, page: u32) -> HistoryPage<Self::Entry> {
		LinkedFromPage::<T>::get((product_id, flow_id, instance_key, page))
	}
	fn append_to_page(
		(product_id, flow_id, instance_key): Self::Key,
		page: u32,
		entry: Self::Entry,
	) {
		LinkedFromPage::<T>::mutate((product_id, flow_id, instance_key, page), |entries| {
			let _ = entries.try_push(entry);
		});
	}
}
