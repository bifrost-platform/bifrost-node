use frame_support::traits::Currency;
use frame_system::pallet_prelude::BlockNumberFor;

use pallet_bfc_staking::{
	CandidateMetadata, CapacityStatus, Nominator, NominatorStatus, RewardDestination,
	TotalSnapshot, ValidatorStatus,
};

use precompile_utils::prelude::*;

use bp_staking::TierType;
use sp_core::{H160, U256};
use sp_std::{vec, vec::Vec};

pub type BalanceOf<Runtime> = <<Runtime as pallet_bfc_staking::Config>::Currency as Currency<
	<Runtime as frame_system::Config>::AccountId,
>>::Balance;

pub type StakingOf<Runtime> = pallet_bfc_staking::Pallet<Runtime>;

pub type EvmTotalOf = (
	U256,
	U256,
	U256,
	U256,
	U256,
	U256,
	U256,
	U256,
	u32,
	u32,
	u32,
	u32,
	u32,
	u32,
	U256,
	U256,
	U256,
	U256,
);

pub type EvmRoundInfoOf = (u32, u32, u32, U256, U256, U256, u32, u32);

pub type EvmCandidatePoolOf = (Vec<Address>, Vec<U256>);

pub type EvmCandidateStateOf = (
	Address,
	Address,
	U256,
	U256,
	u32,
	U256,
	U256,
	U256,
	U256,
	u32,
	u32,
	u32,
	bool,
	u32,
	U256,
	u32,
	u32,
	u32,
	U256,
	u32,
);

pub type EvmCandidateStatesOf = (
	Vec<Address>,
	Vec<Address>,
	Vec<U256>,
	Vec<U256>,
	Vec<u32>,
	Vec<U256>,
	Vec<U256>,
	Vec<U256>,
	Vec<U256>,
	Vec<u32>,
	Vec<u32>,
	Vec<u32>,
	Vec<bool>,
	Vec<u32>,
	Vec<U256>,
	Vec<u32>,
	Vec<u32>,
	Vec<u32>,
	Vec<U256>,
	Vec<u32>,
);

pub type EvmNominatorStateOf =
	(Address, U256, u32, U256, Vec<Address>, Vec<U256>, Vec<U256>, u32, U256, Vec<U256>);

pub type EvmNominatorRequestsOf =
	(Address, U256, Vec<Address>, Vec<U256>, Vec<Vec<(u32, U256)>>, Vec<u32>);

/// EVM struct for candidate states
pub struct CandidateStates<Runtime: pallet_bfc_staking::Config> {
	/// This candidate's controller account
	pub controller: Vec<Address>,
	/// This candidate's stash account
	pub stash: Vec<Address>,
	/// This candidate's current self-bond
	pub bond: Vec<BalanceOf<Runtime>>,
	/// This candidate's initial self-bond
	pub initial_bond: Vec<BalanceOf<Runtime>>,
	/// Total number of nominations to this candidate
	pub nomination_count: Vec<u32>,
	/// Self bond + sum of top nominations
	pub voting_power: Vec<BalanceOf<Runtime>>,
	/// The smallest top nomination amount
	pub lowest_top_nomination_amount: Vec<BalanceOf<Runtime>>,
	/// The highest bottom nomination amount
	pub highest_bottom_nomination_amount: Vec<BalanceOf<Runtime>>,
	/// The smallest bottom nomination amount
	pub lowest_bottom_nomination_amount: Vec<BalanceOf<Runtime>>,
	/// Capacity status for top nominations
	pub top_capacity: Vec<u32>,
	/// Capacity status for bottom nominations
	pub bottom_capacity: Vec<u32>,
	/// Current status of the validator
	pub status: Vec<u32>,
	/// Selection state of the candidate in the current round
	pub is_selected: Vec<bool>,
	/// The validator commission ratio
	pub commission: Vec<u32>,
	/// The last block number this candidate produced
	pub last_block: Vec<BlockNumberFor<Runtime>>,
	/// The total blocks this candidate produced in the current round
	pub blocks_produced: Vec<u32>,
	/// The block productivity for this candidate in the current round
	pub productivity: Vec<u32>,
	/// The destination for round rewards
	pub reward_dst: Vec<u32>,
	/// The amount of awarded tokens to this candidate
	pub awarded_tokens: Vec<BalanceOf<Runtime>>,
	/// The tier type of this candidate
	pub tier: Vec<u32>,
}

