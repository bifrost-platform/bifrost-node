#![allow(unused_parens)]
#![allow(unused_imports)]

use frame_support::{
	traits::Get,
	weights::{constants::RocksDbWeight, Weight},
};
use sp_std::marker::PhantomData;

/// Weight functions needed for `pallet_btc_registration_pool`.
pub trait WeightInfo {
	fn register() -> Weight;
	fn set_issuer() -> Weight;
}

/// Weights for `pallet_btc_registration_pool` using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	fn register() -> Weight {
		Weight::from_parts(18_178_000, 0)
			.saturating_add(T::DbWeight::get().reads(6 as u64))
			.saturating_add(T::DbWeight::get().writes(4 as u64))
	}
	fn set_issuer() -> Weight {
		Weight::from_parts(18_178_000, 0)
			.saturating_add(T::DbWeight::get().reads(6 as u64))
			.saturating_add(T::DbWeight::get().writes(4 as u64))
	}
}

// For backwards compatibility and tests
impl WeightInfo for () {
	fn register() -> Weight {
		Weight::from_parts(18_178_000, 0)
			.saturating_add(RocksDbWeight::get().reads(6 as u64))
			.saturating_add(RocksDbWeight::get().writes(4 as u64))
	}
	fn set_issuer() -> Weight {
		Weight::from_parts(18_178_000, 0)
			.saturating_add(RocksDbWeight::get().reads(6 as u64))
			.saturating_add(RocksDbWeight::get().writes(4 as u64))
	}
}
