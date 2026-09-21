//! Benchmarks for `pallet-tranche-custom-flows`.
//!
//! `set_flow_descriptor` scales on total slot count `s`; `record_flow_tx` on
//! the `resolve_lane` slot scan `s`, the attempt-metadata byte length `n`, and
//! (2026-09-21, security-review H1) the slot-metadata byte length `m` — all
//! three in its `WeightInfo` signature. The recorder identity — shared with
//! `pallet-tranche-tx-registry` — is seeded via `T::BenchmarkHelper`.
//!
//! `record_flow_tx`'s setup also pre-seeds `MAX_ATTEMPTS - 1` prior attempts
//! at max metadata size before the measured call, so the benchmarked
//! `FlowSlots::try_mutate` decode/re-encode reflects a near-worst-case
//! existing `SlotRecord` (up to `MAX_ATTEMPTS` × `MAX_ATTEMPT_METADATA` +
//! `MAX_SLOT_METADATA` ≈ 330KB) rather than an empty one — previously this
//! benchmark only ever measured against a freshly-opened, empty record.

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::{
	BenchmarkHelper, FlowDescriptor, FlowId, MainTrack, SlotDef, SubTrack, TrackChain,
	MAX_ATTEMPTS, MAX_ATTEMPT_METADATA, MAX_DESCRIPTOR_SLOTS, MAX_SLOTS, MAX_SLOT_METADATA,
};
use frame_benchmarking::v2::*;
use frame_support::{traits::EnsureOrigin, BoundedVec};
use sp_core::{H160, H256};
use sp_std::{vec, vec::Vec};

const PID: ProductId = 1;
const FID: FlowId = *b"bench-flow\0\0\0\0\0\0";

fn gov_origin<T: Config>() -> T::RuntimeOrigin {
	T::GovernanceOrigin::try_successful_origin().expect("GovernanceOrigin benchmark helper")
}
fn rec_origin<T: Config>() -> T::RuntimeOrigin {
	T::BenchmarkHelper::seed_recorder();
	T::RecorderOrigin::try_successful_origin().expect("RecorderOrigin benchmark helper")
}

fn slots(ids: core::ops::Range<u16>) -> BoundedVec<SlotDef, ConstU32<MAX_SLOTS>> {
	BoundedVec::try_from(ids.map(|i| SlotDef { id: i as u8, optional: false }).collect::<Vec<_>>())
		.expect("<= MAX_SLOTS")
}

/// A descriptor whose slot ids total `s` (>=1): the main track takes up to
/// `MAX_SLOTS`, the rest spill into single-chain sub tracks. Ids stay globally
/// disjoint and strictly ascending; `s` is capped at `MAX_DESCRIPTOR_SLOTS`
/// (256), the `u8` slot-id ceiling, so `i as u8` below never wraps.
fn descriptor(s: u32) -> FlowDescriptor {
	let main_n = s.min(MAX_SLOTS);
	let main = MainTrack { chain_id: 1, slots: slots(0..main_n as u16), required_count: 0 };
	let mut sub_tracks: Vec<SubTrack> = Vec::new();
	let mut next: u16 = main_n as u16;
	let mut left = s.saturating_sub(main_n);
	while left > 0 {
		let take = left.min(MAX_SLOTS);
		sub_tracks.push(SubTrack {
			slots: slots(next..next + take as u16),
			chains: BoundedVec::try_from(vec![TrackChain {
				chain_id: 100 + sub_tracks.len() as u64,
				optional: false,
				skip_slots: BoundedVec::default(),
				required_count: 0,
			}])
			.unwrap(),
		});
		next += take as u16;
		left -= take;
	}
	FlowDescriptor {
		version: 1,
		main_track: main,
		sub_tracks: BoundedVec::try_from(sub_tracks).expect("<= MAX_SUB_TRACKS"),
		investor_scoped: true,
		non_optional_lane_count: 0,
	}
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn set_flow_descriptor(s: Linear<1, { MAX_DESCRIPTOR_SLOTS }>) {
		let d = descriptor(s);

		#[extrinsic_call]
		_(gov_origin::<T>() as T::RuntimeOrigin, PID, FID, d);

		assert!(FlowDescriptors::<T>::contains_key(PID, FID));
	}

	#[benchmark]
	fn record_flow_tx(
		s: Linear<1, { MAX_DESCRIPTOR_SLOTS }>,
		n: Linear<0, { MAX_ATTEMPT_METADATA }>,
		m: Linear<0, { MAX_SLOT_METADATA }>,
	) {
		// Register the descriptor whose slots `resolve_lane` will scan.
		Pallet::<T>::set_flow_descriptor(gov_origin::<T>(), PID, FID, descriptor(s))
			.expect("descriptor");
		let instance_key = H256::repeat_byte(0xab);

		// Pre-seed MAX_ATTEMPTS-1 prior attempts at max metadata size, so the
		// measured call below appends the *last* slot in an already
		// near-worst-case `SlotRecord` (security-review H1) — distinct
		// `tx_hash` per seed call (only `DuplicateAttestation`-checked within
		// this exact (instance, lane, slot)), `success: false` so none of them
		// trip the close-counter side effects the measured call itself checks.
		let max_attempt_meta = BoundedVec::<u8, ConstU32<MAX_ATTEMPT_METADATA>>::try_from(vec![
				0u8;
				MAX_ATTEMPT_METADATA as usize
			])
		.unwrap();
		for i in 0..(MAX_ATTEMPTS - 1) {
			Pallet::<T>::record_flow_tx(
				rec_origin::<T>(),
				PID,
				FID,
				instance_key,
				None, // track_key -> main lane
				0u8,  // slot_id 0 -> opens the instance (first call only)
				1u64,
				H256::repeat_byte(i as u8 + 1), // +1: repeat_byte(0) is the zero hash (rejected)
				false,                          // don't satisfy yet — avoid closing before the measured call
				Some(max_attempt_meta.clone()),
				Some(max_attempt_meta.clone()), // also max out slot_metadata each time
				Some(H160::repeat_byte(0x11)),
			)
			.expect("seed attempt");
		}

		let origin = rec_origin::<T>();
		let attempt_meta =
			BoundedVec::<u8, ConstU32<MAX_ATTEMPT_METADATA>>::try_from(vec![0u8; n as usize])
				.unwrap();
		let slot_meta =
			BoundedVec::<u8, ConstU32<MAX_SLOT_METADATA>>::try_from(vec![0u8; m as usize]).unwrap();

		#[extrinsic_call]
		_(
			origin,
			PID,
			FID,
			instance_key,
			None,                    // track_key -> main lane
			0u8,                     // slot_id 0
			1u64,                    // chain_id (attestation)
			H256::repeat_byte(0xcd), // tx_hash (distinct from every seed attempt)
			true,                    // success
			Some(attempt_meta),
			Some(slot_meta),
			Some(H160::repeat_byte(0x11)), // investor (investor_scoped)
		);

		assert!(FlowInstances::<T>::contains_key((PID, FID, instance_key)));
	}
}
