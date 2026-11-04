//! EVM precompile for `pallet-tranche-tx-registry-v3` — universal ABI only
//! (`interface.sol`): investors are `bytes32` `ChainAddress`es (EVM addresses
//! left-padded) and tx hashes are `bytes`.
//!
//! Three write functions — `record_tx`, `record_tx_batch`, `close_instance` —
//! callable only by the shared tx recorder (the pallet's `RecorderOrigin`
//! rejects everyone else at dispatch; no pre-check here, same as the other
//! tx-registry precompiles). Template registration/assignment is governance
//! only and not surfaced. Read-only views follow the template → instance →
//! lane → slot → attempt hierarchy, plus the investor and link indices.
//!
//! ABI conventions:
//! - `flow_id` / template names are `bytes16` slugs, left-aligned.
//! - `lane_chain_id == 0` means the main lane.
//! - `investor == bytes32(0)` means "none"; empty `metadata` / `instance_metadata` mean "none"
//!   (instance metadata left untouched).
//! - `link_key == bytes32(0)` means "no link".
//! - `part_count == 0` means "single-part step" (`part_index` ignored).
//! - `track == 255` in a slot view means the main track.

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::dispatch::{GetDispatchInfo, PostDispatchInfo};
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_evm::AddressMapping;
use pallet_tranche_tx_registry_v3::{
	history::HISTORY_PAGE_SIZE, Call as RegistryCall, ChainId, CloseOutcome, FlowId, InstanceKey,
	InstanceStatus, Lane, Part, ProductId, ProductModel, SlotDef, SlotId, SlotName, SlotRecord,
	TemplateId, TxHash, MAX_INSTANCE_LANES, MAX_MAIN_SLOTS, MAX_TRACK_SLOTS, MAX_TX_HASH_LEN,
};
use parity_scale_codec::Encode;
use precompile_utils::{
	prelude::*,
	solidity::{
		codec::{Reader, UnboundedString, Writer},
		Codec,
	},
};
use sp_core::{ConstU32, H256, U256};
use sp_runtime::{traits::Dispatchable, BoundedVec};
use sp_std::{marker::PhantomData, vec::Vec};

// ---------------------------------------------------------------------------
// bytes16
// ---------------------------------------------------------------------------

/// A 16-byte slug as a Solidity `bytes16` — one left-aligned ABI word.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Bytes16(pub [u8; 16]);

impl Codec for Bytes16 {
	fn read(reader: &mut Reader) -> MayRevert<Self> {
		let word = <H256 as Codec>::read(reader)?;
		if word.0[16..].iter().any(|b| *b != 0) {
			return Err(RevertReason::custom("bytes16 padding must be zero").into());
		}
		let mut id = [0u8; 16];
		id.copy_from_slice(&word.0[..16]);
		Ok(Bytes16(id))
	}

	fn write(writer: &mut Writer, value: Self) {
		<H256 as Codec>::write(writer, bytes16_word(value.0));
	}

	fn has_static_size() -> bool {
		true
	}

	fn signature() -> String {
		String::from("bytes16")
	}
}

fn bytes16_word(id: [u8; 16]) -> H256 {
	let mut word = [0u8; 32];
	word[..16].copy_from_slice(&id);
	H256(word)
}

// ---------------------------------------------------------------------------
// ABI tuples (see interface.sol)
// ---------------------------------------------------------------------------

type EvmTxHash = BoundedBytes<ConstU32<MAX_TX_HASH_LEN>>;
/// `SlotDefView` — (id, name, required)
type EvmSlotDef = (u8, UnboundedString, bool);
/// `TrackView` — (name, required_count, slots)
type EvmTrack = (UnboundedString, u16, Vec<EvmSlotDef>);
/// `get_template` return — (exists, investor_scoped, main_required_count, main, tracks)
type EvmTemplate = (bool, bool, u16, Vec<EvmSlotDef>, Vec<EvmTrack>);
/// `DefaultFlowView` — (flow_id, template_name, template_version)
type EvmDefaultFlow = (Bytes16, Bytes16, u16);
/// `InstanceRefView` — (flow_id, instance_key)
type EvmRef = (Bytes16, H256);
/// `get_instance` return — see interface.sol `get_instance`.
type EvmInstance = (
	bool,
	Bytes16,
	u16,
	H256,
	u8,
	u64,
	u64,
	u16,
	u16,
	bool,
	u8,
	UnboundedString,
	u16,
	u16,
	UnboundedBytes,
	Vec<EvmRef>,
);
/// `LaneView` — (track, chain, done, done_required, required_count, has_stage, stage, stage_name)
type EvmLane = (u8, u64, bool, u16, u16, bool, u8, UnboundedString);
/// `AttemptView` — (chain_id, tx_hash, recorded_at, completes, part_index, part_count, metadata)
type EvmAttempt = (u64, EvmTxHash, u64, bool, u16, u16, UnboundedBytes);
/// `SlotView` — (track, lane_chain_id, slot_id, name, completed, part_count, parts_done, attempts)
type EvmSlot = (u8, u64, u8, UnboundedString, bool, u16, u64, Vec<EvmAttempt>);

