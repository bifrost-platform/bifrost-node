#![allow(unused_parens)]
#![allow(unused_imports)]

use frame_support::{
	traits::Get,
	weights::{constants::RocksDbWeight, Weight},
};
use sp_std::marker::PhantomData;

/// Weight functions needed for `pallet_btc_socket_queue`.
pub trait WeightInfo {
	fn set_authority() -> Weight;
	fn set_socket() -> Weight;
	fn submit_unsigned_psbt() -> Weight;
	fn submit_signed_psbt() -> Weight;
	fn submit_executed_request() -> Weight;
	fn submit_rollback_request() -> Weight;
}

/// Weights for `pallet_btc_socket_queue` using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	fn set_authority() -> Weight {
		Weight::from_parts(18_178_000, 0)
			.saturating_add(T::DbWeight::get().reads(6 as u64))
			.saturating_add(T::DbWeight::get().writes(4 as u64))
	}
	fn set_socket() -> Weight {
		Weight::from_parts(18_178_000, 0)
			.saturating_add(T::DbWeight::get().reads(6 as u64))
			.saturating_add(T::DbWeight::get().writes(4 as u64))
	}
	fn submit_unsigned_psbt() -> Weight {
		Weight::from_parts(18_178_000, 0)
			.saturating_add(T::DbWeight::get().reads(6 as u64))
			.saturating_add(T::DbWeight::get().writes(4 as u64))
	}
	fn submit_signed_psbt() -> Weight {
		Weight::from_parts(18_178_000, 0)
			.saturating_add(T::DbWeight::get().reads(6 as u64))
			.saturating_add(T::DbWeight::get().writes(4 as u64))
	}
	fn submit_executed_request() -> Weight {
		Weight::from_parts(18_178_000, 0)
			.saturating_add(T::DbWeight::get().reads(6 as u64))
			.saturating_add(T::DbWeight::get().writes(4 as u64))
	}
	fn submit_rollback_request() -> Weight {
		Weight::from_parts(18_178_000, 0)
			.saturating_add(T::DbWeight::get().reads(6 as u64))
			.saturating_add(T::DbWeight::get().writes(4 as u64))
	}
}

// For backwards compatibility and tests
impl WeightInfo for () {
	fn set_authority() -> Weight {
		Weight::from_parts(18_178_000, 0)
			.saturating_add(RocksDbWeight::get().reads(6 as u64))
			.saturating_add(RocksDbWeight::get().writes(4 as u64))
	}
	fn set_socket() -> Weight {
		Weight::from_parts(18_178_000, 0)
			.saturating_add(RocksDbWeight::get().reads(6 as u64))
			.saturating_add(RocksDbWeight::get().writes(4 as u64))
	}
	fn submit_unsigned_psbt() -> Weight {
		Weight::from_parts(18_178_000, 0)
			.saturating_add(RocksDbWeight::get().reads(6 as u64))
			.saturating_add(RocksDbWeight::get().writes(4 as u64))
	}
	fn submit_signed_psbt() -> Weight {
		Weight::from_parts(18_178_000, 0)
			.saturating_add(RocksDbWeight::get().reads(6 as u64))
			.saturating_add(RocksDbWeight::get().writes(4 as u64))
	}
	fn submit_executed_request() -> Weight {
		Weight::from_parts(18_178_000, 0)
			.saturating_add(RocksDbWeight::get().reads(6 as u64))
			.saturating_add(RocksDbWeight::get().writes(4 as u64))
	}
	fn submit_rollback_request() -> Weight {
		Weight::from_parts(18_178_000, 0)
			.saturating_add(RocksDbWeight::get().reads(6 as u64))
			.saturating_add(RocksDbWeight::get().writes(4 as u64))
	}
}
