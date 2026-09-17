//! Benchmarks for `pallet-tranche-custom-flows`.
//!
//! `set_flow_descriptor` scales on total slot count `s`; `record_flow_tx` on
//! the `resolve_lane` slot scan `s` and the attempt-metadata byte length `n`
//! (both already in its `WeightInfo` signature). The recorder identity — shared
//! with `pallet-tranche-tx-registry` — is seeded via `T::BenchmarkHelper`.

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::{
	BenchmarkHelper, FlowDescriptor, FlowId, MainTrack, SlotDef, SubTrack, TrackChain,
	MAX_ATTEMPT_METADATA, MAX_DESCRIPTOR_SLOTS, MAX_SLOTS,
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
	) {
		// Register the descriptor whose slots `resolve_lane` will scan.
		Pallet::<T>::set_flow_descriptor(gov_origin::<T>(), PID, FID, descriptor(s))
			.expect("descriptor");
		let origin = rec_origin::<T>();
		let meta =
			BoundedVec::<u8, ConstU32<MAX_ATTEMPT_METADATA>>::try_from(vec![0u8; n as usize])
				.unwrap();

		#[extrinsic_call]
		_(
			origin,
			PID,
			FID,
			H256::repeat_byte(0xab), // instance_key
			None,                    // track_key -> main lane
			0u8,                     // slot_id 0 -> opens the instance
			1u64,                    // chain_id (attestation)
			H256::repeat_byte(0xcd), // tx_hash
			true,                    // success
			Some(meta),
			None,                          // slot_metadata
			Some(H160::repeat_byte(0x11)), // investor (investor_scoped)
		);

		assert!(FlowInstances::<T>::contains_key((PID, FID, H256::repeat_byte(0xab))));
	}
}