/// `keccak256("TxRecorded(uint64,bytes16,bytes32,uint64,uint8,uint64,bytes,bool,uint16,uint16)")`
pub const SELECTOR_LOG_TX_RECORDED: [u8; 32] =
	keccak256!("TxRecorded(uint64,bytes16,bytes32,uint64,uint8,uint64,bytes,bool,uint16,uint16)");
/// `keccak256("InstanceClosed(uint64,bytes16,bytes32,uint8)")`
pub const SELECTOR_LOG_INSTANCE_CLOSED: [u8; 32] =
	keccak256!("InstanceClosed(uint64,bytes16,bytes32,uint8)");

/// `record_tx_batch` instances per EVM call — keeps one call well inside a
/// block (the pallet itself accepts up to `MAX_BATCH_INSTANCES`).
pub const MAX_EVM_BATCH: usize = 50;
/// `get_instance_slots` backstops: slot entries scanned (every slot of a
/// fully laned instance), and cumulative metadata bytes assembled.
const MAX_SLOT_SCAN: usize =
	(MAX_MAIN_SLOTS as usize) + (MAX_INSTANCE_LANES as usize) * (MAX_TRACK_SLOTS as usize);
const MAX_SLOT_SCAN_BYTES: usize = 2 * 1024 * 1024;
/// Gas per 32-byte word of storage data copied into the ABI output (the EVM
/// `COPY` rate).
const COPY_GAS_PER_WORD: u64 = 3;

pub const MAIN_TRACK: u8 = u8::MAX;

pub struct TrancheTxRegistryV3Precompile<Runtime>(PhantomData<Runtime>);

