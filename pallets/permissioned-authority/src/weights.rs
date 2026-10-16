//! Weights for `pallet_permissioned_authority`.
//!
//! NOT benchmarked: conservative storage-access estimates. Both calls are root-only and rare,
//! so overestimation is harmless. Replace with `frame-omni-bencher` output once a benchmarking
//! module is added.

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]

use frame_support::{traits::Get, weights::{Weight, constants::RocksDbWeight}};
use core::marker::PhantomData;

/// Weight functions needed for `pallet_permissioned_authority`.
pub trait WeightInfo {
	fn add_authority() -> Weight;
	fn remove_authority() -> Weight;
	fn set_relayer() -> Weight;
	fn cancel_relayer_set() -> Weight;
}

/// Estimated weights. Storage access includes the `pallet-relay-manager` hooks
/// (`join_relayers` / `leave_relayers` and the delayed relayer-set requests, which touch its
/// relayer pool, state and request maps).
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	fn add_authority() -> Weight {
		Weight::from_parts(50_000_000, 10_000)
			.saturating_add(T::DbWeight::get().reads(8_u64))
			.saturating_add(T::DbWeight::get().writes(5_u64))
	}
	fn remove_authority() -> Weight {
		Weight::from_parts(50_000_000, 10_000)
			.saturating_add(T::DbWeight::get().reads(7_u64))
			.saturating_add(T::DbWeight::get().writes(5_u64))
	}
	fn set_relayer() -> Weight {
		Weight::from_parts(40_000_000, 10_000)
			.saturating_add(T::DbWeight::get().reads(6_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	fn cancel_relayer_set() -> Weight {
		Weight::from_parts(30_000_000, 10_000)
			.saturating_add(T::DbWeight::get().reads(3_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
}

impl WeightInfo for () {
	fn add_authority() -> Weight {
		Weight::from_parts(50_000_000, 10_000)
			.saturating_add(RocksDbWeight::get().reads(8_u64))
			.saturating_add(RocksDbWeight::get().writes(5_u64))
	}
	fn remove_authority() -> Weight {
		Weight::from_parts(50_000_000, 10_000)
			.saturating_add(RocksDbWeight::get().reads(7_u64))
			.saturating_add(RocksDbWeight::get().writes(5_u64))
	}
	fn set_relayer() -> Weight {
		Weight::from_parts(40_000_000, 10_000)
			.saturating_add(RocksDbWeight::get().reads(6_u64))
			.saturating_add(RocksDbWeight::get().writes(1_u64))
	}
	fn cancel_relayer_set() -> Weight {
		Weight::from_parts(30_000_000, 10_000)
			.saturating_add(RocksDbWeight::get().reads(3_u64))
			.saturating_add(RocksDbWeight::get().writes(1_u64))
	}
}
