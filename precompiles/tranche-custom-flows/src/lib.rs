#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]

//! EVM precompile for `pallet-tranche-custom-flows`.
//!
//! One write function — `record_flow_tx`, called only by the shared tx recorder
//! account (the pallet's `RecorderOrigin` rejects everyone else at dispatch, so
//! this precompile does no pre-check of its own, same as
//! `precompile-tranche-tx-registry`) — plus read-only visibility into the
//! descriptor / instance / investor-index storage, mirroring
//! `precompile-tranche-tx-registry`'s three-tier read pattern
//! (active list -> paged history -> full detail).
//!
//! ABI notes (see `docs/tranche-custom-flows/design-minimal.md` §9):
//! - `flow_id` (`[u8; 16]` slug) is a Solidity `bytes16` — [`EvmFlowId`] encodes
//!   it left-aligned in the 32-byte ABI word, ignoring the low 16 bytes.
//! - `track_chain_id == 0` means the main lane (`TrackKey::None`); any other
//!   value is `TrackKey::Some(chain)`.
//! - `investor == address(0)` on `record_flow_tx` means "not investor-scoped /
//!   not the opening call" (`Option::None`).
//! - `slot_metadata` empty bytes means "leave the stored value untouched"
//!   (`Option::None`); a non-empty value overwrites. Overwriting with a
//!   genuinely empty value isn't expressible here (a niche the pallet allows but
//!   the recorder never needs).

use frame_support::dispatch::{GetDispatchInfo, PostDispatchInfo};
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_evm::AddressMapping;
use pallet_tranche_custom_flows::{
	Attempt, Call as CustomFlowsCall, ChainId, FlowDescriptor, FlowId, Lane, ProductId, SlotId,
	SlotRecord, TrackKey,
};
use precompile_utils::{
	prelude::*,
	solidity::{
		codec::{Reader, Writer},
		Codec,
	},
};
use sp_core::{H160, H256, U256};
use sp_runtime::{traits::Dispatchable, BoundedVec};
use sp_std::{collections::btree_map::BTreeMap, marker::PhantomData, vec::Vec};

// ---------------------------------------------------------------------------
// EvmFlowId — Solidity `bytes16`
// ---------------------------------------------------------------------------

/// A [`FlowId`] (`[u8; 16]` ASCII slug) as a Solidity `bytes16`. On the wire it
/// occupies a full 32-byte ABI word, left-aligned — exactly how Solidity encodes
/// a `bytes16` value. Decoding takes the high 16 bytes and ignores the rest
/// (Solidity always zero-pads them).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EvmFlowId(pub FlowId);

impl From<FlowId> for EvmFlowId {
	fn from(id: FlowId) -> Self {
		EvmFlowId(id)
	}
}

impl Codec for EvmFlowId {
	fn read(reader: &mut Reader) -> MayRevert<Self> {
		let word = <H256 as Codec>::read(reader)?;
		let mut id = [0u8; 16];
		id.copy_from_slice(&word.0[..16]);
		Ok(EvmFlowId(id))
	}

	fn write(writer: &mut Writer, value: Self) {
		let mut word = [0u8; 32];
		word[..16].copy_from_slice(&value.0);
		<H256 as Codec>::write(writer, H256(word));
	}

	fn has_static_size() -> bool {
		true
	}

	fn signature() -> String {
		String::from("bytes16")
	}
}

/// The left-aligned 32-byte word form of a `flow_id`, for use as an indexed
/// event topic (matches how Solidity indexes a `bytes16`).
fn flow_id_topic(id: FlowId) -> H256 {
	let mut word = [0u8; 32];
	word[..16].copy_from_slice(&id);
	H256(word)
}

// ---------------------------------------------------------------------------
// interface.sol struct <-> tuple mappings
// ---------------------------------------------------------------------------