#[precompile_utils::precompile]
impl<Runtime> TrancheTxRegistryV3Precompile<Runtime>
where
	Runtime: pallet_tranche_tx_registry_v3::Config + pallet_evm::Config + frame_system::Config,
	Runtime::RuntimeCall: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	Runtime::RuntimeCall: From<RegistryCall<Runtime>>,
	BlockNumberFor<Runtime>: Into<U256>,
	<Runtime as pallet_evm::Config>::AddressMapping: AddressMapping<Runtime::AccountId>,
{
	// -----------------------------------------------------------------------
	// Writes (recorder only)
	// -----------------------------------------------------------------------

	#[precompile::public(
		"record_tx(uint64,bytes16,bytes32,uint64,uint8,uint64,bytes,bool,uint16,uint16,bytes,bytes32,bytes,bytes32,(uint8,uint64)[],bytes16,bytes32)"
	)]
	fn record_tx(
		handle: &mut impl PrecompileHandle,
		product_id: ProductId,
		flow_id: Bytes16,
		instance_key: H256,
		lane_chain_id: u64,
		slot_id: u8,
		chain_id: u64,
		tx_hash: EvmTxHash,
		completes: bool,
		part_index: u16,
		part_count: u16,
		metadata: UnboundedBytes,
		investor: H256,
		instance_metadata: UnboundedBytes,
		scope: H256,
		declare_lanes: Vec<(u8, u64)>,
		link_flow_id: Bytes16,
		link_key: H256,
	) -> EvmResult {
		let call = RegistryCall::<Runtime>::record_tx {
			product_id,
			flow_id: flow_id.0,
			instance_key,
			lane: lane(lane_chain_id),
			slot_id,
			chain_id,
			tx_hash: TxHash::truncate_from(tx_hash.clone().into()),
			completes,
			part: part(part_index, part_count),
			metadata: optional_bytes(metadata, "metadata exceeds MAX_ATTEMPT_METADATA")?,
			investor: if investor.is_zero() { None } else { Some(investor) },
			instance_metadata: optional_bytes(
				instance_metadata,
				"instance_metadata exceeds MAX_INSTANCE_METADATA",
			)?,
			scope: if scope.is_zero() { None } else { Some(scope) },
			declare_lanes: BoundedVec::try_from(declare_lanes)
				.map_err(|_| revert("declare_lanes exceeds MAX_LANE_DECLARATIONS"))?,
			link: link(link_flow_id, link_key),
		};
		let was_open = Self::is_open_or_new(handle, product_id, flow_id.0, instance_key)?;
		Self::dispatch(handle, call)?;
		Self::log_recorded(
			handle,
			product_id,
			flow_id.0,
			instance_key,
			lane_chain_id,
			slot_id,
			chain_id,
			&tx_hash,
			completes,
			part_index,
			part_count,
		)?;
		Self::log_if_completed(handle, product_id, flow_id.0, instance_key, was_open)
	}

	/// At most `MAX_EVM_BATCH` (50) keys; a key listed twice is attested once.
	#[precompile::public(
		"record_tx_batch(uint64,bytes16,bytes32[],uint64,uint8,uint64,bytes,bool,uint16,uint16,bytes,bytes16,bytes32)"
	)]
	fn record_tx_batch(
		handle: &mut impl PrecompileHandle,
		product_id: ProductId,
		flow_id: Bytes16,
		instance_keys: Vec<H256>,
		lane_chain_id: u64,
		slot_id: u8,
		chain_id: u64,
		tx_hash: EvmTxHash,
		completes: bool,
		part_index: u16,
		part_count: u16,
		metadata: UnboundedBytes,
		link_flow_id: Bytes16,
		link_key: H256,
	) -> EvmResult {
		if instance_keys.len() > MAX_EVM_BATCH {
			return Err(revert("instance_keys exceeds MAX_EVM_BATCH (50)"));
		}
		let mut unique: Vec<H256> = Vec::with_capacity(instance_keys.len());
		for key in &instance_keys {
			if !unique.contains(key) {
				unique.push(*key);
			}
		}
		let mut was_open = Vec::with_capacity(unique.len());
		for key in &unique {
			was_open.push(Self::is_open_or_new(handle, product_id, flow_id.0, *key)?);
		}
		let call = RegistryCall::<Runtime>::record_tx_batch {
			product_id,
			flow_id: flow_id.0,
			instance_keys: BoundedVec::try_from(instance_keys.clone())
				.map_err(|_| revert("instance_keys exceeds MAX_BATCH_INSTANCES"))?,
			lane: lane(lane_chain_id),
			slot_id,
			chain_id,
			tx_hash: TxHash::truncate_from(tx_hash.clone().into()),
			completes,
			part: part(part_index, part_count),
			metadata: optional_bytes(metadata, "metadata exceeds MAX_ATTEMPT_METADATA")?,
			link: link(link_flow_id, link_key),
		};
		Self::dispatch(handle, call)?;
		for (instance_key, was_open) in unique.into_iter().zip(was_open) {
			Self::log_recorded(
				handle,
				product_id,
				flow_id.0,
				instance_key,
				lane_chain_id,
				slot_id,
				chain_id,
				&tx_hash,
				completes,
				part_index,
				part_count,
			)?;
			Self::log_if_completed(handle, product_id, flow_id.0, instance_key, was_open)?;
		}
		Ok(())
	}

	/// `outcome`: 0 = Completed, 1 = Failed.
	#[precompile::public("close_instance(uint64,bytes16,bytes32,uint8)")]
	fn close_instance(
		handle: &mut impl PrecompileHandle,
		product_id: ProductId,
		flow_id: Bytes16,
		instance_key: H256,
		outcome: u8,
	) -> EvmResult {
		let outcome = match outcome {
			0 => CloseOutcome::Completed,
			1 => CloseOutcome::Failed,
			_ => return Err(revert("invalid outcome")),
		};
		let call = RegistryCall::<Runtime>::close_instance {
			product_id,
			flow_id: flow_id.0,
			instance_key,
			outcome,
		};
		Self::dispatch(handle, call)?;
		Self::log_closed(handle, product_id, flow_id.0, instance_key, outcome)
	}

	// -----------------------------------------------------------------------
	// Views — templates / assignment
	// -----------------------------------------------------------------------

	#[precompile::public("get_template(bytes16,uint16)")]
	#[precompile::view]
	fn get_template(
		handle: &mut impl PrecompileHandle,
		name: Bytes16,
		version: u16,
	) -> EvmResult<EvmTemplate> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let Some(template) = pallet_tranche_tx_registry_v3::Templates::<Runtime>::get(TemplateId {
			name: name.0,
			version,
		}) else {
			return Ok((false, false, 0, Vec::new(), Vec::new()));
		};
		let main = template.main.iter().map(slot_def).collect();
		let tracks = template
			.tracks
			.iter()
			.map(|track| {
				(
					string(&track.name),
					track.required_count,
					track.slots.iter().map(slot_def).collect(),
				)
			})
			.collect();
		Ok((true, template.investor_scoped, template.main_required_count, main, tracks))
	}

	/// The template new instances of `(product_id, flow_id)` open with —
	/// `(false, 0, 0)` if unassigned.
	#[precompile::public("get_product_flow(uint64,bytes16)")]
	#[precompile::view]
	fn get_product_flow(
		handle: &mut impl PrecompileHandle,
		product_id: ProductId,
		flow_id: Bytes16,
	) -> EvmResult<(bool, Bytes16, u16)> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		Ok(
			match pallet_tranche_tx_registry_v3::ProductFlows::<Runtime>::get(product_id, flow_id.0)
			{
				Some(id) => (true, Bytes16(id.name), id.version),
				None => (false, Bytes16([0; 16]), 0),
			},
		)
	}

	/// `model`: 0 = Multichain, 1 = SingleChain.
	#[precompile::public("get_default_flows(uint8)")]
	#[precompile::view]
	fn get_default_flows(
		handle: &mut impl PrecompileHandle,
		model: u8,
	) -> EvmResult<Vec<EvmDefaultFlow>> {
		let model = match model {
			0 => ProductModel::Multichain,
			1 => ProductModel::SingleChain,
			_ => return Err(revert("invalid model")),
		};
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		Ok(pallet_tranche_tx_registry_v3::DefaultFlows::<Runtime>::get(model)
			.into_iter()
			.map(|(flow_id, id)| (Bytes16(flow_id), Bytes16(id.name), id.version))
			.collect())
	}

	// -----------------------------------------------------------------------
	// Views — instances
	// -----------------------------------------------------------------------

	/// Summary of one instance: status, progress counters, and its current
	/// step (`main_stage`, the highest completed main slot, with its name).
	/// All-zero (`exists == false`) if the instance doesn't exist.
	#[precompile::public("get_instance(uint64,bytes16,bytes32)")]
	#[precompile::view]
	fn get_instance(
		handle: &mut impl PrecompileHandle,
		product_id: ProductId,
		flow_id: Bytes16,
		instance_key: H256,
	) -> EvmResult<EvmInstance> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost().saturating_mul(2))?;
		let Some(instance) = pallet_tranche_tx_registry_v3::Instances::<Runtime>::get((
			product_id,
			flow_id.0,
			instance_key,
		)) else {
			return Ok((
				false,
				Bytes16([0; 16]),
				0,
				H256::zero(),
				0,
				0,
				0,
				0,
				0,
				false,
				0,
				UnboundedString::from(Vec::new()),
				0,
				0,
				UnboundedBytes::from(Vec::new()),
				Vec::new(),
			));
		};
		let template = pallet_tranche_tx_registry_v3::Templates::<Runtime>::get(instance.template);
		let main_required = template.as_ref().map_or(0, |t| t.main_required_count);
		let stage_name = match (template.as_ref(), instance.main_stage) {
			(Some(t), Some(stage)) => t.slot_name(stage).map(string),
			_ => None,
		}
		.unwrap_or_else(|| UnboundedString::from(Vec::new()));
		Ok((
			true,
			Bytes16(instance.template.name),
			instance.template.version,
			instance.investor.unwrap_or_default(),
			status_code(instance.status),
			block::<Runtime>(instance.opened_at),
			instance.closed_at.map_or(0, block::<Runtime>),
			main_required,
			instance.main_done,
			instance.main_stage.is_some(),
			instance.main_stage.unwrap_or(0),
			stage_name,
			instance.lane_count,
			instance.pending_lanes,
			UnboundedBytes::from(instance.metadata.into_inner()),
			instance.links.into_iter().map(|(f, k)| (Bytes16(f), k)).collect(),
		))
	}

	/// Every lane of one instance with its progress and current step.
	#[precompile::public("get_instance_lanes(uint64,bytes16,bytes32)")]
	#[precompile::view]
	fn get_instance_lanes(
		handle: &mut impl PrecompileHandle,
		product_id: ProductId,
		flow_id: Bytes16,
		instance_key: H256,
	) -> EvmResult<Vec<EvmLane>> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost().saturating_mul(2))?;
		let Some(instance) = pallet_tranche_tx_registry_v3::Instances::<Runtime>::get((
			product_id,
			flow_id.0,
			instance_key,
		)) else {
			return Ok(Vec::new());
		};
		let template = pallet_tranche_tx_registry_v3::Templates::<Runtime>::get(instance.template);
		let mut lanes = Vec::new();
		let mut iter = pallet_tranche_tx_registry_v3::Lanes::<Runtime>::iter_prefix((
			product_id,
			flow_id.0,
			instance_key,
		));
		loop {
			// charged before each read, including the one that ends the scan
			handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
			let Some((lane, state)) = iter.next() else { break };
			let Lane::Sub { track, chain } = lane else { continue };
			let required = template
				.as_ref()
				.and_then(|t| t.tracks.get(track as usize))
				.map_or(0, |t| t.required_count);
			let stage_name = match (template.as_ref(), state.stage) {
				(Some(t), Some(stage)) => t.slot_name(stage).map(string),
				_ => None,
			}
			.unwrap_or_else(|| UnboundedString::from(Vec::new()));
			lanes.push((
				track,
				chain,
				state.done,
				state.done_required,
				required,
				state.stage.is_some(),
				state.stage.unwrap_or(0),
				stage_name,
			));
		}
		lanes.sort_by_key(|lane| (lane.0, lane.1));
		Ok(lanes)
	}

	/// Every recorded slot of one instance with its full attempt history,
	/// ordered by (track, lane chain, slot id); main-track slots carry
	/// `track == 255` and `lane_chain_id == 0`.
	#[precompile::public("get_instance_slots(uint64,bytes16,bytes32)")]
	#[precompile::view]
	fn get_instance_slots(
		handle: &mut impl PrecompileHandle,
		product_id: ProductId,
		flow_id: Bytes16,
		instance_key: H256,
	) -> EvmResult<Vec<EvmSlot>> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost().saturating_mul(2))?;
		let Some(instance) = pallet_tranche_tx_registry_v3::Instances::<Runtime>::get((
			product_id,
			flow_id.0,
			instance_key,
		)) else {
			return Ok(Vec::new());
		};
		let template = pallet_tranche_tx_registry_v3::Templates::<Runtime>::get(instance.template);

		let mut entries: Vec<(u8, u64, SlotId, SlotRecord<BlockNumberFor<Runtime>>)> = Vec::new();
		let mut scanned_bytes = 0usize;
		let mut iter = pallet_tranche_tx_registry_v3::Slots::<Runtime>::iter_prefix((
			product_id,
			flow_id.0,
			instance_key,
		));
		loop {
			// charged before each read, including the one that ends the scan
			handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
			let Some(((lane, slot_id), record)) = iter.next() else { break };
			if entries.len() >= MAX_SLOT_SCAN {
				return Err(revert("instance has too many recorded slots to serialise"));
			}
			let size = record.encoded_size();
			Self::charge_value_size(handle, size)?;
			scanned_bytes = scanned_bytes.saturating_add(size);
			if scanned_bytes > MAX_SLOT_SCAN_BYTES {
				return Err(revert("instance's recorded slots are too large to serialise"));
			}
			let (track, chain) = match lane {
				Lane::Main => (MAIN_TRACK, 0),
				Lane::Sub { track, chain } => (track, chain),
			};
			entries.push((track, chain, slot_id, record));
		}
		// main first, then tracks in order
		entries.sort_by_key(|(track, chain, slot_id, _)| {
			(if *track == MAIN_TRACK { 0u16 } else { *track as u16 + 1 }, *chain, *slot_id)
		});

		Ok(entries
			.into_iter()
			.map(|(track, chain, slot_id, record)| {
				slot_view::<Runtime>(template.as_ref(), track, chain, slot_id, record)
			})
			.collect())
	}

	/// One slot of one instance with its full attempt history — the bounded
	/// alternative to `get_instance_slots`. `lane_chain_id == 0` for a main
	/// slot. `exists == false` if nothing was recorded there.
	#[precompile::public("get_slot(uint64,bytes16,bytes32,uint64,uint8)")]
	#[precompile::view]
	fn get_slot(
		handle: &mut impl PrecompileHandle,
		product_id: ProductId,
		flow_id: Bytes16,
		instance_key: H256,
		lane_chain_id: u64,
		slot_id: u8,
	) -> EvmResult<(bool, EvmSlot)> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost().saturating_mul(2))?;
		let empty =
			|| (MAIN_TRACK, 0, slot_id, UnboundedString::from(Vec::new()), false, 0, 0, Vec::new());
		let Some(instance) = pallet_tranche_tx_registry_v3::Instances::<Runtime>::get((
			product_id,
			flow_id.0,
			instance_key,
		)) else {
			return Ok((false, empty()));
		};
		let Some(template) =
			pallet_tranche_tx_registry_v3::Templates::<Runtime>::get(instance.template)
		else {
			return Ok((false, empty()));
		};
		let (lane, track) = if template.main_slot(slot_id).is_some() {
			(Lane::Main, MAIN_TRACK)
		} else if let Some((track, _)) = template.track_slot(slot_id) {
			if lane_chain_id == 0 {
				return Err(revert("track slot needs a lane_chain_id"));
			}
			(Lane::Sub { track, chain: lane_chain_id }, track)
		} else {
			return Ok((false, empty()));
		};
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let record = pallet_tranche_tx_registry_v3::Slots::<Runtime>::get((
			product_id,
			flow_id.0,
			instance_key,
			lane,
			slot_id,
		));
		Self::charge_value_size(handle, record.encoded_size())?;
		let exists = !record.attempts.is_empty();
		let chain = if track == MAIN_TRACK { 0 } else { lane_chain_id };
		Ok((exists, slot_view::<Runtime>(Some(&template), track, chain, slot_id, record)))
	}

	// -----------------------------------------------------------------------
	// Views — indices
	// -----------------------------------------------------------------------

	/// `investor`'s open instances of `(product_id, flow_id)` — `limit` keys
	/// after skipping `offset` (storage order, not chronological), plus the
	/// total. `limit` MUST NOT exceed `HISTORY_PAGE_SIZE`.
	#[precompile::public("get_investor_active(bytes32,uint64,bytes16,uint256,uint256)")]
	#[precompile::view]
	fn get_investor_active(
		handle: &mut impl PrecompileHandle,
		investor: H256,
		product_id: ProductId,
		flow_id: Bytes16,
		offset: U256,
		limit: U256,
	) -> EvmResult<(Vec<H256>, U256)> {
		let (offset, limit) = page_args(offset, limit)?;
		// the count, plus every entry the scan touches (skipped ones included)
		let total = {
			handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
			pallet_tranche_tx_registry_v3::InvestorActiveCount::<Runtime>::get((
				investor, product_id, flow_id.0,
			))
		};
		let touched = total.min(offset.saturating_add(limit)) as u64;
		handle.record_cost(
			RuntimeHelper::<Runtime>::db_read_gas_cost().saturating_mul(touched + 1),
		)?;
		let (keys, total) = pallet_tranche_tx_registry_v3::Pallet::<Runtime>::read_active(
			investor, product_id, flow_id.0, offset, limit,
		);
		Ok((keys, U256::from(total)))
	}

	/// The most recently opened instance of `(product_id, flow_id)` that
	/// `investor` opened with `scope` (e.g. a vault), or `bytes32(0)`. Resolves
	/// closed instances too — a whitelist action's latest nonce for a vault.
	#[precompile::public("get_latest_by_scope(bytes32,uint64,bytes16,bytes32)")]
	#[precompile::view]
	fn get_latest_by_scope(
		handle: &mut impl PrecompileHandle,
		investor: H256,
		product_id: ProductId,
		flow_id: Bytes16,
		scope: H256,
	) -> EvmResult<H256> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		Ok(pallet_tranche_tx_registry_v3::LatestByScope::<Runtime>::get((
			investor, product_id, flow_id.0, scope,
		))
		.unwrap_or_default())
	}

	/// `investor`'s closed instances of `(product_id, flow_id)`, most recent
	/// first. `limit` MUST NOT exceed `HISTORY_PAGE_SIZE`.
	#[precompile::public("get_investor_history(bytes32,uint64,bytes16,uint256,uint256)")]
	#[precompile::view]
	fn get_investor_history(
		handle: &mut impl PrecompileHandle,
		investor: H256,
		product_id: ProductId,
		flow_id: Bytes16,
		offset: U256,
		limit: U256,
	) -> EvmResult<(Vec<H256>, U256)> {
		let (offset, limit) = page_args(offset, limit)?;
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost().saturating_mul(3))?;
		let (keys, total) = pallet_tranche_tx_registry_v3::Pallet::<Runtime>::read_history(
			investor, product_id, flow_id.0, offset, limit,
		);
		Ok((keys, U256::from(total)))
	}

	/// Instances linking to `(product_id, flow_id, instance_key)` — e.g. the
	/// requests approved into a settlement — most recent first. `limit` MUST
	/// NOT exceed `HISTORY_PAGE_SIZE`.
	#[precompile::public("get_linked_from(uint64,bytes16,bytes32,uint256,uint256)")]
	#[precompile::view]
	fn get_linked_from(
		handle: &mut impl PrecompileHandle,
		product_id: ProductId,
		flow_id: Bytes16,
		instance_key: H256,
		offset: U256,
		limit: U256,
	) -> EvmResult<(Vec<EvmRef>, U256)> {
		let (offset, limit) = page_args(offset, limit)?;
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost().saturating_mul(3))?;
		let (refs, total) = pallet_tranche_tx_registry_v3::Pallet::<Runtime>::read_linked_from(
			product_id,
			flow_id.0,
			instance_key,
			offset,
			limit,
		);
		Ok((refs.into_iter().map(|(f, k)| (Bytes16(f), k)).collect(), U256::from(total)))
	}
}

