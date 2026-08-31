#![allow(unused_parens)]
#![allow(unused_imports)]

use frame_support::{
	traits::Get,
	weights::{constants::RocksDbWeight, Weight},
};
use sp_std::marker::PhantomData;

/// Weight functions needed for `pallet_tranche_custom_flows`.
///
/// Not yet benchmarked — these are conservative hand estimates. `s` is the
/// total `SlotDef` count across `main_track.slots` + every sub track's `slots`
/// (governs the `set_flow_descriptor` derived-count pass and the
/// `record_flow_tx` slot→track lookup); `n` is `attempt_metadata` length.
pub trait WeightInfo {
	fn set_flow_descriptor(s: u32) -> Weight;
	fn record_flow_tx(s: u32, n: u32) -> Weight;
}

/// Weights for `pallet_tranche_custom_flows` using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	fn set_flow_descriptor(s: u32) -> Weight {
		Weight::from_parts(15_000_000, 0)
			.saturating_add(T::DbWeight::get().reads(1_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
			.saturating_add(Weight::from_parts(2_000, 0).saturating_mul(s as u64))
	}
	fn record_flow_tx(s: u32, n: u32) -> Weight {
		Weight::from_parts(25_000_000, 0)
			// descriptor + instance + slot record + lane counter (+ maybe investor indices on close)
			.saturating_add(T::DbWeight::get().reads(3_u64))
			.saturating_add(T::DbWeight::get().writes(3_u64))
			.saturating_add(Weight::from_parts(500, 0).saturating_mul(s as u64))
			.saturating_add(Weight::from_parts(1_000, 0).saturating_mul(n as u64))
	}
}

// For tests / runtimes without benchmarked weights.
impl WeightInfo for () {
	fn set_flow_descriptor(s: u32) -> Weight {
		Weight::from_parts(15_000_000, 0)
			.saturating_add(RocksDbWeight::get().reads(1_u64))
			.saturating_add(RocksDbWeight::get().writes(1_u64))
			.saturating_add(Weight::from_parts(2_000, 0).saturating_mul(s as u64))
	}
	fn record_flow_tx(s: u32, n: u32) -> Weight {
		Weight::from_parts(25_000_000, 0)
			.saturating_add(RocksDbWeight::get().reads(3_u64))
			.saturating_add(RocksDbWeight::get().writes(3_u64))
			.saturating_add(Weight::from_parts(500, 0).saturating_mul(s as u64))
			.saturating_add(Weight::from_parts(1_000, 0).saturating_mul(n as u64))
	}
}