/// `SlotDefView` — (id, optional)
type EvmSlotDefView = (u8, bool);
/// `TrackChainView` — (chain_id, optional, skip_slots)
type EvmTrackChainView = (u64, bool, Vec<u8>);
/// `DescriptorTrackView` — (slots, chains)
type EvmDescriptorTrackView = (Vec<EvmSlotDefView>, Vec<EvmTrackChainView>);
/// `get_flow_descriptor` return — flat tuple (version, main_chain_id, investor_scoped,
/// main_slots, sub_tracks), NOT a single wrapping struct (matches how the precompile
/// encodes an N-value return; see interface.sol).
type EvmFlowDescriptorView = (u16, u64, bool, Vec<EvmSlotDefView>, Vec<EvmDescriptorTrackView>);
/// `TxRecord` — (chain_id, tx_hash, recorded_at)
type EvmTxRecord = (u64, H256, U256);
/// `AttemptView` — (success, metadata, tx)
type EvmAttemptView = (bool, UnboundedBytes, EvmTxRecord);
/// `SlotView` — (slot_id, satisfied, slot_metadata, attempts)
type EvmSlotView = (u8, bool, UnboundedBytes, Vec<EvmAttemptView>);
/// `SubLaneView` — (chain, slots)
type EvmSubLaneView = (u64, Vec<EvmSlotView>);
/// `SubLaneGroup` — (track_index, lanes)
type EvmSubLaneGroup = (u8, Vec<EvmSubLaneView>);
/// `get_flow_instance` return — flat tuple (investor, closed, pending_lanes, opened_at,
/// main_lane, sub_tracks), NOT a single wrapping struct (see `EvmFlowDescriptorView`).
type EvmFlowInstanceView = (Address, bool, u16, u64, Vec<EvmSlotView>, Vec<EvmSubLaneGroup>);

/// `keccak256("FlowTxRecorded(uint64,bytes16,bytes32,uint64,uint8,uint64,bytes32,bool)")` —
/// mirrors the pallet's own `FlowTxRecorded` event so EVM-side indexers can follow
/// attestations (incl. the attested `chain_id` / `tx_hash`) via `eth_getLogs` on the
/// precompile address (parity with `precompile-tranche-tx-registry`).
pub(crate) const SELECTOR_LOG_FLOW_TX_RECORDED: [u8; 32] =
	keccak256!("FlowTxRecorded(uint64,bytes16,bytes32,uint64,uint8,uint64,bytes32,bool)");

/// Upper bound on `get_investor_flow_history`'s `limit` — bounds the response
/// size regardless of how large the underlying history `Vec` has grown.
/// Rejected (not silently clamped) if exceeded — same "catch caller bugs early"
/// convention as `precompile-tranche-tx-registry`.
const MAX_HISTORY_PAGE_SIZE: usize = 50;

/// Ceiling on the size of an investor's `InvestorActiveFlows` `Vec` (across all
/// products/flows) that `get_investor_active_flows` will decode+scan. An
/// investor never legitimately has this many flows open at once (entries are
/// removed on close); hitting it means something upstream is wrong, so revert
/// loudly rather than walk a multi-hundred-element `Vec` on an `eth_call`.
const MAX_ACTIVE_FLOWS: usize = 500;

/// Ceiling on how many `FlowSlots` entries `get_flow_instance` will scan for one
/// instance. The descriptor already bounds this (main slots + Σ sub-track slots
/// × chains), but that product can be large; a real flow records far fewer.
/// Revert past it rather than let one `eth_call` walk thousands of storage keys.
const MAX_INSTANCE_SLOT_SCAN: usize = 1024;

// ---------------------------------------------------------------------------
// Precompile
// ---------------------------------------------------------------------------

/// Wraps `pallet_tranche_custom_flows`'s `record_flow_tx` extrinsic and exposes
/// read-only views over its storage. `set_flow_descriptor` is governance-only
/// and intentionally not surfaced here.
pub struct TrancheCustomFlowsPrecompile<Runtime>(PhantomData<Runtime>);