impl<Runtime> TrancheTxRegistryV3Precompile<Runtime>
where
	Runtime: pallet_tranche_tx_registry_v3::Config + pallet_evm::Config + frame_system::Config,
	Runtime::RuntimeCall: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	Runtime::RuntimeCall: From<RegistryCall<Runtime>>,
	<Runtime as pallet_evm::Config>::AddressMapping: AddressMapping<Runtime::AccountId>,
{
	fn dispatch(handle: &mut impl PrecompileHandle, call: RegistryCall<Runtime>) -> EvmResult {
		let caller = Runtime::AddressMapping::into_account_id(handle.context().caller);
		RuntimeHelper::<Runtime>::try_dispatch(
			handle,
			frame_system::RawOrigin::Signed(caller).into(),
			call,
			0,
		)?;
		Ok(())
	}

	fn log_recorded(
		handle: &mut impl PrecompileHandle,
		product_id: ProductId,
		flow_id: FlowId,
		instance_key: H256,
		lane_chain_id: u64,
		slot_id: u8,
		chain_id: u64,
		tx_hash: &EvmTxHash,
		completes: bool,
		part_index: u16,
		part_count: u16,
	) -> EvmResult {
		let (part_index, part_count) =
			if part_count == 0 { (0, 0) } else { (part_index, part_count) };
		let event = log4(
			handle.context().address,
			SELECTOR_LOG_TX_RECORDED,
			H256::from(U256::from(product_id).to_big_endian()),
			bytes16_word(flow_id),
			instance_key,
			solidity::encode_event_data((
				lane_chain_id,
				slot_id,
				chain_id,
				tx_hash.clone(),
				completes,
				part_index,
				part_count,
			)),
		);
		handle.record_log_costs(&[&event])?;
		event.record(handle)?;
		Ok(())
	}

	fn log_closed(
		handle: &mut impl PrecompileHandle,
		product_id: ProductId,
		flow_id: FlowId,
		instance_key: H256,
		outcome: CloseOutcome,
	) -> EvmResult {
		let outcome: u8 = match outcome {
			CloseOutcome::Completed => 0,
			CloseOutcome::Failed => 1,
		};
		let event = log4(
			handle.context().address,
			SELECTOR_LOG_INSTANCE_CLOSED,
			H256::from(U256::from(product_id).to_big_endian()),
			bytes16_word(flow_id),
			instance_key,
			solidity::encode_event_data(outcome),
		);
		handle.record_log_costs(&[&event])?;
		event.record(handle)?;
		Ok(())
	}

	/// Whether `(product, flow, key)` is open or doesn't exist yet — read
	/// before a record, to tell whether that record closed it.
	fn is_open_or_new(
		handle: &mut impl PrecompileHandle,
		product_id: ProductId,
		flow_id: FlowId,
		instance_key: InstanceKey,
	) -> EvmResult<bool> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		Ok(pallet_tranche_tx_registry_v3::Instances::<Runtime>::get((
			product_id,
			flow_id,
			instance_key,
		))
		.map_or(true, |i| i.status == InstanceStatus::Open))
	}

	/// Emits `InstanceClosed(…, 0)` if a record just completed the instance.
	fn log_if_completed(
		handle: &mut impl PrecompileHandle,
		product_id: ProductId,
		flow_id: FlowId,
		instance_key: InstanceKey,
		was_open: bool,
	) -> EvmResult {
		if !was_open {
			return Ok(());
		}
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let completed = pallet_tranche_tx_registry_v3::Instances::<Runtime>::get((
			product_id,
			flow_id,
			instance_key,
		))
		.is_some_and(|i| i.status == InstanceStatus::Completed);
		if completed {
			Self::log_closed(handle, product_id, flow_id, instance_key, CloseOutcome::Completed)?;
		}
		Ok(())
	}

	/// Charges for a storage value of `size` bytes: its proof size, and the
	/// cost of copying it into the ABI output.
	fn charge_value_size(handle: &mut impl PrecompileHandle, size: usize) -> EvmResult {
		handle.record_external_cost(None, Some(size as u64), None)?;
		handle.record_cost(COPY_GAS_PER_WORD.saturating_mul((size as u64).div_ceil(32)))?;
		Ok(())
	}
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn lane(lane_chain_id: u64) -> Option<ChainId> {
	(lane_chain_id != 0).then_some(lane_chain_id)
}

