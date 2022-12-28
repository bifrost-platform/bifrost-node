#![allow(unused_parens)]
#![allow(unused_imports)]

use frame_support::{
	traits::Get,
	weights::{constants::RocksDbWeight, Weight},
};
use sp_std::marker::PhantomData;

/// Weight functions needed for pallet_bfc_staking.
pub trait WeightInfo {
	fn hotfix_remove_nomination_requests(x: u32) -> Weight;
	fn hotfix_update_candidate_pool_value(x: u32) -> Weight;
	fn set_staking_expectations() -> Weight;
	fn set_inflation() -> Weight;
	fn set_max_total_selected() -> Weight;
	fn set_min_total_selected() -> Weight;
	fn set_default_validator_commission() -> Weight;
	fn set_max_validator_commission() -> Weight;
	fn set_validator_commission() -> Weight;
	fn cancel_validator_commission_set() -> Weight;
	fn set_validator_tier() -> Weight;
	fn set_blocks_per_round() -> Weight;
	fn set_storage_cache_lifetime() -> Weight;
	fn set_controller() -> Weight;
	fn cancel_controller_set() -> Weight;
	fn set_candidate_reward_dst() -> Weight;
	fn set_nominator_reward_dst() -> Weight;
	fn join_candidates(x: u32) -> Weight;
	fn schedule_leave_candidates(x: u32) -> Weight;
	fn execute_leave_candidates(x: u32) -> Weight;
	fn cancel_leave_candidates(x: u32) -> Weight;
	fn go_offline() -> Weight;
	fn go_online() -> Weight;
	fn candidate_bond_more() -> Weight;
	fn schedule_candidate_bond_less() -> Weight;
	fn execute_candidate_bond_less() -> Weight;
	fn cancel_candidate_bond_less() -> Weight;
	fn nominate(x: u32, y: u32) -> Weight;
	fn schedule_leave_nominators() -> Weight;
	fn execute_leave_nominators(x: u32) -> Weight;
	fn cancel_leave_nominators() -> Weight;
	fn schedule_revoke_nomination() -> Weight;
	fn nominator_bond_more() -> Weight;
	fn schedule_nominator_bond_less() -> Weight;
	fn execute_revoke_nomination() -> Weight;
	fn execute_nominator_bond_less() -> Weight;
	fn cancel_revoke_nomination() -> Weight;
	fn cancel_nominator_bond_less() -> Weight;
	fn round_transition_on_initialize(x: u32, y: u32) -> Weight;
	fn base_on_initialize() -> Weight;
	fn pay_one_validator_reward(y: u32) -> Weight;
}