impl<Runtime> From<CandidateStates<Runtime>> for EvmCandidateStateOf
where
	Runtime: pallet_bfc_staking::Config,
	BalanceOf<Runtime>: Into<U256>,
	BlockNumberFor<Runtime>: Into<U256>,
{
	fn from(state: CandidateStates<Runtime>) -> EvmCandidateStateOf {
		(
			state.controller[0],
			state.stash[0],
			state.bond[0].into(),
			state.initial_bond[0].into(),
			state.nomination_count[0],
			state.voting_power[0].into(),
			state.lowest_top_nomination_amount[0].into(),
			state.highest_bottom_nomination_amount[0].into(),
			state.lowest_bottom_nomination_amount[0].into(),
			state.top_capacity[0],
			state.bottom_capacity[0],
			state.status[0],
			state.is_selected[0],
			state.commission[0],
			state.last_block[0].into(),
			state.blocks_produced[0],
			state.productivity[0],
			state.reward_dst[0],
			state.awarded_tokens[0].into(),
			state.tier[0],
		)
	}
}

impl<Runtime> From<CandidateStates<Runtime>> for EvmCandidateStatesOf
where
	Runtime: pallet_bfc_staking::Config,
	BalanceOf<Runtime>: Into<U256>,
	BlockNumberFor<Runtime>: Into<U256>,
{
	fn from(state: CandidateStates<Runtime>) -> EvmCandidateStatesOf {
		(
			state.controller,
			state.stash,
			state.bond.clone().into_iter().map(|b| b.into()).collect::<Vec<U256>>(),
			state.initial_bond.clone().into_iter().map(|i| i.into()).collect::<Vec<U256>>(),
			state.nomination_count,
			state.voting_power.clone().into_iter().map(|v| v.into()).collect::<Vec<U256>>(),
			state
				.lowest_top_nomination_amount
				.clone()
				.into_iter()
				.map(|n| n.into())
				.collect::<Vec<U256>>(),
			state
				.highest_bottom_nomination_amount
				.clone()
				.into_iter()
				.map(|n| n.into())
				.collect::<Vec<U256>>(),
			state
				.lowest_bottom_nomination_amount
				.clone()
				.into_iter()
				.map(|n| n.into())
				.collect::<Vec<U256>>(),
			state.top_capacity,
			state.bottom_capacity,
			state.status,
			state.is_selected,
			state.commission,
			state.last_block.clone().into_iter().map(|b| b.into()).collect::<Vec<U256>>(),
			state.blocks_produced,
			state.productivity,
			state.reward_dst,
			state
				.awarded_tokens
				.clone()
				.into_iter()
				.map(|a| a.into())
				.collect::<Vec<U256>>(),
			state.tier,
		)
	}
}

impl<Runtime> CandidateStates<Runtime>
where
	Runtime: pallet_bfc_staking::Config,
	Runtime::AccountId: Into<H160>,
{
	pub fn default() -> Self {
		CandidateStates {
			controller: vec![],
			stash: vec![],
			bond: vec![],
			initial_bond: vec![],
			nomination_count: vec![],
			voting_power: vec![],
			lowest_top_nomination_amount: vec![],
			highest_bottom_nomination_amount: vec![],
			lowest_bottom_nomination_amount: vec![],
			top_capacity: vec![],
			bottom_capacity: vec![],
			status: vec![],
			is_selected: vec![],
			commission: vec![],
			last_block: vec![],
			blocks_produced: vec![],
			productivity: vec![],
			reward_dst: vec![],
			awarded_tokens: vec![],
			tier: vec![],
		}
	}

	pub fn insert_empty(&mut self) {
		let zero = 0u32;
		self.controller.push(Address(Default::default()));
		self.stash.push(Address(Default::default()));
		self.bond.push(zero.into());
		self.initial_bond.push(zero.into());
		self.nomination_count.push(zero.into());
		self.voting_power.push(zero.into());
		self.lowest_top_nomination_amount.push(zero.into());
		self.highest_bottom_nomination_amount.push(zero.into());
		self.lowest_bottom_nomination_amount.push(zero.into());

		self.top_capacity.push(zero.into());
		self.bottom_capacity.push(zero.into());

		self.status.push(zero.into());
		self.is_selected.push(false);

		self.commission.push(zero.into());
		self.last_block.push(zero.into());
		self.blocks_produced.push(zero.into());
		self.productivity.push(zero.into());

		self.reward_dst.push(zero.into());
		self.awarded_tokens.push(zero.into());

		self.tier.push(zero.into());
	}

	pub fn insert_state(&mut self, state: CandidateState<Runtime>) {
		self.controller.push(Address(state.controller.into()));
		self.stash.push(Address(state.stash.into()));
		self.bond.push(state.bond);
		self.initial_bond.push(state.initial_bond);
		self.nomination_count.push(state.nomination_count);
		self.voting_power.push(state.voting_power);
		self.lowest_top_nomination_amount.push(state.lowest_top_nomination_amount);
		self.highest_bottom_nomination_amount
			.push(state.highest_bottom_nomination_amount);
		self.lowest_bottom_nomination_amount.push(state.lowest_bottom_nomination_amount);
		self.top_capacity.push(state.top_capacity);
		self.bottom_capacity.push(state.bottom_capacity);
		self.status.push(state.status);
		self.is_selected.push(state.is_selected);
		self.commission.push(state.commission);
		self.last_block.push(state.last_block);
		self.blocks_produced.push(state.blocks_produced);
		self.productivity.push(state.productivity);
		self.reward_dst.push(state.reward_dst);
		self.awarded_tokens.push(state.awarded_tokens);
		self.tier.push(state.tier);
	}
}