#[precompile_utils::precompile]
impl<Runtime> TrancheCustomFlowsPrecompile<Runtime>
where
	Runtime: pallet_tranche_custom_flows::Config + pallet_evm::Config + frame_system::Config,
	Runtime::RuntimeCall: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	Runtime::RuntimeCall: From<CustomFlowsCall<Runtime>>,
	BlockNumberFor<Runtime>: Into<U256>,
	<Runtime as pallet_evm::Config>::AddressMapping: AddressMapping<Runtime::AccountId>,
{
	/// Record one observed on-chain tx for a flow step. Only the pallet's
	/// registered recorder account is accepted (enforced pallet-side). There is
	/// no `open` flag — recording the first main slot for an unseen
	/// `instance_key` opens the instance; see the pallet docs.
	///
	/// @param product_id      Product the flow belongs to
	/// @param flow_id         Flow slug (`bytes16`)
	/// @param instance_key    This flow-execution's correlation id
	/// @param track_chain_id  0 = main lane; otherwise the sub-lane chain
	/// @param slot_id         Which descriptor step this attests to
	/// @param chain_id        Chain the tx actually landed on (explorer lookup)
	/// @param tx_hash         The tx hash (MUST be non-zero)
	/// @param success         Whether this observation satisfies the slot
	/// @param attempt_metadata Opaque per-attempt bytes (empty = none)
	/// @param slot_metadata    Opaque per-slot bytes, overwrites the stored value (empty = leave untouched)
	/// @param investor        Investor for the opening call iff the flow is investor-scoped; address(0) otherwise
	#[precompile::public(
		"record_flow_tx(uint64,bytes16,bytes32,uint64,uint8,uint64,bytes32,bool,bytes,bytes,address)"
	)]
	fn record_flow_tx(
		handle: &mut impl PrecompileHandle,
		product_id: ProductId,
		flow_id: EvmFlowId,
		instance_key: H256,
		track_chain_id: u64,
		slot_id: u8,
		chain_id: u64,
		tx_hash: H256,
		success: bool,
		attempt_metadata: UnboundedBytes,
		slot_metadata: UnboundedBytes,
		investor: Address,
	) -> EvmResult {
		let flow_id = flow_id.0;
		let track_key: TrackKey = if track_chain_id == 0 { None } else { Some(track_chain_id) };

		let attempt_bytes: Vec<u8> = attempt_metadata.into();
		let attempt_metadata = if attempt_bytes.is_empty() {
			None
		} else {
			Some(
				BoundedVec::try_from(attempt_bytes)
					.map_err(|_| revert("attempt_metadata exceeds MAX_ATTEMPT_METADATA"))?,
			)
		};

		let slot_bytes: Vec<u8> = slot_metadata.into();
		let slot_metadata = if slot_bytes.is_empty() {
			None
		} else {
			Some(
				BoundedVec::try_from(slot_bytes)
					.map_err(|_| revert("slot_metadata exceeds MAX_SLOT_METADATA"))?,
			)
		};

		let investor = if investor.0 == H160::zero() { None } else { Some(investor.0) };

		let caller_account = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = CustomFlowsCall::<Runtime>::record_flow_tx {
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
		};
		RuntimeHelper::<Runtime>::try_dispatch(
			handle,
			frame_system::RawOrigin::Signed(caller_account).into(),
			call,
			0,
		)?;

		let event = log4(
			handle.context().address,
			SELECTOR_LOG_FLOW_TX_RECORDED,
			topic_u256(U256::from(product_id)),
			flow_id_topic(flow_id),
			instance_key,
			solidity::encode_event_data((track_chain_id, slot_id, chain_id, tx_hash, success)),
		);
		handle.record_log_costs(&[&event])?;
		event.record(handle)?;

		Ok(())
	}

	/// The `instance_key`s `investor` has open (not yet `closed`) for one
	/// `(product_id, flow_id)`. Only investor-scoped flows are indexed here, and
	/// an entry is removed on close — so a flow that opens and closes in one
	/// `record_flow_tx` never appears (look for it in `get_investor_flow_history`).
	///
	/// The investor's whole active-flow `Vec` (across all products/flows) is one
	/// storage read, then filtered to `(product_id, flow_id)` in memory — its
	/// growth is bounded by real, weight-metered `record_flow_tx` opens, not
	/// something this call can be tricked into inflating (same rationale as
	/// `precompile-tranche-tx-registry`). `MAX_ACTIVE_FLOWS` is a loud backstop.
	#[precompile::public("get_investor_active_flows(address,uint64,bytes16)")]
	#[precompile::view]
	fn get_investor_active_flows(
		handle: &mut impl PrecompileHandle,
		investor: Address,
		product_id: ProductId,
		flow_id: EvmFlowId,
	) -> EvmResult<Vec<H256>> {
		let flow_id = flow_id.0;
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let flows = pallet_tranche_custom_flows::InvestorActiveFlows::<Runtime>::get(investor.0);
		if flows.len() > MAX_ACTIVE_FLOWS {
			return Err(revert("investor has too many active flows"));
		}
		Ok(flows
			.into_iter()
			.filter(|(pid, fid, _)| *pid == product_id && *fid == flow_id)
			.map(|(_, _, instance_key)| instance_key)
			.collect())
	}

	/// Page through the `instance_key`s an `investor` has completed for one
	/// `(product_id, flow_id)`, most-recent first. `offset`/`limit` index into
	/// that order; `offset >= total` returns an empty array. `limit` MUST NOT
	/// exceed `MAX_HISTORY_PAGE_SIZE`.
	///
	/// History for a product is one `Vec<(flow_id, instance_key)>` in storage,
	/// read and decoded whole, then filtered to `flow_id` and sliced in memory —
	/// pagination bounds the response, not the read cost (same as
	/// `precompile-tranche-tx-registry`).
	#[precompile::public("get_investor_flow_history(address,uint64,bytes16,uint256,uint256)")]
	#[precompile::view]
	fn get_investor_flow_history(
		handle: &mut impl PrecompileHandle,
		investor: Address,
		product_id: ProductId,
		flow_id: EvmFlowId,
		offset: U256,
		limit: U256,
	) -> EvmResult<(Vec<H256>, U256)> {
		if limit > U256::from(MAX_HISTORY_PAGE_SIZE) {
			return Err(revert("limit exceeds MAX_HISTORY_PAGE_SIZE"));
		}
		let limit = limit.as_usize();
		let flow_id = flow_id.0;

		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let history = pallet_tranche_custom_flows::InvestorFlowHistory::<Runtime>::get(
			investor.0, product_id,
		);

		let total = U256::from(history.iter().filter(|(fid, _)| *fid == flow_id).count());
		if offset >= total {
			return Ok((Vec::new(), total));
		}
		let offset = offset.as_usize();

		let instance_keys = history
			.iter()
			.rev()
			.filter(|(fid, _)| *fid == flow_id)
			.map(|(_, instance_key)| *instance_key)
			.skip(offset)
			.take(limit)
			.collect();
		Ok((instance_keys, total))
	}

	/// The flow's step topology. Returns a zeroed view (`version == 0`) if no
	/// descriptor is registered for `(product_id, flow_id)`.
	#[precompile::public("get_flow_descriptor(uint64,bytes16)")]
	#[precompile::view]
	fn get_flow_descriptor(
		handle: &mut impl PrecompileHandle,
		product_id: ProductId,
		flow_id: EvmFlowId,
	) -> EvmResult<EvmFlowDescriptorView> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let Some(descriptor) =
			pallet_tranche_custom_flows::FlowDescriptors::<Runtime>::get(product_id, flow_id.0)
		else {
			return Ok((0, 0, false, Vec::new(), Vec::new()));
		};

		Ok(encode_descriptor(descriptor))
	}

	/// Full timeline for one flow execution. Returns a zeroed view (`opened_at ==
	/// 0`) if the instance doesn't exist. Scans every recorded slot for the
	/// instance (capped at `MAX_INSTANCE_SLOT_SCAN`).
	#[precompile::public("get_flow_instance(uint64,bytes16,bytes32)")]
	#[precompile::view]
	fn get_flow_instance(
		handle: &mut impl PrecompileHandle,
		product_id: ProductId,
		flow_id: EvmFlowId,
		instance_key: H256,
	) -> EvmResult<EvmFlowInstanceView> {
		let flow_id = flow_id.0;
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let Some(instance) = pallet_tranche_custom_flows::FlowInstances::<Runtime>::get((
			product_id,
			flow_id,
			instance_key,
		)) else {
			return Ok((Address(H160::zero()), false, 0, 0, Vec::new(), Vec::new()));
		};

		type Record<Runtime> = SlotRecord<BlockNumberFor<Runtime>>;

		let mut main_lane: Vec<(SlotId, Record<Runtime>)> = Vec::new();
		// track_index -> chain -> [(slot_id, record)]
		let mut sub: BTreeMap<u8, BTreeMap<ChainId, Vec<(SlotId, Record<Runtime>)>>> =
			BTreeMap::new();

		let mut scanned = 0usize;
		for ((lane, slot_id), record) in
			pallet_tranche_custom_flows::FlowSlots::<Runtime>::iter_prefix((
				product_id,
				flow_id,
				instance_key,
			)) {
			handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
			scanned += 1;
			if scanned > MAX_INSTANCE_SLOT_SCAN {
				return Err(revert("instance has too many recorded slots to serialise"));
			}
			match lane {
				Lane::Main => main_lane.push((slot_id, record)),
				Lane::Sub { track, chain } => {
					sub.entry(track).or_default().entry(chain).or_default().push((slot_id, record))
				},
			}
		}

		main_lane.sort_by_key(|(slot_id, _)| *slot_id);
		let main_view: Vec<EvmSlotView> = main_lane
			.into_iter()
			.map(|(slot_id, record)| encode_slot(slot_id, record))
			.collect();

		let sub_view: Vec<EvmSubLaneGroup> = sub
			.into_iter()
			.map(|(track, chains)| {
				let lanes: Vec<EvmSubLaneView> = chains
					.into_iter()
					.map(|(chain, mut slots)| {
						slots.sort_by_key(|(slot_id, _)| *slot_id);
						let slot_views: Vec<EvmSlotView> = slots
							.into_iter()
							.map(|(slot_id, record)| encode_slot(slot_id, record))
							.collect();
						(chain, slot_views)
					})
					.collect();
				(track, lanes)
			})
			.collect();

		let opened_at: u64 = instance.opened_at.into().try_into().unwrap_or(u64::MAX);

		Ok((
			Address(instance.investor.unwrap_or_default()),
			instance.closed,
			instance.pending_lanes,
			opened_at,
			main_view,
			sub_view,
		))
	}
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// `uint256` topic encoding — left-padded big-endian, matching Solidity's ABI
/// encoding of an indexed `uint64`/`uint256` event parameter.
fn topic_u256(value: U256) -> H256 {
	H256::from(value.to_big_endian())
}