/// Weights for pallet_bfc_staking using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	fn hotfix_remove_nomination_requests(x: u32) -> Weight {
		Weight::from_ref_time(0)
			.saturating_add(Weight::from_ref_time(8_132_000).saturating_mul(x as u64))
			.saturating_add(T::DbWeight::get().reads((1 as u64).saturating_mul(x as u64)))
			.saturating_add(T::DbWeight::get().writes((1 as u64).saturating_mul(x as u64)))
	}
	fn hotfix_update_candidate_pool_value(x: u32) -> Weight {
		Weight::from_ref_time(0)
			.saturating_add(Weight::from_ref_time(26_825_000).saturating_mul(x as u64))
			.saturating_add(T::DbWeight::get().reads((1 as u64).saturating_mul(x as u64)))
			.saturating_add(T::DbWeight::get().writes(1 as u64))
	}
	fn set_staking_expectations() -> Weight {
		Weight::from_ref_time(20_719_000)
			.saturating_add(T::DbWeight::get().reads(5 as u64))
			.saturating_add(T::DbWeight::get().writes(3 as u64))
	}
	fn set_inflation() -> Weight {
		Weight::from_ref_time(63_011_000)
			.saturating_add(T::DbWeight::get().reads(6 as u64))
			.saturating_add(T::DbWeight::get().writes(3 as u64))
	}
	fn set_max_total_selected() -> Weight {
		Weight::from_ref_time(18_402_000)
			.saturating_add(T::DbWeight::get().reads(5 as u64))
			.saturating_add(T::DbWeight::get().writes(3 as u64))
	}
	fn set_min_total_selected() -> Weight {
		Weight::from_ref_time(18_402_000)
			.saturating_add(T::DbWeight::get().reads(5 as u64))
			.saturating_add(T::DbWeight::get().writes(3 as u64))
	}
	fn set_default_validator_commission() -> Weight {
		Weight::from_ref_time(18_178_000)
			.saturating_add(T::DbWeight::get().reads(5 as u64))
			.saturating_add(T::DbWeight::get().writes(3 as u64))
	}
	fn set_max_validator_commission() -> Weight {
		Weight::from_ref_time(18_178_000)
			.saturating_add(T::DbWeight::get().reads(5 as u64))
			.saturating_add(T::DbWeight::get().writes(3 as u64))
	}
	fn set_validator_commission() -> Weight {
		Weight::from_ref_time(18_178_000)
			.saturating_add(T::DbWeight::get().reads(5 as u64))
			.saturating_add(T::DbWeight::get().writes(3 as u64))
	}
	fn cancel_validator_commission_set() -> Weight {
		Weight::from_ref_time(18_178_000)
			.saturating_add(T::DbWeight::get().reads(5 as u64))
			.saturating_add(T::DbWeight::get().writes(3 as u64))
	}
	fn set_validator_tier() -> Weight {
		Weight::from_ref_time(18_178_000)
			.saturating_add(T::DbWeight::get().reads(5 as u64))
			.saturating_add(T::DbWeight::get().writes(3 as u64))
	}
	fn set_blocks_per_round() -> Weight {
		Weight::from_ref_time(65_939_000)
			.saturating_add(T::DbWeight::get().reads(6 as u64))
			.saturating_add(T::DbWeight::get().writes(4 as u64))
	}
	fn set_storage_cache_lifetime() -> Weight {
		Weight::from_ref_time(18_178_000)
			.saturating_add(T::DbWeight::get().reads(6 as u64))
			.saturating_add(T::DbWeight::get().writes(4 as u64))
	}
	fn set_controller() -> Weight {
		Weight::from_ref_time(18_178_000)
			.saturating_add(T::DbWeight::get().reads(6 as u64))
			.saturating_add(T::DbWeight::get().writes(4 as u64))
	}
	fn cancel_controller_set() -> Weight {
		Weight::from_ref_time(18_178_000)
			.saturating_add(T::DbWeight::get().reads(6 as u64))
			.saturating_add(T::DbWeight::get().writes(4 as u64))
	}
	fn set_candidate_reward_dst() -> Weight {
		Weight::from_ref_time(18_178_000)
			.saturating_add(T::DbWeight::get().reads(6 as u64))
			.saturating_add(T::DbWeight::get().writes(4 as u64))
	}
	fn set_nominator_reward_dst() -> Weight {
		Weight::from_ref_time(18_178_000)
			.saturating_add(T::DbWeight::get().reads(6 as u64))
			.saturating_add(T::DbWeight::get().writes(4 as u64))
	}
	fn join_candidates(x: u32) -> Weight {
		Weight::from_ref_time(80_619_000)
			.saturating_add(Weight::from_ref_time(107_000).saturating_mul(x as u64))
			.saturating_add(T::DbWeight::get().reads(9 as u64))
			.saturating_add(T::DbWeight::get().writes(8 as u64))
	}
	fn schedule_leave_candidates(x: u32) -> Weight {
		Weight::from_ref_time(50_933_000)
			.saturating_add(Weight::from_ref_time(108_000).saturating_mul(x as u64))
			.saturating_add(T::DbWeight::get().reads(7 as u64))
			.saturating_add(T::DbWeight::get().writes(4 as u64))
	}
	fn execute_leave_candidates(x: u32) -> Weight {
		Weight::from_ref_time(8_634_000)
			.saturating_add(Weight::from_ref_time(26_979_000).saturating_mul(x as u64))
			.saturating_add(T::DbWeight::get().reads(8 as u64))
			.saturating_add(T::DbWeight::get().reads((2 as u64).saturating_mul(x as u64)))
			.saturating_add(T::DbWeight::get().writes(5 as u64))
			.saturating_add(T::DbWeight::get().writes((2 as u64).saturating_mul(x as u64)))
	}
	fn cancel_leave_candidates(x: u32) -> Weight {
		Weight::from_ref_time(43_482_000)
			.saturating_add(Weight::from_ref_time(111_000).saturating_mul(x as u64))
			.saturating_add(T::DbWeight::get().reads(6 as u64))
			.saturating_add(T::DbWeight::get().writes(4 as u64))
	}
	fn go_offline() -> Weight {
		Weight::from_ref_time(30_778_000)
			.saturating_add(T::DbWeight::get().reads(6 as u64))
			.saturating_add(T::DbWeight::get().writes(4 as u64))
	}
	fn go_online() -> Weight {
		Weight::from_ref_time(31_178_000)
			.saturating_add(T::DbWeight::get().reads(6 as u64))
			.saturating_add(T::DbWeight::get().writes(4 as u64))
	}
	fn candidate_bond_more() -> Weight {
		Weight::from_ref_time(53_492_000)
			.saturating_add(T::DbWeight::get().reads(8 as u64))
			.saturating_add(T::DbWeight::get().writes(6 as u64))
	}
	fn schedule_candidate_bond_less() -> Weight {
		Weight::from_ref_time(29_393_000)
			.saturating_add(T::DbWeight::get().reads(6 as u64))
			.saturating_add(T::DbWeight::get().writes(3 as u64))
	}
	fn execute_candidate_bond_less() -> Weight {
		Weight::from_ref_time(62_395_000)
			.saturating_add(T::DbWeight::get().reads(9 as u64))
			.saturating_add(T::DbWeight::get().writes(6 as u64))
	}
	fn cancel_candidate_bond_less() -> Weight {
		Weight::from_ref_time(25_564_000)
			.saturating_add(T::DbWeight::get().reads(5 as u64))
			.saturating_add(T::DbWeight::get().writes(3 as u64))
	}
	fn nominate(x: u32, y: u32) -> Weight {
		Weight::from_ref_time(103_760_000)
			.saturating_add(Weight::from_ref_time(198_000).saturating_mul(x as u64))
			.saturating_add(Weight::from_ref_time(112_000).saturating_mul(y as u64))
			.saturating_add(T::DbWeight::get().reads(10 as u64))
			.saturating_add(T::DbWeight::get().writes(8 as u64))
	}
	fn schedule_leave_nominators() -> Weight {
		Weight::from_ref_time(30_908_000)
			.saturating_add(T::DbWeight::get().reads(6 as u64))
			.saturating_add(T::DbWeight::get().writes(3 as u64))
	}
	fn execute_leave_nominators(x: u32) -> Weight {
		Weight::from_ref_time(1_091_000)
			.saturating_add(Weight::from_ref_time(37_192_000).saturating_mul(x as u64))
			.saturating_add(T::DbWeight::get().reads(6 as u64))
			.saturating_add(T::DbWeight::get().reads((2 as u64).saturating_mul(x as u64)))
			.saturating_add(T::DbWeight::get().writes(3 as u64))
			.saturating_add(T::DbWeight::get().writes((2 as u64).saturating_mul(x as u64)))
	}
	fn cancel_leave_nominators() -> Weight {
		Weight::from_ref_time(26_796_000)
			.saturating_add(T::DbWeight::get().reads(5 as u64))
			.saturating_add(T::DbWeight::get().writes(3 as u64))
	}
	fn schedule_revoke_nomination() -> Weight {
		Weight::from_ref_time(37_580_000)
			.saturating_add(T::DbWeight::get().reads(7 as u64))
			.saturating_add(T::DbWeight::get().writes(4 as u64))
	}
	fn nominator_bond_more() -> Weight {
		Weight::from_ref_time(65_757_000)
			.saturating_add(T::DbWeight::get().reads(9 as u64))
			.saturating_add(T::DbWeight::get().writes(7 as u64))
	}
	fn schedule_nominator_bond_less() -> Weight {
		Weight::from_ref_time(70_859_000)
			.saturating_add(T::DbWeight::get().reads(9 as u64))
			.saturating_add(T::DbWeight::get().writes(7 as u64))
	}
	fn execute_revoke_nomination() -> Weight {
		Weight::from_ref_time(87_836_000)
			.saturating_add(T::DbWeight::get().reads(10 as u64))
			.saturating_add(T::DbWeight::get().writes(7 as u64))
	}
	fn execute_nominator_bond_less() -> Weight {
		Weight::from_ref_time(80_983_000)
			.saturating_add(T::DbWeight::get().reads(11 as u64))
			.saturating_add(T::DbWeight::get().writes(8 as u64))
	}
	fn cancel_revoke_nomination() -> Weight {
		Weight::from_ref_time(37_923_000)
			.saturating_add(T::DbWeight::get().reads(7 as u64))
			.saturating_add(T::DbWeight::get().writes(4 as u64))
	}
	fn cancel_nominator_bond_less() -> Weight {
		Weight::from_ref_time(70_813_000)
			.saturating_add(T::DbWeight::get().reads(9 as u64))
			.saturating_add(T::DbWeight::get().writes(7 as u64))
	}
	fn round_transition_on_initialize(x: u32, y: u32) -> Weight {
		Weight::from_ref_time(0)
			.saturating_add(Weight::from_ref_time(100_164_000).saturating_mul(x as u64))
			.saturating_add(Weight::from_ref_time(1_202_000).saturating_mul(y as u64))
			.saturating_add(T::DbWeight::get().reads((4 as u64).saturating_mul(x as u64)))
			.saturating_add(T::DbWeight::get().writes((3 as u64).saturating_mul(x as u64)))
	}
	fn base_on_initialize() -> Weight {
		Weight::from_ref_time(4_913_000).saturating_add(T::DbWeight::get().reads(1 as u64))
	}
	fn pay_one_validator_reward(y: u32) -> Weight {
		Weight::from_ref_time(0)
			.saturating_add(Weight::from_ref_time(23_284_000).saturating_mul(y as u64))
			.saturating_add(T::DbWeight::get().reads(11 as u64))
			.saturating_add(T::DbWeight::get().reads((1 as u64).saturating_mul(y as u64)))
			.saturating_add(T::DbWeight::get().writes(6 as u64))
			.saturating_add(T::DbWeight::get().writes((1 as u64).saturating_mul(y as u64)))
	}
}

