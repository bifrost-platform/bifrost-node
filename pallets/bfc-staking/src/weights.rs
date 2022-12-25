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
	fn validate() -> Weight;
}

/// Weights for pallet_bfc_staking using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	fn hotfix_remove_nomination_requests(x: u32) -> Weight {
		(0 as Weight) // Standard Error: 3_000
			.saturating_add((8_132_000 as Weight).saturating_mul(x as Weight))
			.saturating_add(T::DbWeight::get().reads((1 as Weight).saturating_mul(x as Weight)))
			.saturating_add(T::DbWeight::get().writes((1 as Weight).saturating_mul(x as Weight)))
	}
	fn hotfix_update_candidate_pool_value(x: u32) -> Weight {
		(0 as Weight) // Standard Error: 147_000
			.saturating_add((26_825_000 as Weight).saturating_mul(x as Weight))
			.saturating_add(T::DbWeight::get().reads((1 as Weight).saturating_mul(x as Weight)))
			.saturating_add(T::DbWeight::get().writes(1 as Weight))
	}
	fn set_staking_expectations() -> Weight {
		(20_719_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(5 as Weight))
			.saturating_add(T::DbWeight::get().writes(3 as Weight))
	}
	fn set_inflation() -> Weight {
		(63_011_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(6 as Weight))
			.saturating_add(T::DbWeight::get().writes(3 as Weight))
	}
	fn set_max_total_selected() -> Weight {
		(18_402_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(5 as Weight))
			.saturating_add(T::DbWeight::get().writes(3 as Weight))
	}
	fn set_min_total_selected() -> Weight {
		(18_402_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(5 as Weight))
			.saturating_add(T::DbWeight::get().writes(3 as Weight))
	}
	fn set_default_validator_commission() -> Weight {
		(18_178_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(5 as Weight))
			.saturating_add(T::DbWeight::get().writes(3 as Weight))
	}
	fn set_max_validator_commission() -> Weight {
		(18_178_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(5 as Weight))
			.saturating_add(T::DbWeight::get().writes(3 as Weight))
	}
	fn set_validator_commission() -> Weight {
		(18_178_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(5 as Weight))
			.saturating_add(T::DbWeight::get().writes(3 as Weight))
	}
	fn cancel_validator_commission_set() -> Weight {
		(18_178_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(5 as Weight))
			.saturating_add(T::DbWeight::get().writes(3 as Weight))
	}
	fn set_validator_tier() -> Weight {
		(18_178_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(5 as Weight))
			.saturating_add(T::DbWeight::get().writes(3 as Weight))
	}
	fn set_blocks_per_round() -> Weight {
		(65_939_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(6 as Weight))
			.saturating_add(T::DbWeight::get().writes(4 as Weight))
	}
	fn set_storage_cache_lifetime() -> Weight {
		(18_178_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(6 as Weight))
			.saturating_add(T::DbWeight::get().writes(4 as Weight))
	}
	fn set_controller() -> Weight {
		(18_178_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(6 as Weight))
			.saturating_add(T::DbWeight::get().writes(4 as Weight))
	}
	fn cancel_controller_set() -> Weight {
		(18_178_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(6 as Weight))
			.saturating_add(T::DbWeight::get().writes(4 as Weight))
	}
	fn set_candidate_reward_dst() -> Weight {
		(18_178_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(6 as Weight))
			.saturating_add(T::DbWeight::get().writes(4 as Weight))
	}
	fn set_nominator_reward_dst() -> Weight {
		(18_178_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(6 as Weight))
			.saturating_add(T::DbWeight::get().writes(4 as Weight))
	}
	fn join_candidates(x: u32) -> Weight {
		(80_619_000 as Weight) // Standard Error: 1_000
			.saturating_add((107_000 as Weight).saturating_mul(x as Weight))
			.saturating_add(T::DbWeight::get().reads(9 as Weight))
			.saturating_add(T::DbWeight::get().writes(8 as Weight))
	}
	fn schedule_leave_candidates(x: u32) -> Weight {
		(50_933_000 as Weight) // Standard Error: 1_000
			.saturating_add((108_000 as Weight).saturating_mul(x as Weight))
			.saturating_add(T::DbWeight::get().reads(7 as Weight))
			.saturating_add(T::DbWeight::get().writes(4 as Weight))
	}
	fn execute_leave_candidates(x: u32) -> Weight {
		(8_634_000 as Weight) // Standard Error: 6_000
			.saturating_add((26_979_000 as Weight).saturating_mul(x as Weight))
			.saturating_add(T::DbWeight::get().reads(8 as Weight))
			.saturating_add(T::DbWeight::get().reads((2 as Weight).saturating_mul(x as Weight)))
			.saturating_add(T::DbWeight::get().writes(5 as Weight))
			.saturating_add(T::DbWeight::get().writes((2 as Weight).saturating_mul(x as Weight)))
	}
	fn cancel_leave_candidates(x: u32) -> Weight {
		(43_482_000 as Weight) // Standard Error: 0
			.saturating_add((111_000 as Weight).saturating_mul(x as Weight))
			.saturating_add(T::DbWeight::get().reads(6 as Weight))
			.saturating_add(T::DbWeight::get().writes(4 as Weight))
	}
	fn go_offline() -> Weight {
		(30_778_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(6 as Weight))
			.saturating_add(T::DbWeight::get().writes(4 as Weight))
	}
	fn go_online() -> Weight {
		(31_178_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(6 as Weight))
			.saturating_add(T::DbWeight::get().writes(4 as Weight))
	}
	fn candidate_bond_more() -> Weight {
		(53_492_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(8 as Weight))
			.saturating_add(T::DbWeight::get().writes(6 as Weight))
	}
	fn schedule_candidate_bond_less() -> Weight {
		(29_393_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(6 as Weight))
			.saturating_add(T::DbWeight::get().writes(3 as Weight))
	}
	fn execute_candidate_bond_less() -> Weight {
		(62_395_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(9 as Weight))
			.saturating_add(T::DbWeight::get().writes(6 as Weight))
	}
	fn cancel_candidate_bond_less() -> Weight {
		(25_564_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(5 as Weight))
			.saturating_add(T::DbWeight::get().writes(3 as Weight))
	}
	fn nominate(x: u32, y: u32) -> Weight {
		(103_760_000 as Weight) // Standard Error: 12_000
			.saturating_add((198_000 as Weight).saturating_mul(x as Weight)) // Standard Error: 3000
			.saturating_add((112_000 as Weight).saturating_mul(y as Weight))
			.saturating_add(T::DbWeight::get().reads(10 as Weight))
			.saturating_add(T::DbWeight::get().writes(8 as Weight))
	}
	fn schedule_leave_nominators() -> Weight {
		(30_908_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(6 as Weight))
			.saturating_add(T::DbWeight::get().writes(3 as Weight))
	}
	fn execute_leave_nominators(x: u32) -> Weight {
		(1_091_000 as Weight) // Standard Error: 14_000
			.saturating_add((37_192_000 as Weight).saturating_mul(x as Weight))
			.saturating_add(T::DbWeight::get().reads(6 as Weight))
			.saturating_add(T::DbWeight::get().reads((2 as Weight).saturating_mul(x as Weight)))
			.saturating_add(T::DbWeight::get().writes(3 as Weight))
			.saturating_add(T::DbWeight::get().writes((2 as Weight).saturating_mul(x as Weight)))
	}
	fn cancel_leave_nominators() -> Weight {
		(26_796_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(5 as Weight))
			.saturating_add(T::DbWeight::get().writes(3 as Weight))
	}
	fn schedule_revoke_nomination() -> Weight {
		(37_580_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(7 as Weight))
			.saturating_add(T::DbWeight::get().writes(4 as Weight))
	}
	fn nominator_bond_more() -> Weight {
		(65_757_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(9 as Weight))
			.saturating_add(T::DbWeight::get().writes(7 as Weight))
	}
	fn schedule_nominator_bond_less() -> Weight {
		(70_859_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(9 as Weight))
			.saturating_add(T::DbWeight::get().writes(7 as Weight))
	}
	fn execute_revoke_nomination() -> Weight {
		(87_836_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(10 as Weight))
			.saturating_add(T::DbWeight::get().writes(7 as Weight))
	}
	fn execute_nominator_bond_less() -> Weight {
		(80_983_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(11 as Weight))
			.saturating_add(T::DbWeight::get().writes(8 as Weight))
	}
	fn cancel_revoke_nomination() -> Weight {
		(37_923_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(7 as Weight))
			.saturating_add(T::DbWeight::get().writes(4 as Weight))
	}
	fn cancel_nominator_bond_less() -> Weight {
		(70_813_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(9 as Weight))
			.saturating_add(T::DbWeight::get().writes(7 as Weight))
	}
	fn round_transition_on_initialize(x: u32, y: u32) -> Weight {
		(0 as Weight) // Standard Error: 4_087_000
			// Standard Error: 12_000
			.saturating_add((100_164_000 as Weight).saturating_mul(x as Weight))
			.saturating_add((1_202_000 as Weight).saturating_mul(y as Weight))
			.saturating_add(T::DbWeight::get().reads((4 as Weight).saturating_mul(x as Weight)))
			.saturating_add(T::DbWeight::get().writes((3 as Weight).saturating_mul(x as Weight)))
	}
	fn base_on_initialize() -> Weight {
		(4_913_000 as Weight).saturating_add(T::DbWeight::get().reads(1 as Weight))
	}
	fn pay_one_validator_reward(y: u32) -> Weight {
		(0 as Weight)
			// Standard Error: 6_000
			.saturating_add((23_284_000 as Weight).saturating_mul(y as Weight))
			.saturating_add(T::DbWeight::get().reads(11 as Weight))
			.saturating_add(T::DbWeight::get().reads((1 as Weight).saturating_mul(y as Weight)))
			.saturating_add(T::DbWeight::get().writes(6 as Weight))
			.saturating_add(T::DbWeight::get().writes((1 as Weight).saturating_mul(y as Weight)))
	}
	fn validate() -> Weight {
		(0 as Weight)
	}
}