/// EVM struct for candidate state
pub struct CandidateState<Runtime: pallet_bfc_staking::Config> {
	/// This candidate's controller account
	pub controller: Address,
	/// This candidate's stash account
	pub stash: Address,
	/// This candidate's current self-bond
	pub bond: BalanceOf<Runtime>,
	/// This candidate's initial self-bond
	pub initial_bond: BalanceOf<Runtime>,
	/// Total number of nominations to this candidate
	pub nomination_count: u32,
	/// Self bond + sum of top nominations
	pub voting_power: BalanceOf<Runtime>,
	/// The smallest top nomination amount
	pub lowest_top_nomination_amount: BalanceOf<Runtime>,
	/// The highest bottom nomination amount
	pub highest_bottom_nomination_amount: BalanceOf<Runtime>,
	/// The smallest bottom nomination amount
	pub lowest_bottom_nomination_amount: BalanceOf<Runtime>,
	/// Capacity status for top nominations
	pub top_capacity: u32,
	/// Capacity status for bottom nominations
	pub bottom_capacity: u32,
	/// Current status of the validator
	pub status: u32,
	/// Selection state of the candidate in the current round
	pub is_selected: bool,
	/// The validator commission ratio
	pub commission: u32,
	/// The last block number this candidate produced
	pub last_block: BlockNumberFor<Runtime>,
	/// The total blocks this candidate produced in the current round
	pub blocks_produced: u32,
	/// The block productivity for this candidate in the current round
	pub productivity: u32,
	/// The destination for round rewards
	pub reward_dst: u32,
	/// The amount of awarded tokens to this candidate
	pub awarded_tokens: BalanceOf<Runtime>,
	/// The tier type of this candidate
	pub tier: u32,
}

impl<Runtime> CandidateState<Runtime>
where
	Runtime: pallet_bfc_staking::Config,
	Runtime::AccountId: Into<H160>,
	BlockNumberFor<Runtime>: Into<U256>,
{
	pub fn default() -> Self {
		let zero = 0u32;
		CandidateState {
			controller: Address(Default::default()),
			stash: Address(Default::default()),
			bond: zero.into(),
			initial_bond: zero.into(),
			nomination_count: zero.into(),
			voting_power: zero.into(),
			lowest_top_nomination_amount: zero.into(),
			highest_bottom_nomination_amount: zero.into(),
			lowest_bottom_nomination_amount: zero.into(),
			top_capacity: zero.into(),
			bottom_capacity: zero.into(),
			status: zero.into(),
			is_selected: false,
			commission: zero.into(),
			last_block: zero.into(),
			blocks_produced: zero.into(),
			productivity: zero.into(),
			reward_dst: zero.into(),
			awarded_tokens: zero.into(),
			tier: zero.into(),
		}
	}

	pub fn set_state(
		&mut self,
		controller: Runtime::AccountId,
		state: CandidateMetadata<Runtime::AccountId, BalanceOf<Runtime>, BlockNumberFor<Runtime>>,
	) {
		self.controller = Address(controller.into());
		self.stash = Address(state.stash.into());
		self.bond = state.bond.into();
		self.initial_bond = state.initial_bond.into();
		self.nomination_count = state.nomination_count.into();
		self.voting_power = state.voting_power.into();
		self.lowest_top_nomination_amount = state.lowest_top_nomination_amount.into();
		self.highest_bottom_nomination_amount = state.highest_bottom_nomination_amount.into();
		self.lowest_bottom_nomination_amount = state.lowest_bottom_nomination_amount.into();

		self.top_capacity = match state.top_capacity {
			CapacityStatus::Full => 2u32.into(),
			CapacityStatus::Partial => 1u32.into(),
			CapacityStatus::Empty => 0u32.into(),
		};
		self.bottom_capacity = match state.bottom_capacity {
			CapacityStatus::Full => 2u32.into(),
			CapacityStatus::Partial => 1u32.into(),
			CapacityStatus::Empty => 0u32.into(),
		};

		self.status = match state.status {
			ValidatorStatus::KickedOut => 2u32.into(),
			ValidatorStatus::Active => 1u32.into(),
			ValidatorStatus::Idle => 0u32.into(),
			ValidatorStatus::Leaving(when) => when.into(),
		};
		self.is_selected = state.is_selected.into();

		self.commission = state.commission.deconstruct();
		self.last_block = state.last_block;
		self.blocks_produced = state.blocks_produced.into();
		self.productivity = state.productivity.deconstruct();

		self.reward_dst = match state.reward_dst {
			RewardDestination::Staked => 0u32.into(),
			RewardDestination::Account => 1u32.into(),
		};
		self.awarded_tokens = state.awarded_tokens.into();

		self.tier = match state.tier {
			TierType::Full => 2u32.into(),
			TierType::Basic => 1u32.into(),
			_ => 0u32.into(),
		};
	}
}