// For backwards compatibility and tests
impl WeightInfo for () {
	fn hotfix_remove_nomination_requests(x: u32) -> Weight {
		Weight::from_ref_time(0)
			.saturating_add(Weight::from_ref_time(8_132_000).saturating_mul(x as u64))
			.saturating_add(RocksDbWeight::get().reads((1 as u64).saturating_mul(x as u64)))
			.saturating_add(RocksDbWeight::get().writes((1 as u64).saturating_mul(x as u64)))
	}
	fn hotfix_update_candidate_pool_value(x: u32) -> Weight {
		Weight::from_ref_time(0)
			.saturating_add(Weight::from_ref_time(26_825_000).saturating_mul(x as u64))
			.saturating_add(RocksDbWeight::get().reads((1 as u64).saturating_mul(x as u64)))
			.saturating_add(RocksDbWeight::get().writes(1 as u64))
	}
	fn set_staking_expectations() -> Weight {
		Weight::from_ref_time(20_719_000)
			.saturating_add(RocksDbWeight::get().reads(5 as u64))
			.saturating_add(RocksDbWeight::get().writes(3 as u64))
	}
	fn set_inflation() -> Weight {
		Weight::from_ref_time(63_011_000)
			.saturating_add(RocksDbWeight::get().reads(6 as u64))
			.saturating_add(RocksDbWeight::get().writes(3 as u64))
	}
	fn set_max_total_selected() -> Weight {
		Weight::from_ref_time(18_402_000)
			.saturating_add(RocksDbWeight::get().reads(5 as u64))
			.saturating_add(RocksDbWeight::get().writes(3 as u64))
	}
	fn set_min_total_selected() -> Weight {
		Weight::from_ref_time(18_402_000)
			.saturating_add(RocksDbWeight::get().reads(5 as u64))
			.saturating_add(RocksDbWeight::get().writes(3 as u64))
	}
	fn set_default_validator_commission() -> Weight {
		Weight::from_ref_time(18_178_000)
			.saturating_add(RocksDbWeight::get().reads(5 as u64))
			.saturating_add(RocksDbWeight::get().writes(3 as u64))
	}
	fn set_max_validator_commission() -> Weight {
		Weight::from_ref_time(18_178_000)
			.saturating_add(RocksDbWeight::get().reads(5 as u64))
			.saturating_add(RocksDbWeight::get().writes(3 as u64))
	}
	fn set_validator_commission() -> Weight {
		Weight::from_ref_time(18_178_000)
			.saturating_add(RocksDbWeight::get().reads(5 as u64))
			.saturating_add(RocksDbWeight::get().writes(3 as u64))
	}
	fn cancel_validator_commission_set() -> Weight {
		Weight::from_ref_time(18_178_000)
			.saturating_add(RocksDbWeight::get().reads(5 as u64))
			.saturating_add(RocksDbWeight::get().writes(3 as u64))
	}
	fn set_validator_tier() -> Weight {
		Weight::from_ref_time(18_178_000)
			.saturating_add(RocksDbWeight::get().reads(5 as u64))
			.saturating_add(RocksDbWeight::get().writes(3 as u64))
	}
	fn set_blocks_per_round() -> Weight {
		Weight::from_ref_time(65_939_000)
			.saturating_add(RocksDbWeight::get().reads(6 as u64))
			.saturating_add(RocksDbWeight::get().writes(4 as u64))
	}
	fn set_storage_cache_lifetime() -> Weight {
		Weight::from_ref_time(18_178_000)
			.saturating_add(RocksDbWeight::get().reads(6 as u64))
			.saturating_add(RocksDbWeight::get().writes(4 as u64))
	}
	fn set_controller() -> Weight {
		Weight::from_ref_time(18_178_000)
			.saturating_add(RocksDbWeight::get().reads(6 as u64))
			.saturating_add(RocksDbWeight::get().writes(4 as u64))
	}
	fn cancel_controller_set() -> Weight {
		Weight::from_ref_time(18_178_000)
			.saturating_add(RocksDbWeight::get().reads(6 as u64))
			.saturating_add(RocksDbWeight::get().writes(4 as u64))
	}
	fn set_candidate_reward_dst() -> Weight {
		Weight::from_ref_time(18_178_000)
			.saturating_add(RocksDbWeight::get().reads(6 as u64))
			.saturating_add(RocksDbWeight::get().writes(4 as u64))
	}
	fn set_nominator_reward_dst() -> Weight {
		Weight::from_ref_time(18_178_000)
			.saturating_add(RocksDbWeight::get().reads(6 as u64))
			.saturating_add(RocksDbWeight::get().writes(4 as u64))
	}
	fn join_candidates(x: u32) -> Weight {
		Weight::from_ref_time(80_619_000)
			.saturating_add(Weight::from_ref_time(107_000).saturating_mul(x as u64))
			.saturating_add(RocksDbWeight::get().reads(9 as u64))
			.saturating_add(RocksDbWeight::get().writes(8 as u64))
	}
	fn schedule_leave_candidates(x: u32) -> Weight {
		Weight::from_ref_time(50_933_000)
			.saturating_add(Weight::from_ref_time(108_000).saturating_mul(x as u64))
			.saturating_add(RocksDbWeight::get().reads(7 as u64))
			.saturating_add(RocksDbWeight::get().writes(4 as u64))
	}
	fn execute_leave_candidates(x: u32) -> Weight {
		Weight::from_ref_time(8_634_000)
			.saturating_add(Weight::from_ref_time(26_979_000).saturating_mul(x as u64))
			.saturating_add(RocksDbWeight::get().reads(8 as u64))
			.saturating_add(RocksDbWeight::get().reads((2 as u64).saturating_mul(x as u64)))
			.saturating_add(RocksDbWeight::get().writes(5 as u64))
			.saturating_add(RocksDbWeight::get().writes((2 as u64).saturating_mul(x as u64)))
	}
	fn cancel_leave_candidates(x: u32) -> Weight {
		Weight::from_ref_time(43_482_000)
			.saturating_add(Weight::from_ref_time(111_000).saturating_mul(x as u64))
			.saturating_add(RocksDbWeight::get().reads(6 as u64))
			.saturating_add(RocksDbWeight::get().writes(4 as u64))
	}
	fn go_offline() -> Weight {
		Weight::from_ref_time(30_778_000)
			.saturating_add(RocksDbWeight::get().reads(6 as u64))
			.saturating_add(RocksDbWeight::get().writes(4 as u64))
	}
	fn go_online() -> Weight {
		Weight::from_ref_time(31_178_000)
			.saturating_add(RocksDbWeight::get().reads(6 as u64))
			.saturating_add(RocksDbWeight::get().writes(4 as u64))
	}
	fn candidate_bond_more() -> Weight {
		Weight::from_ref_time(53_492_000)
			.saturating_add(RocksDbWeight::get().reads(8 as u64))
			.saturating_add(RocksDbWeight::get().writes(6 as u64))
	}
	fn schedule_candidate_bond_less() -> Weight {
		Weight::from_ref_time(29_393_000)
			.saturating_add(RocksDbWeight::get().reads(6 as u64))
			.saturating_add(RocksDbWeight::get().writes(3 as u64))
	}
	fn execute_candidate_bond_less() -> Weight {
		Weight::from_ref_time(62_395_000)
			.saturating_add(RocksDbWeight::get().reads(9 as u64))
			.saturating_add(RocksDbWeight::get().writes(6 as u64))
	}
	fn cancel_candidate_bond_less() -> Weight {
		Weight::from_ref_time(25_564_000)
			.saturating_add(RocksDbWeight::get().reads(5 as u64))
			.saturating_add(RocksDbWeight::get().writes(3 as u64))
	}
	fn nominate(x: u32, y: u32) -> Weight {
		Weight::from_ref_time(103_760_000)
			.saturating_add(Weight::from_ref_time(198_000).saturating_mul(x as u64))
			.saturating_add(Weight::from_ref_time(112_000).saturating_mul(y as u64))
			.saturating_add(RocksDbWeight::get().reads(10 as u64))
			.saturating_add(RocksDbWeight::get().writes(8 as u64))
	}
	fn schedule_leave_nominators() -> Weight {
		Weight::from_ref_time(30_908_000)
			.saturating_add(RocksDbWeight::get().reads(6 as u64))
			.saturating_add(RocksDbWeight::get().writes(3 as u64))
	}
	fn execute_leave_nominators(x: u32) -> Weight {
		Weight::from_ref_time(1_091_000)
			.saturating_add(Weight::from_ref_time(37_192_000).saturating_mul(x as u64))
			.saturating_add(RocksDbWeight::get().reads(6 as u64))
			.saturating_add(RocksDbWeight::get().reads((2 as u64).saturating_mul(x as u64)))
			.saturating_add(RocksDbWeight::get().writes(3 as u64))
			.saturating_add(RocksDbWeight::get().writes((2 as u64).saturating_mul(x as u64)))
	}
	fn cancel_leave_nominators() -> Weight {
		Weight::from_ref_time(26_796_000)
			.saturating_add(RocksDbWeight::get().reads(5 as u64))
			.saturating_add(RocksDbWeight::get().writes(3 as u64))
	}
	fn schedule_revoke_nomination() -> Weight {
		Weight::from_ref_time(37_580_000)
			.saturating_add(RocksDbWeight::get().reads(7 as u64))
			.saturating_add(RocksDbWeight::get().writes(4 as u64))
	}
	fn nominator_bond_more() -> Weight {
		Weight::from_ref_time(65_757_000)
			.saturating_add(RocksDbWeight::get().reads(9 as u64))
			.saturating_add(RocksDbWeight::get().writes(7 as u64))
	}
	fn schedule_nominator_bond_less() -> Weight {
		Weight::from_ref_time(70_859_000)
			.saturating_add(RocksDbWeight::get().reads(9 as u64))
			.saturating_add(RocksDbWeight::get().writes(7 as u64))
	}
	fn execute_revoke_nomination() -> Weight {
		Weight::from_ref_time(87_836_000)
			.saturating_add(RocksDbWeight::get().reads(10 as u64))
			.saturating_add(RocksDbWeight::get().writes(7 as u64))
	}
	fn execute_nominator_bond_less() -> Weight {
		Weight::from_ref_time(80_983_000)
			.saturating_add(RocksDbWeight::get().reads(11 as u64))
			.saturating_add(RocksDbWeight::get().writes(8 as u64))
	}
	fn cancel_revoke_nomination() -> Weight {
		Weight::from_ref_time(37_923_000)
			.saturating_add(RocksDbWeight::get().reads(7 as u64))
			.saturating_add(RocksDbWeight::get().writes(4 as u64))
	}
	fn cancel_nominator_bond_less() -> Weight {
		Weight::from_ref_time(70_813_000)
			.saturating_add(RocksDbWeight::get().reads(7 as u64))
			.saturating_add(RocksDbWeight::get().writes(4 as u64))
	}
	fn round_transition_on_initialize(x: u32, y: u32) -> Weight {
		Weight::from_ref_time(0)
			.saturating_add(Weight::from_ref_time(100_164_000).saturating_mul(x as u64))
			.saturating_add(Weight::from_ref_time(1_202_000).saturating_mul(y as u64))
			.saturating_add(RocksDbWeight::get().reads((4 as u64).saturating_mul(x as u64)))
			.saturating_add(RocksDbWeight::get().writes((3 as u64).saturating_mul(x as u64)))
	}
	fn base_on_initialize() -> Weight {
		Weight::from_ref_time(4_913_000).saturating_add(RocksDbWeight::get().reads(1 as u64))
	}
	fn pay_one_validator_reward(y: u32) -> Weight {
		Weight::from_ref_time(0)
			.saturating_add(Weight::from_ref_time(23_284_000).saturating_mul(y as u64))
			.saturating_add(RocksDbWeight::get().reads(11 as u64))
			.saturating_add(RocksDbWeight::get().reads((1 as u64).saturating_mul(y as u64)))
			.saturating_add(RocksDbWeight::get().writes(6 as u64))
			.saturating_add(RocksDbWeight::get().writes((1 as u64).saturating_mul(y as u64)))
	}
}