fn encode_slot<B: Into<U256>>(slot_id: SlotId, record: SlotRecord<B>) -> EvmSlotView {
	let attempts = record
		.attempts
		.into_iter()
		.map(|attempt: Attempt<B>| {
			(
				attempt.success,
				UnboundedBytes::from(attempt.metadata.into_inner()),
				(attempt.tx.chain_id, attempt.tx.tx_hash, attempt.tx.recorded_at.into()),
			)
		})
		.collect();
	(slot_id, record.satisfied, UnboundedBytes::from(record.metadata.into_inner()), attempts)
}

fn encode_descriptor(descriptor: FlowDescriptor) -> EvmFlowDescriptorView {
	let main_slots = descriptor
		.main_track
		.slots
		.iter()
		.map(|slot| (slot.id, slot.optional))
		.collect();

	let sub_tracks = descriptor
		.sub_tracks
		.iter()
		.map(|sub_track| {
			let slots: Vec<EvmSlotDefView> =
				sub_track.slots.iter().map(|slot| (slot.id, slot.optional)).collect();
			let chains: Vec<EvmTrackChainView> = sub_track
				.chains
				.iter()
				.map(|track_chain| {
					(
						track_chain.chain_id,
						track_chain.optional,
						track_chain.skip_slots.iter().copied().collect::<Vec<u8>>(),
					)
				})
				.collect();
			(slots, chains)
		})
		.collect();

	(
		descriptor.version,
		descriptor.main_track.chain_id,
		descriptor.investor_scoped,
		main_slots,
		sub_tracks,
	)
}