// For backwards compatibility and tests
impl WeightInfo for () {
	fn hotfix_remove_nomination_requests(x: u32) -> Weight {
		(0 as Weight) // Standard Error: 3_000
			.saturating_add((8_132_000 as Weight).saturating_mul(x as Weight))
			.saturating_add(RocksDbWeight::get().reads((1 as Weight).saturating_mul(x as Weight)))
			.saturating_add(RocksDbWeight::get().writes((1 as Weight).saturating_mul(x as Weight)))
	}
	fn hotfix_update_candidate_pool_value(x: u32) -> Weight {
		(0 as Weight) // Standard Error: 147_000
			.saturating_add((26_825_000 as Weight).saturating_mul(x as Weight))
			.saturating_add(RocksDbWeight::get().reads((1 as Weight).saturating_mul(x as Weight)))
			.saturating_add(RocksDbWeight::get().writes(1 as Weight))
	}
	fn set_staking_expectations() -> Weight {
		(20_719_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(5 as Weight))
			.saturating_add(RocksDbWeight::get().writes(3 as Weight))
	}
	fn set_inflation() -> Weight {
		(63_011_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(6 as Weight))
			.saturating_add(RocksDbWeight::get().writes(3 as Weight))
	}
	fn set_max_total_selected() -> Weight {
		(18_402_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(5 as Weight))
			.saturating_add(RocksDbWeight::get().writes(3 as Weight))
	}
	fn set_min_total_selected() -> Weight {
		(18_402_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(5 as Weight))
			.saturating_add(RocksDbWeight::get().writes(3 as Weight))
	}
	fn set_default_validator_commission() -> Weight {
		(18_178_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(5 as Weight))
			.saturating_add(RocksDbWeight::get().writes(3 as Weight))
	}
	fn set_max_validator_commission() -> Weight {
		(18_178_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(5 as Weight))
			.saturating_add(RocksDbWeight::get().writes(3 as Weight))
	}
	fn set_validator_commission() -> Weight {
		(18_178_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(5 as Weight))
			.saturating_add(RocksDbWeight::get().writes(3 as Weight))
	}
	fn cancel_validator_commission_set() -> Weight {
		(18_178_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(5 as Weight))
			.saturating_add(RocksDbWeight::get().writes(3 as Weight))
	}
	fn set_validator_tier() -> Weight {
		(18_178_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(5 as Weight))
			.saturating_add(RocksDbWeight::get().writes(3 as Weight))
	}
	fn set_blocks_per_round() -> Weight {
		(65_939_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(6 as Weight))
			.saturating_add(RocksDbWeight::get().writes(4 as Weight))
	}
	fn set_storage_cache_lifetime() -> Weight {
		(18_178_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(6 as Weight))
			.saturating_add(RocksDbWeight::get().writes(4 as Weight))
	}
	fn set_controller() -> Weight {
		(18_178_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(6 as Weight))
			.saturating_add(RocksDbWeight::get().writes(4 as Weight))
	}
	fn cancel_controller_set() -> Weight {
		(18_178_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(6 as Weight))
			.saturating_add(RocksDbWeight::get().writes(4 as Weight))
	}
	fn set_candidate_reward_dst() -> Weight {
		(18_178_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(6 as Weight))
			.saturating_add(RocksDbWeight::get().writes(4 as Weight))
	}
	fn set_nominator_reward_dst() -> Weight {
		(18_178_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(6 as Weight))
			.saturating_add(RocksDbWeight::get().writes(4 as Weight))
	}
	fn join_candidates(x: u32) -> Weight {
		(80_619_000 as Weight) // Standard Error: 1_000
			.saturating_add((107_000 as Weight).saturating_mul(x as Weight))
			.saturating_add(RocksDbWeight::get().reads(9 as Weight))
			.saturating_add(RocksDbWeight::get().writes(8 as Weight))
	}
	fn schedule_leave_candidates(x: u32) -> Weight {
		(50_933_000 as Weight) // Standard Error: 1_000
			.saturating_add((108_000 as Weight).saturating_mul(x as Weight))
			.saturating_add(RocksDbWeight::get().reads(7 as Weight))
			.saturating_add(RocksDbWeight::get().writes(4 as Weight))
	}
	fn execute_leave_candidates(x: u32) -> Weight {
		(8_634_000 as Weight) // Standard Error: 6_000
			.saturating_add((26_979_000 as Weight).saturating_mul(x as Weight))
			.saturating_add(RocksDbWeight::get().reads(8 as Weight))
			.saturating_add(RocksDbWeight::get().reads((2 as Weight).saturating_mul(x as Weight)))
			.saturating_add(RocksDbWeight::get().writes(5 as Weight))
			.saturating_add(RocksDbWeight::get().writes((2 as Weight).saturating_mul(x as Weight)))
	}
	fn cancel_leave_candidates(x: u32) -> Weight {
		(43_482_000 as Weight) // Standard Error: 0
			.saturating_add((111_000 as Weight).saturating_mul(x as Weight))
			.saturating_add(RocksDbWeight::get().reads(6 as Weight))
			.saturating_add(RocksDbWeight::get().writes(4 as Weight))
	}
	fn go_offline() -> Weight {
		(30_778_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(6 as Weight))
			.saturating_add(RocksDbWeight::get().writes(4 as Weight))
	}
	fn go_online() -> Weight {
		(31_178_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(6 as Weight))
			.saturating_add(RocksDbWeight::get().writes(4 as Weight))
	}
	fn candidate_bond_more() -> Weight {
		(53_492_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(8 as Weight))
			.saturating_add(RocksDbWeight::get().writes(6 as Weight))
	}
	fn schedule_candidate_bond_less() -> Weight {
		(29_393_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(6 as Weight))
			.saturating_add(RocksDbWeight::get().writes(3 as Weight))
	}
	fn execute_candidate_bond_less() -> Weight {
		(62_395_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(9 as Weight))
			.saturating_add(RocksDbWeight::get().writes(6 as Weight))
	}
	fn cancel_candidate_bond_less() -> Weight {
		(25_564_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(5 as Weight))
			.saturating_add(RocksDbWeight::get().writes(3 as Weight))
	}
	fn nominate(x: u32, y: u32) -> Weight {
		(103_760_000 as Weight) // Standard Error: 12_000
			.saturating_add((198_000 as Weight).saturating_mul(x as Weight)) // Standard Error: 3000
			.saturating_add((112_000 as Weight).saturating_mul(y as Weight))
			.saturating_add(RocksDbWeight::get().reads(10 as Weight))
			.saturating_add(RocksDbWeight::get().writes(8 as Weight))
	}
	fn schedule_leave_nominators() -> Weight {
		(30_908_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(6 as Weight))
			.saturating_add(RocksDbWeight::get().writes(3 as Weight))
	}
	fn execute_leave_nominators(x: u32) -> Weight {
		(1_091_000 as Weight) // Standard Error: 14_000
			.saturating_add((37_192_000 as Weight).saturating_mul(x as Weight))
			.saturating_add(RocksDbWeight::get().reads(6 as Weight))
			.saturating_add(RocksDbWeight::get().reads((2 as Weight).saturating_mul(x as Weight)))
			.saturating_add(RocksDbWeight::get().writes(3 as Weight))
			.saturating_add(RocksDbWeight::get().writes((2 as Weight).saturating_mul(x as Weight)))
	}
	fn cancel_leave_nominators() -> Weight {
		(26_796_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(5 as Weight))
			.saturating_add(RocksDbWeight::get().writes(3 as Weight))
	}
	fn schedule_revoke_nomination() -> Weight {
		(37_580_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(7 as Weight))
			.saturating_add(RocksDbWeight::get().writes(4 as Weight))
	}
	fn nominator_bond_more() -> Weight {
		(65_757_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(9 as Weight))
			.saturating_add(RocksDbWeight::get().writes(7 as Weight))
	}
	fn schedule_nominator_bond_less() -> Weight {
		(70_859_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(9 as Weight))
			.saturating_add(RocksDbWeight::get().writes(7 as Weight))
	}
	fn execute_revoke_nomination() -> Weight {
		(87_836_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(10 as Weight))
			.saturating_add(RocksDbWeight::get().writes(7 as Weight))
	}
	fn execute_nominator_bond_less() -> Weight {
		(80_983_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(11 as Weight))
			.saturating_add(RocksDbWeight::get().writes(8 as Weight))
	}
	fn cancel_revoke_nomination() -> Weight {
		(37_923_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(7 as Weight))
			.saturating_add(RocksDbWeight::get().writes(4 as Weight))
	}
	fn cancel_nominator_bond_less() -> Weight {
		(70_813_000 as Weight)
			.saturating_add(RocksDbWeight::get().reads(7 as Weight))
			.saturating_add(RocksDbWeight::get().writes(4 as Weight))
	}
	fn round_transition_on_initialize(x: u32, y: u32) -> Weight {
		(0 as Weight) // Standard Error: 4_087_000
			// Standard Error: 12_000
			.saturating_add((100_164_000 as Weight).saturating_mul(x as Weight))
			.saturating_add((1_202_000 as Weight).saturating_mul(y as Weight))
			.saturating_add(RocksDbWeight::get().reads((4 as Weight).saturating_mul(x as Weight)))
			.saturating_add(RocksDbWeight::get().writes((3 as Weight).saturating_mul(x as Weight)))
	}
	fn base_on_initialize() -> Weight {
		(4_913_000 as Weight).saturating_add(RocksDbWeight::get().reads(1 as Weight))
	}
	fn pay_one_validator_reward(y: u32) -> Weight {
		(0 as Weight)
			// Standard Error: 6_000
			.saturating_add((23_284_000 as Weight).saturating_mul(y as Weight))
			.saturating_add(RocksDbWeight::get().reads(11 as Weight))
			.saturating_add(RocksDbWeight::get().reads((1 as Weight).saturating_mul(y as Weight)))
			.saturating_add(RocksDbWeight::get().writes(6 as Weight))
			.saturating_add(RocksDbWeight::get().writes((1 as Weight).saturating_mul(y as Weight)))
	}

	fn validate() -> Weight {
		(0 as Weight)
	}
}