fn part(index: u16, count: u16) -> Option<Part> {
	(count != 0).then_some((index, count))
}

fn slot_view<Runtime: pallet_tranche_tx_registry_v3::Config>(
	template: Option<&pallet_tranche_tx_registry_v3::Template>,
	track: u8,
	chain: u64,
	slot_id: SlotId,
	record: SlotRecord<BlockNumberFor<Runtime>>,
) -> EvmSlot
where
	BlockNumberFor<Runtime>: Into<U256>,
{
	let name = template
		.and_then(|t| t.slot_name(slot_id))
		.map(string)
		.unwrap_or_else(|| UnboundedString::from(Vec::new()));
	let attempts = record
		.attempts
		.into_iter()
		.map(|attempt| {
			let (part_index, part_count) = attempt.part.unwrap_or((0, 0));
			(
				attempt.tx.chain_id,
				attempt.tx.tx_hash.into_inner().into(),
				block::<Runtime>(attempt.tx.recorded_at),
				attempt.completes,
				part_index,
				part_count,
				UnboundedBytes::from(attempt.metadata.into_inner()),
			)
		})
		.collect();
	(
		track,
		chain,
		slot_id,
		name,
		record.completed,
		record.part_count.unwrap_or(0),
		record.parts_done,
		attempts,
	)
}