/// EVM struct for nominator state
pub struct NominatorState<Runtime: pallet_bfc_staking::Config> {
	/// The candidates nominated by this nominator
	pub candidates: Vec<Address>,
	/// The current state of nominations by this nominator
	pub nominations: Vec<BalanceOf<Runtime>>,
	/// The initial state of nominations by this nominator
	pub initial_nominations: Vec<BalanceOf<Runtime>>,
	/// The total balance locked for this nominator
	pub total: BalanceOf<Runtime>,
	/// The sum of pending revocation amounts + bond less amounts
	pub request_less_total: BalanceOf<Runtime>,
	/// The status of this nominator
	pub status: u32,
	/// The destination for round rewards
	pub reward_dst: u32,
	/// The total amount of awarded tokens to this nominator
	pub awarded_tokens: BalanceOf<Runtime>,
	/// The amount of awarded tokens to this nominator per candidate
	pub awarded_tokens_per_candidate: Vec<BalanceOf<Runtime>>,
}

impl<Runtime> NominatorState<Runtime>
where
	Runtime: pallet_bfc_staking::Config,
	Runtime::AccountId: Into<H160>,
	BalanceOf<Runtime>: Into<U256>,
{
	pub fn default() -> Self {
		let zero = 0u32;
		NominatorState {
			candidates: vec![],
			nominations: vec![],
			initial_nominations: vec![],
			total: zero.into(),
			request_less_total: zero.into(),
			status: zero.into(),
			reward_dst: zero.into(),
			awarded_tokens: zero.into(),
			awarded_tokens_per_candidate: vec![],
		}
	}

	pub fn set_state(&mut self, state: Nominator<Runtime::AccountId, BalanceOf<Runtime>>) {
		state.nominations.into_iter().for_each(|(owner, amount)| {
			self.candidates.push(Address(owner.into()));
			self.nominations.push(amount);
		});
		state.initial_nominations.into_iter().for_each(|(_, amount)| {
			self.initial_nominations.push(amount);
		});

		self.total = state.total;
		self.request_less_total = state.requests.less_total;

		self.status = match state.status {
			NominatorStatus::Active => 1u32.into(),
			NominatorStatus::Leaving(when) => when.into(),
		};

		self.reward_dst = match state.reward_dst {
			RewardDestination::Staked => 0u32.into(),
			RewardDestination::Account => 1u32.into(),
		};

		state.awarded_tokens_per_candidate.iter().for_each(|(_, amount)| {
			self.awarded_tokens_per_candidate.push(*amount);
		});

		self.awarded_tokens = state.awarded_tokens;
	}

	pub fn from_owner(&self, owner: Address) -> EvmNominatorStateOf {
		(
			owner,
			self.total.into(),
			self.status,
			self.request_less_total.into(),
			self.candidates.clone(),
			self.nominations.clone().into_iter().map(|n| n.into()).collect::<Vec<U256>>(),
			self.initial_nominations
				.clone()
				.into_iter()
				.map(|n| n.into())
				.collect::<Vec<U256>>(),
			self.reward_dst,
			self.awarded_tokens.into(),
			self.awarded_tokens_per_candidate
				.clone()
				.into_iter()
				.map(|a| a.into())
				.collect::<Vec<U256>>(),
		)
	}
}

