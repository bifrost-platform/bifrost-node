#![allow(unused_parens)]
#![allow(unused_imports)]

use frame_support::{
	traits::Get,
	weights::{constants::RocksDbWeight, Weight},
};
use sp_std::marker::PhantomData;

/// Weight functions needed for `pallet_tranche_tx_registry`.
pub trait WeightInfo {
	fn set_tx_recorder() -> Weight;
	fn record_request_tx(extra_len: u32) -> Weight;
	fn record_settlement_tx(n: u32, extra_len: u32) -> Weight;
	fn record_receive_tx() -> Weight;
	fn record_whitelist_tx() -> Weight;
}

/// Weights for `pallet_tranche_tx_registry` using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	fn set_tx_recorder() -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(T::DbWeight::get().reads(1_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	fn record_request_tx(extra_len: u32) -> Weight {
		Weight::from_parts(20_000_000, 0)
			.saturating_add(T::DbWeight::get().reads(2_u64))
			.saturating_add(T::DbWeight::get().writes(2_u64))
			// `RequestStep::Extended` additionally reads
			// `pallet_tranche_system::RequestFlowVersion` (via `ProductInspect`)
			// and decodes `extra` — `extra_len == 0` for every other step.
			.saturating_add(T::DbWeight::get().reads(1_u64))
			.saturating_add(Weight::from_parts(1_000, 0).saturating_mul(extra_len as u64))
	}
	fn record_settlement_tx(n: u32, extra_len: u32) -> Weight {
		Weight::from_parts(20_000_000, 0)
			.saturating_add(T::DbWeight::get().reads(3_u64))
			.saturating_add(T::DbWeight::get().writes(3_u64))
			// `SettlementStep::RequestsApproved` reads+writes one `RequestEntries` entry per
			// `request_ids` element, on top of the fixed cost above — `n == 0` for
			// every other step.
			.saturating_add(T::DbWeight::get().reads_writes(n as u64, n as u64))
			// `SettlementStep::Extended` additionally reads
			// `pallet_tranche_system::SettlementFlowVersion` (via `ProductInspect`)
			// and decodes `extra` — `extra_len == 0` for every other step.
			.saturating_add(T::DbWeight::get().reads(1_u64))
			.saturating_add(Weight::from_parts(1_000, 0).saturating_mul(extra_len as u64))
	}
	fn record_receive_tx() -> Weight {
		Weight::from_parts(20_000_000, 0)
			.saturating_add(T::DbWeight::get().reads(1_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	fn record_whitelist_tx() -> Weight {
		Weight::from_parts(20_000_000, 0)
			.saturating_add(T::DbWeight::get().reads(3_u64))
			.saturating_add(T::DbWeight::get().writes(2_u64))
	}
}

// For backwards compatibility and tests
impl WeightInfo for () {
	fn set_tx_recorder() -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(RocksDbWeight::get().reads(1_u64))
			.saturating_add(RocksDbWeight::get().writes(1_u64))
	}
	fn record_request_tx(extra_len: u32) -> Weight {
		Weight::from_parts(20_000_000, 0)
			.saturating_add(RocksDbWeight::get().reads(2_u64))
			.saturating_add(RocksDbWeight::get().writes(2_u64))
			.saturating_add(RocksDbWeight::get().reads(1_u64))
			.saturating_add(Weight::from_parts(1_000, 0).saturating_mul(extra_len as u64))
	}
	fn record_settlement_tx(n: u32, extra_len: u32) -> Weight {
		Weight::from_parts(20_000_000, 0)
			.saturating_add(RocksDbWeight::get().reads(3_u64))
			.saturating_add(RocksDbWeight::get().writes(3_u64))
			.saturating_add(RocksDbWeight::get().reads_writes(n as u64, n as u64))
			.saturating_add(RocksDbWeight::get().reads(1_u64))
			.saturating_add(Weight::from_parts(1_000, 0).saturating_mul(extra_len as u64))
	}
	fn record_receive_tx() -> Weight {
		Weight::from_parts(20_000_000, 0)
			.saturating_add(RocksDbWeight::get().reads(1_u64))
			.saturating_add(RocksDbWeight::get().writes(1_u64))
	}
	fn record_whitelist_tx() -> Weight {
		Weight::from_parts(20_000_000, 0)
			.saturating_add(RocksDbWeight::get().reads(3_u64))
			.saturating_add(RocksDbWeight::get().writes(2_u64))
	}
}