fn link(flow_id: Bytes16, key: H256) -> Option<(FlowId, H256)> {
	(!key.is_zero()).then_some((flow_id.0, key))
}

fn optional_bytes<S: sp_core::Get<u32>>(
	bytes: UnboundedBytes,
	too_long: &'static str,
) -> EvmResult<Option<BoundedVec<u8, S>>> {
	let bytes: Vec<u8> = bytes.into();
	if bytes.is_empty() {
		return Ok(None);
	}
	BoundedVec::try_from(bytes).map(Some).map_err(|_| revert(too_long))
}

fn page_args(offset: U256, limit: U256) -> EvmResult<(u32, u32)> {
	if limit > U256::from(HISTORY_PAGE_SIZE) {
		return Err(revert("limit exceeds HISTORY_PAGE_SIZE"));
	}
	let offset = if offset > U256::from(u32::MAX) { u32::MAX } else { offset.low_u32() };
	Ok((offset, limit.low_u32()))
}

fn string(bytes: &SlotName) -> UnboundedString {
	UnboundedString::from(bytes.to_vec())
}

fn slot_def(slot: &SlotDef) -> EvmSlotDef {
	(slot.id, string(&slot.name), slot.required)
}

fn status_code(status: InstanceStatus) -> u8 {
	match status {
		InstanceStatus::Open => 0,
		InstanceStatus::Completed => 1,
		InstanceStatus::Failed => 2,
	}
}

fn block<Runtime: frame_system::Config>(n: BlockNumberFor<Runtime>) -> u64
where
	BlockNumberFor<Runtime>: Into<U256>,
{
	let n: U256 = n.into();
	n.try_into().unwrap_or(u64::MAX)
}
