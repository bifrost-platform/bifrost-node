#![allow(unused_parens)]
#![allow(unused_imports)]

use frame_support::{
	traits::Get,
	weights::{constants::RocksDbWeight, Weight},
};
use sp_std::marker::PhantomData;

/// Weight functions needed for `pallet_btc_socket_queue`.
pub trait WeightInfo {
	fn default() -> Weight;
	fn base_on_initialize() -> Weight;
	fn psbt_composition_on_initialize() -> Weight;
}

/// Weights for `pallet_btc_socket_queue` using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	fn default() -> Weight {
		Weight::from_parts(18_178_000, 0)
			.saturating_add(T::DbWeight::get().reads(6 as u64))
			.saturating_add(T::DbWeight::get().writes(4 as u64))
	}
	fn base_on_initialize() -> Weight {
		Weight::from_parts(5_000_000, 0).saturating_add(T::DbWeight::get().reads(1 as u64))
	}
	fn psbt_composition_on_initialize() -> Weight {
		Weight::from_parts(100_000_000, 0)
			.saturating_add(T::DbWeight::get().reads(5 as u64))
			.saturating_add(T::DbWeight::get().writes(3 as u64))
	}
}

// For backwards compatibility and tests
impl WeightInfo for () {
	fn default() -> Weight {
		Weight::from_parts(18_178_000, 0)
			.saturating_add(RocksDbWeight::get().reads(6 as u64))
			.saturating_add(RocksDbWeight::get().writes(4 as u64))
	}
	fn base_on_initialize() -> Weight {
		Weight::from_parts(5_000_000, 0).saturating_add(RocksDbWeight::get().reads(1 as u64))
	}
	fn psbt_composition_on_initialize() -> Weight {
		Weight::from_parts(100_000_000, 0)
			.saturating_add(RocksDbWeight::get().reads(5 as u64))
			.saturating_add(RocksDbWeight::get().writes(3 as u64))
	}
}