pub struct TotalStake<Runtime: pallet_bfc_staking::Config> {
	/// The total self-bonded amount
	pub total_self_bond: BalanceOf<Runtime>,
	/// The active self-bonded amount of selected validators
	pub active_self_bond: BalanceOf<Runtime>,
	/// The total (top + bottom) nominations
	pub total_nominations: BalanceOf<Runtime>,
	/// The total top nominations
	pub total_top_nominations: BalanceOf<Runtime>,
	/// The total bottom nominations
	pub total_bottom_nominations: BalanceOf<Runtime>,
	/// The active (top + bottom) nominations of selected validators
	pub active_nominations: BalanceOf<Runtime>,
	/// The active top nominations of selected validators
	pub active_top_nominations: BalanceOf<Runtime>,
	/// The active bottom nominations of selected validators
	pub active_bottom_nominations: BalanceOf<Runtime>,
	/// The count of total nominators (top + bottom)
	pub total_nominators: u32,
	/// The count of total top nominators
	pub total_top_nominators: u32,
	/// The count of total bottom nominators
	pub total_bottom_nominators: u32,
	/// The count of active nominators (top + bottom) of selected validators
	pub active_nominators: u32,
	/// The count of active top nominators
	pub active_top_nominators: u32,
	/// The count of active bottom nominators
	pub active_bottom_nominators: u32,
	/// The total staked amount (self-bond + top/bottom nominations)
	pub total_stake: BalanceOf<Runtime>,
	/// The active staked amount (self-bond + top/bottom nominations) of selected validators
	pub active_stake: BalanceOf<Runtime>,
	/// The total voting power (self-bond + top nominations)
	pub total_voting_power: BalanceOf<Runtime>,
	/// The active voting power (self-bond + top nominations) of selected validators
	pub active_voting_power: BalanceOf<Runtime>,
}

impl<Runtime> From<TotalStake<Runtime>> for EvmTotalOf
where
	Runtime: pallet_bfc_staking::Config,
	BalanceOf<Runtime>: Into<U256>,
{
	fn from(total: TotalStake<Runtime>) -> Self {
		(
			total.total_self_bond.into(),
			total.active_self_bond.into(),
			total.total_nominations.into(),
			total.total_top_nominations.into(),
			total.total_bottom_nominations.into(),
			total.active_nominations.into(),
			total.active_top_nominations.into(),
			total.active_bottom_nominations.into(),
			total.total_nominators,
			total.total_top_nominators,
			total.total_bottom_nominators,
			total.active_nominators,
			total.active_top_nominators,
			total.active_bottom_nominators,
			total.total_stake.into(),
			total.active_stake.into(),
			total.total_voting_power.into(),
			total.active_voting_power.into(),
		)
	}
}

impl<Runtime> TotalStake<Runtime>
where
	Runtime: pallet_bfc_staking::Config,
{
	pub fn default() -> Self {
		let zero = 0u32;
		TotalStake {
			total_self_bond: zero.into(),
			active_self_bond: zero.into(),
			total_nominations: zero.into(),
			total_top_nominations: zero.into(),
			total_bottom_nominations: zero.into(),
			active_nominations: zero.into(),
			active_top_nominations: zero.into(),
			active_bottom_nominations: zero.into(),
			total_nominators: zero.into(),
			total_top_nominators: zero.into(),
			total_bottom_nominators: zero.into(),
			active_nominators: zero.into(),
			active_top_nominators: zero.into(),
			active_bottom_nominators: zero.into(),
			total_stake: zero.into(),
			active_stake: zero.into(),
			total_voting_power: zero.into(),
			active_voting_power: zero.into(),
		}
	}

	pub fn set_stake(&mut self, stake: TotalSnapshot<BalanceOf<Runtime>>) {
		self.total_self_bond = stake.total_self_bond;
		self.active_self_bond = stake.active_self_bond;
		self.total_nominations = stake.total_nominations;
		self.total_top_nominations = stake.total_top_nominations;
		self.total_bottom_nominations = stake.total_bottom_nominations;
		self.active_nominations = stake.active_nominations;
		self.active_top_nominations = stake.active_top_nominations;
		self.active_bottom_nominations = stake.active_bottom_nominations;
		self.total_nominators = stake.total_nominators;
		self.total_top_nominators = stake.total_top_nominators;
		self.total_bottom_nominators = stake.total_bottom_nominators;
		self.active_nominators = stake.active_nominators;
		self.active_top_nominators = stake.active_top_nominators;
		self.active_bottom_nominators = stake.active_bottom_nominators;
		self.total_stake = stake.total_stake;
		self.active_stake = stake.active_stake;
		self.total_voting_power = stake.total_voting_power;
		self.active_voting_power = stake.active_voting_power;
	}
}
