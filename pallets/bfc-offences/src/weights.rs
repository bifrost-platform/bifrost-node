#![allow(unused_parens)]
#![allow(unused_imports)]

use frame_support::{
	traits::Get,
	weights::{constants::RocksDbWeight, Weight},
};
use sp_std::marker::PhantomData;

/// Weight functions needed for `pallet_bfc_offences`.
pub trait WeightInfo {
	fn set_offence_expiration() -> Weight;
	fn set_max_offence_count() -> Weight;
	fn set_offence_activation() -> Weight;
	fn set_slash_activation() -> Weight;
}

/// Weights for `pallet_bfc_offences` using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	fn set_offence_expiration() -> Weight {
		T::DbWeight::get().reads(1).saturating_add(T::DbWeight::get().writes(1))
	}
	fn set_max_offence_count() -> Weight {
		T::DbWeight::get().reads(1).saturating_add(T::DbWeight::get().writes(1))
	}
	fn set_offence_activation() -> Weight {
		T::DbWeight::get().reads(1).saturating_add(T::DbWeight::get().writes(1))
	}
	fn set_slash_activation() -> Weight {
		T::DbWeight::get().reads(1).saturating_add(T::DbWeight::get().writes(1))
	}
}

// For backwards compatibility and tests
impl WeightInfo for () {
	fn set_offence_expiration() -> Weight {
		RocksDbWeight::get().reads(1).saturating_add(RocksDbWeight::get().writes(1))
	}
	fn set_max_offence_count() -> Weight {
		RocksDbWeight::get().reads(1).saturating_add(RocksDbWeight::get().writes(1))
	}
	fn set_offence_activation() -> Weight {
		RocksDbWeight::get().reads(1).saturating_add(RocksDbWeight::get().writes(1))
	}
	fn set_slash_activation() -> Weight {
		RocksDbWeight::get().reads(1).saturating_add(RocksDbWeight::get().writes(1))
	}
}
