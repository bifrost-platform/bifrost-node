#![allow(unused_parens)]
#![allow(unused_imports)]

use frame_support::{
	traits::Get,
	weights::{constants::RocksDbWeight, Weight},
};
use sp_std::marker::PhantomData;

/// Weight functions needed for `pallet_relay_manager`.
pub trait WeightInfo {
	fn set_storage_cache_lifetime() -> Weight;
	fn set_heartbeat_offence_activation() -> Weight;
	fn set_heartbeat_slash_fraction() -> Weight;
	fn set_relayer() -> Weight;
	fn heartbeat() -> Weight;
}

/// Weights for `pallet_relay_manager` using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	fn set_storage_cache_lifetime() -> Weight {
		(18_178_000 as Weight) // Standard Error: 1_000
			.saturating_add(T::DbWeight::get().reads(6 as Weight))
			.saturating_add(T::DbWeight::get().writes(4 as Weight))
	}
	fn set_heartbeat_offence_activation() -> Weight {
		(18_178_000 as Weight) // Standard Error: 1_000
			.saturating_add(T::DbWeight::get().reads(6 as Weight))
			.saturating_add(T::DbWeight::get().writes(4 as Weight))
	}
	fn set_heartbeat_slash_fraction() -> Weight {
		(18_178_000 as Weight) // Standard Error: 1_000
			.saturating_add(T::DbWeight::get().reads(6 as Weight))
			.saturating_add(T::DbWeight::get().writes(4 as Weight))
	}
	fn set_relayer() -> Weight {
		(18_178_000 as Weight) // Standard Error: 1_000
			.saturating_add(T::DbWeight::get().reads(6 as Weight))
			.saturating_add(T::DbWeight::get().writes(4 as Weight))
	}
	fn heartbeat() -> Weight {
		(18_178_000 as Weight) // Standard Error: 1_000
			.saturating_add(T::DbWeight::get().reads(6 as Weight))
			.saturating_add(T::DbWeight::get().writes(4 as Weight))
	}
}

// For backwards compatibility and tests
impl WeightInfo for () {
	fn set_storage_cache_lifetime() -> Weight {
		(18_178_000 as Weight) // Standard Error: 1_000
			.saturating_add(RocksDbWeight::get().reads(6 as Weight))
			.saturating_add(RocksDbWeight::get().writes(4 as Weight))
	}
	fn set_heartbeat_offence_activation() -> Weight {
		(18_178_000 as Weight) // Standard Error: 1_000
			.saturating_add(RocksDbWeight::get().reads(6 as Weight))
			.saturating_add(RocksDbWeight::get().writes(4 as Weight))
	}
	fn set_heartbeat_slash_fraction() -> Weight {
		(18_178_000 as Weight) // Standard Error: 1_000
			.saturating_add(RocksDbWeight::get().reads(6 as Weight))
			.saturating_add(RocksDbWeight::get().writes(4 as Weight))
	}
	fn set_relayer() -> Weight {
		(18_178_000 as Weight) // Standard Error: 1_000
			.saturating_add(RocksDbWeight::get().reads(6 as Weight))
			.saturating_add(RocksDbWeight::get().writes(4 as Weight))
	}
	fn heartbeat() -> Weight {
		(18_178_000 as Weight) // Standard Error: 1_000
			.saturating_add(RocksDbWeight::get().reads(6 as Weight))
			.saturating_add(RocksDbWeight::get().writes(4 as Weight))
	}
}
