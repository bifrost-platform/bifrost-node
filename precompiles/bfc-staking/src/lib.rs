#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(test, feature(assert_matches))]

use frame_support::{
	dispatch::{Dispatchable, GetDispatchInfo, PostDispatchInfo},
	traits::{Currency, Get},
};

use bp_staking::{RoundIndex, TierType};
use fp_evm::{Context, ExitError, ExitSucceed, PrecompileFailure, PrecompileOutput};
use pallet_bfc_staking::{
	Call as StakingCall, CandidateMetadata, CapacityStatus, NominationChange, Nominator,
	NominatorStatus, RewardDestination, TotalSnapshot, ValidatorStatus,
};
use pallet_evm::{AddressMapping, Precompile};
use precompile_utils::{
	Address, EvmData, EvmDataReader, EvmDataWriter, EvmResult, FunctionModifier, Gasometer,
	RuntimeHelper,
};
use sp_core::H160;
use sp_runtime::{traits::Zero, Perbill};
use sp_std::{convert::TryInto, fmt::Debug, marker::PhantomData, vec, vec::Vec};

type BalanceOf<Runtime> = <<Runtime as pallet_bfc_staking::Config>::Currency as Currency<
	<Runtime as frame_system::Config>::AccountId,
>>::Balance;

type BlockNumberOf<Runtime> = <Runtime as frame_system::Config>::BlockNumber;

type StakingOf<Runtime> = pallet_bfc_staking::Pallet<Runtime>;

#[precompile_utils::generate_function_selector]
#[derive(Debug, PartialEq)]
enum Action {
	// Role verifiers
	IsNominator = "is_nominator(address)",
	IsCandidate = "is_candidate(address,uint256)",
	IsSelectedCandidate = "is_selected_candidate(address,uint256)",
	IsSelectedCandidates = "is_selected_candidates(address[],uint256)",
	IsCompleteSelectedCandidates = "is_complete_selected_candidates(address[],uint256)",
	IsPreviousSelectedCandidate = "is_previous_selected_candidate(uint256,address)",
	IsPreviousSelectedCandidates = "is_previous_selected_candidates(uint256,address[])",
	// Common storage getters
	RoundInfo = "round_info()",
	LatestRound = "latest_round()",
	Majority = "majority()",
	PreviousMajority = "previous_majority(uint256)",
	Points = "points(uint256)",
	ValidatorPoints = "validator_points(uint256,address)",
	Rewards = "rewards()",
	Total = "total(uint256)",
	InflationConfig = "inflation_config()",
	InflationRate = "inflation_rate()",
	EstimatedYearlyReturn = "estimated_yearly_return(address[],uint256[])",
	MinNomination = "min_nomination()",
	MaxNominationsPerNominator = "max_nominations_per_nominator()",
	MaxNominationsPerCandidate = "max_nominations_per_candidate()",
	CandidateBondLessDelay = "candidate_bond_less_delay()",
	NominatorBondLessDelay = "nominator_bond_less_delay()",
	// Validator storage getters
	CandidateCount = "candidate_count()",
	SelectedCandidates = "selected_candidates(uint256)",
	PreviousSelectedCandidates = "previous_selected_candidates(uint256)",
	CandidatePool = "candidate_pool()",
	CandidateState = "candidate_state(address)",
	CandidateStates = "candidate_states(uint256)",
	CandidateStatesBySelection = "candidate_states_by_selection(uint256,bool)",
	CandidateRequest = "candidate_request(address)",
	CandidateTopNominations = "candidate_top_nominations(address)",
	CandidateBottomNominations = "candidate_bottom_nominations(address)",
	CandidateNominationCount = "candidate_nomination_count(address)",
	// Nominator storage getters
	NominatorState = "nominator_state(address)",
	NominatorRequests = "nominator_requests(address)",
	NominatorNominationCount = "nominator_nomination_count(address)",
	// Common dispatchable methods
	GoOffline = "go_offline()",
	GoOnline = "go_online()",
	// Validator dispatchable methods
	JoinCandidates = "join_candidates(address,address,uint256,uint256)",
	CandidateBondMore = "candidate_bond_more(uint256)",
	ScheduleLeaveCandidates = "schedule_leave_candidates(uint256)",
	ScheduleCandidateBondLess = "schedule_candidate_bond_less(uint256)",
	ExecuteLeaveCandidates = "execute_leave_candidates(uint256)",
	ExecuteCandidateBondLess = "execute_candidate_bond_less()",
	CancelLeaveCandidates = "cancel_leave_candidates(uint256)",
	CancelCandidateBondLess = "cancel_candidate_bond_less()",
	SetValidatorCommission = "set_validator_commission(uint256)",
	SetController = "set_controller(address)",
	SetCandidateRewardDst = "set_candidate_reward_dst(uint256)",
	// Nominator dispatchable methods
	Nominate = "nominate(address,uint256,uint256,uint256)",
	NominatorBondMore = "nominator_bond_more(address,uint256)",
	ScheduleLeaveNominators = "schedule_leave_nominators()",
	ScheduleRevokeNomination = "schedule_revoke_nomination(address)",
	ScheduleNominatorBondLess = "schedule_nominator_bond_less(address,uint256)",
	ExecuteLeaveNominators = "execute_leave_nominators(uint256)",
	ExecuteNominationRequest = "execute_nomination_request(address)",
	CancelLeaveNominators = "cancel_leave_nominators()",
	CancelNominationRequest = "cancel_nomination_request(address)",
	SetNominatorRewardDst = "set_nominator_reward_dst(uint256)",
}

/// EVM struct for candidate states
struct CandidateStates<Runtime: pallet_bfc_staking::Config> {
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
	pub last_block: Vec<BlockNumberOf<Runtime>>,
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

impl<Runtime> CandidateStates<Runtime>
where
	Runtime: pallet_bfc_staking::Config,
	Runtime::AccountId: Into<H160>,
{
	fn default() -> Self {
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

	fn insert_empty(&mut self) {
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

	fn insert_state(&mut self, state: CandidateState<Runtime>) {
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
struct CandidateState<Runtime: pallet_bfc_staking::Config> {
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
	pub last_block: BlockNumberOf<Runtime>,
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
{
	fn default() -> Self {
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

	fn set_state(
		&mut self,
		controller: Runtime::AccountId,
		state: CandidateMetadata<Runtime::AccountId, BalanceOf<Runtime>, BlockNumberOf<Runtime>>,
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
		self.last_block = state.last_block.into();
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
struct NominatorState<Runtime: pallet_bfc_staking::Config> {
	/// The candidates nominated by this nominator
	pub candidates: Vec<Address>,
	/// The current state of nominations by this nominator
	pub nominations: Vec<BalanceOf<Runtime>>,
	/// The initial state of nominations by this nominator
	pub initial_nominations: Vec<BalanceOf<Runtime>>,
	/// The total balance locked for this nominator
	pub total: BalanceOf<Runtime>,
	/// The number of pending revocations
	pub request_revocations_count: u32,
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
{
	fn default() -> Self {
		let zero = 0u32;
		NominatorState {
			candidates: vec![],
			nominations: vec![],
			initial_nominations: vec![],
			total: zero.into(),
			request_revocations_count: zero.into(),
			request_less_total: zero.into(),
			status: zero.into(),
			reward_dst: zero.into(),
			awarded_tokens: zero.into(),
			awarded_tokens_per_candidate: vec![],
		}
	}

	fn set_state(&mut self, state: Nominator<Runtime::AccountId, BalanceOf<Runtime>>) {
		for nomination in state.nominations.0 {
			self.candidates.push(Address(nomination.owner.into()));
			self.nominations.push(nomination.amount.into());
		}
		for nomination in state.initial_nominations.0 {
			self.initial_nominations.push(nomination.amount.into());
		}

		self.total = state.total.into();
		self.request_revocations_count = state.requests.revocations_count.into();
		self.request_less_total = state.requests.less_total.into();

		self.status = match state.status {
			NominatorStatus::Active => 1u32.into(),
			NominatorStatus::Leaving(when) => when.into(),
		};

		self.reward_dst = match state.reward_dst {
			RewardDestination::Staked => 0u32.into(),
			RewardDestination::Account => 1u32.into(),
		};

		for awarded_token in state.awarded_tokens_per_candidate.0 {
			self.awarded_tokens_per_candidate.push(awarded_token.amount.into());
		}
		self.awarded_tokens = state.awarded_tokens.into();
	}
}

struct TotalStake<Runtime: pallet_bfc_staking::Config> {
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

impl<Runtime> TotalStake<Runtime>
where
	Runtime: pallet_bfc_staking::Config,
{
	fn default() -> Self {
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

	fn set_stake(&mut self, stake: TotalSnapshot<BalanceOf<Runtime>>) {
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

/// A precompile to wrap the functionality from pallet_bfc_staking.
pub struct BfcStakingPrecompile<Runtime>(PhantomData<Runtime>);

impl<Runtime> Precompile for BfcStakingPrecompile<Runtime>
where
	Runtime: pallet_bfc_staking::Config + pallet_evm::Config,
	BalanceOf<Runtime>: EvmData + Zero,
	BlockNumberOf<Runtime>: EvmData,
	Runtime::AccountId: Into<H160>,
	Runtime::Call: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	<Runtime::Call as Dispatchable>::Origin: From<Option<Runtime::AccountId>>,
	Runtime::Call: From<StakingCall<Runtime>>,
{
	fn execute(
		input: &[u8],
		target_gas: Option<u64>,
		context: &Context,
		is_static: bool,
	) -> EvmResult<PrecompileOutput> {
		let mut gasometer = Gasometer::new(target_gas);
		let gasometer = &mut gasometer;

		let (mut input, selector) = EvmDataReader::new_with_selector(gasometer, input)?;
		let input = &mut input;

		gasometer.check_function_modifier(
			context,
			is_static,
			match selector {
				Action::IsNominator |
				Action::IsCandidate |
				Action::IsSelectedCandidate |
				Action::IsSelectedCandidates |
				Action::IsCompleteSelectedCandidates |
				Action::IsPreviousSelectedCandidate |
				Action::IsPreviousSelectedCandidates |
				Action::RoundInfo |
				Action::LatestRound |
				Action::Majority |
				Action::PreviousMajority |
				Action::Points |
				Action::ValidatorPoints |
				Action::Rewards |
				Action::Total |
				Action::InflationConfig |
				Action::InflationRate |
				Action::EstimatedYearlyReturn |
				Action::MinNomination |
				Action::MaxNominationsPerNominator |
				Action::MaxNominationsPerCandidate |
				Action::CandidateBondLessDelay |
				Action::NominatorBondLessDelay |
				Action::CandidateCount |
				Action::SelectedCandidates |
				Action::PreviousSelectedCandidates |
				Action::CandidatePool |
				Action::CandidateState |
				Action::CandidateStates |
				Action::CandidateStatesBySelection |
				Action::CandidateRequest |
				Action::CandidateTopNominations |
				Action::CandidateBottomNominations |
				Action::CandidateNominationCount |
				Action::NominatorState |
				Action::NominatorRequests |
				Action::NominatorNominationCount => FunctionModifier::View,
				_ => FunctionModifier::NonPayable,
			},
		)?;

		// Return early if storage getter; return (origin, call) if dispatchable
		let (origin, call) = match selector {
			// Role verifiers
			Action::IsNominator => return Self::is_nominator(input, gasometer),
			Action::IsCandidate => return Self::is_candidate(input, gasometer),
			Action::IsSelectedCandidate => return Self::is_selected_candidate(input, gasometer),
			Action::IsSelectedCandidates => return Self::is_selected_candidates(input, gasometer),
			Action::IsCompleteSelectedCandidates =>
				return Self::is_complete_selected_candidates(input, gasometer),
			Action::IsPreviousSelectedCandidate =>
				return Self::is_previous_selected_candidate(input, gasometer),
			Action::IsPreviousSelectedCandidates =>
				return Self::is_previous_selected_candidates(input, gasometer),
			// Common storage getters
			Action::RoundInfo => return Self::round_info(gasometer),
			Action::LatestRound => return Self::latest_round(gasometer),
			Action::Majority => return Self::majority(gasometer),
			Action::PreviousMajority => return Self::previous_majority(input, gasometer),
			Action::Points => return Self::points(input, gasometer),
			Action::ValidatorPoints => return Self::validator_points(input, gasometer),
			Action::Rewards => return Self::rewards(gasometer),
			Action::Total => return Self::total(input, gasometer),
			Action::InflationConfig => return Self::inflation_config(gasometer),
			Action::InflationRate => return Self::inflation_rate(gasometer),
			Action::EstimatedYearlyReturn => return Self::estimated_yearly_return(input, gasometer),
			Action::MinNomination => return Self::min_nomination(gasometer),
			Action::MaxNominationsPerNominator =>
				return Self::max_nominations_per_nominator(gasometer),
			Action::MaxNominationsPerCandidate =>
				return Self::max_nominations_per_candidate(gasometer),
			Action::CandidateBondLessDelay => return Self::candidate_bond_less_delay(gasometer),
			Action::NominatorBondLessDelay => return Self::nominator_bond_less_delay(gasometer),
			// Validator storage getters
			Action::CandidateCount => return Self::candidate_count(gasometer),
			Action::SelectedCandidates => return Self::selected_candidates(input, gasometer),
			Action::PreviousSelectedCandidates =>
				return Self::previous_selected_candidates(input, gasometer),
			Action::CandidatePool => return Self::candidate_pool(gasometer),
			Action::CandidateState => return Self::candidate_state(input, gasometer),
			Action::CandidateStates => return Self::candidate_states(input, gasometer),
			Action::CandidateStatesBySelection =>
				return Self::candidate_states_by_selection(input, gasometer),
			Action::CandidateRequest => return Self::candidate_request(input, gasometer),
			Action::CandidateTopNominations =>
				return Self::candidate_top_nominations(input, gasometer),
			Action::CandidateBottomNominations =>
				return Self::candidate_bottom_nominations(input, gasometer),
			Action::CandidateNominationCount =>
				return Self::candidate_nomination_count(input, gasometer),
			// Nominator storage getters
			Action::NominatorState => return Self::nominator_state(input, gasometer),
			Action::NominatorRequests => return Self::nominator_requests(input, gasometer),
			Action::NominatorNominationCount =>
				return Self::nominator_nomination_count(input, gasometer),
			// Common dispatchable methods
			Action::GoOffline => Self::go_offline(context)?,
			Action::GoOnline => Self::go_online(context)?,
			// Validator dispatchable methods
			Action::JoinCandidates => Self::join_candidates(input, gasometer, context)?,
			Action::CandidateBondMore => Self::candidate_bond_more(input, gasometer, context)?,
			Action::ScheduleLeaveCandidates =>
				Self::schedule_leave_candidates(input, gasometer, context)?,
			Action::ScheduleCandidateBondLess =>
				Self::schedule_candidate_bond_less(input, gasometer, context)?,
			Action::ExecuteLeaveCandidates =>
				Self::execute_leave_candidates(input, gasometer, context)?,
			Action::ExecuteCandidateBondLess => Self::execute_candidate_bond_less(context)?,
			Action::CancelLeaveCandidates =>
				Self::cancel_leave_candidates(input, gasometer, context)?,
			Action::CancelCandidateBondLess => Self::cancel_candidate_bond_less(context)?,
			Action::SetValidatorCommission =>
				Self::set_validator_commission(input, gasometer, context)?,
			Action::SetController => Self::set_controller(input, gasometer, context)?,
			Action::SetCandidateRewardDst =>
				Self::set_candidate_reward_dst(input, gasometer, context)?,
			// Nominator dispatchable methods
			Action::Nominate => Self::nominate(input, gasometer, context)?,
			Action::NominatorBondMore => Self::nominator_bond_more(input, gasometer, context)?,
			Action::ScheduleLeaveNominators => Self::schedule_leave_nominators(context)?,
			Action::ScheduleRevokeNomination =>
				Self::schedule_revoke_nomination(input, gasometer, context)?,
			Action::ScheduleNominatorBondLess =>
				Self::schedule_nominator_bond_less(input, gasometer, context)?,
			Action::ExecuteLeaveNominators =>
				Self::execute_leave_nominators(input, gasometer, context)?,
			Action::ExecuteNominationRequest =>
				Self::execute_nomination_request(input, gasometer, context)?,
			Action::CancelLeaveNominators => Self::cancel_leave_nominators(context)?,
			Action::CancelNominationRequest =>
				Self::cancel_nomination_request(input, gasometer, context)?,
			Action::SetNominatorRewardDst =>
				Self::set_nominator_reward_dst(input, gasometer, context)?,
		};

		// Dispatch call (if enough gas).
		RuntimeHelper::<Runtime>::try_dispatch(origin, call, gasometer)?;

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: vec![],
			logs: vec![],
		})
	}
}

impl<Runtime> BfcStakingPrecompile<Runtime>
where
	Runtime: pallet_bfc_staking::Config + pallet_evm::Config,
	BalanceOf<Runtime>: EvmData + Zero,
	BlockNumberOf<Runtime>: EvmData,
	Runtime::AccountId: Into<H160>,
	Runtime::Call: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	<Runtime::Call as Dispatchable>::Origin: From<Option<Runtime::AccountId>>,
	Runtime::Call: From<StakingCall<Runtime>>,
{
	// Role Verifiers

	/// Verifies if the given `nominator` parameter is a nominator
	/// @param: `nominator` the address for which to verify
	fn is_nominator(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 1)?;
		let nominator = input.read::<Address>(gasometer)?.0;
		let nominator = Runtime::AddressMapping::into_account_id(nominator);

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let is_nominator = StakingOf::<Runtime>::is_nominator(&nominator);

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(is_nominator).build(),
			logs: vec![],
		})
	}

	/// Verifies if the given `candidate` parameter is an validator candidate
	/// @param: `candidate` the address for which to verify
	/// @param: `tier` the validator type for which to verify
	fn is_candidate(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 2)?;
		let candidate = input.read::<Address>(gasometer)?.0;
		let candidate = Runtime::AddressMapping::into_account_id(candidate);
		let raw_tier = input.read::<u32>(gasometer)?;

		let tier = match raw_tier {
			2u32 => TierType::Full,
			1u32 => TierType::Basic,
			_ => TierType::All,
		};

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let is_candidate = StakingOf::<Runtime>::is_candidate(&candidate, tier);

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(is_candidate).build(),
			logs: vec![],
		})
	}

	/// Verifies if the given `candidate` parameter is an active validator for the current round
	/// @param: `candidate` the address for which to verify
	/// @param: `tier` the validator type for which to verify
	fn is_selected_candidate(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 2)?;
		let candidate = input.read::<Address>(gasometer)?.0;
		let candidate = Runtime::AddressMapping::into_account_id(candidate);
		let raw_tier = input.read::<u32>(gasometer)?;

		let tier = match raw_tier {
			2u32 => TierType::Full,
			1u32 => TierType::Basic,
			_ => TierType::All,
		};

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let is_selected = StakingOf::<Runtime>::is_selected_candidate(&candidate, tier);

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(is_selected).build(),
			logs: vec![],
		})
	}

	/// Verifies if each of the address in the given `candidates` vector parameter
	/// is a active validator for the current round
	/// @param: `candidates` the address vector for which to verify
	/// @param: `tier` the validator type for which to verify
	fn is_selected_candidates(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 2)?;
		let candidates = input.read::<Vec<Address>>(gasometer)?;
		let raw_tier = input.read::<u32>(gasometer)?;

		let tier = match raw_tier {
			2u32 => TierType::Full,
			1u32 => TierType::Basic,
			_ => TierType::All,
		};

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let mut unique_candidates = candidates
			.clone()
			.into_iter()
			.map(|address| Runtime::AddressMapping::into_account_id(address.0))
			.collect::<Vec<Runtime::AccountId>>();

		let previous_len = unique_candidates.len();
		unique_candidates.sort();
		unique_candidates.dedup();
		let current_len = unique_candidates.len();
		if current_len < previous_len {
			return Err(PrecompileFailure::Error {
				exit_status: ExitError::Other("Duplicate candidate address received".into()),
			})
		}

		let result: bool = Self::compare_selected_candidates(candidates, tier, false);

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(result).build(),
			logs: vec![],
		})
	}

	/// Verifies if each of the address in the given `candidates` vector parameter
	/// matches with the exact active validators for the current round
	/// @param: `candidates` the address vector for which to verify
	/// @param: `tier` the validator type for which to verify
	fn is_complete_selected_candidates(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 2)?;
		let candidates = input.read::<Vec<Address>>(gasometer)?;
		let raw_tier = input.read::<u32>(gasometer)?;

		let tier = match raw_tier {
			2u32 => TierType::Full,
			1u32 => TierType::Basic,
			_ => TierType::All,
		};

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let mut unique_candidates = candidates
			.clone()
			.into_iter()
			.map(|address| Runtime::AddressMapping::into_account_id(address.0))
			.collect::<Vec<Runtime::AccountId>>();

		let previous_len = unique_candidates.len();
		unique_candidates.sort();
		unique_candidates.dedup();
		let current_len = unique_candidates.len();
		if current_len < previous_len {
			return Err(PrecompileFailure::Error {
				exit_status: ExitError::Other("Duplicate candidate address received".into()),
			})
		}

		let result: bool = Self::compare_selected_candidates(candidates, tier, true);

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(result).build(),
			logs: vec![],
		})
	}

	/// Verifies if the given `candidate` parameter was an active validator at the given
	/// `round_index` @param: `round_index` the round index for which to verify
	/// @param: `candidate` the address for which to verify
	fn is_previous_selected_candidate(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 2)?;
		let round_index = input.read::<u32>(gasometer)?;
		let candidate = input.read::<Address>(gasometer)?.0;
		let candidate = Runtime::AddressMapping::into_account_id(candidate);

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let mut result: bool = false;
		let previous_selected_candidates = <StakingOf<Runtime>>::cached_selected_candidates();

		let cached_len = previous_selected_candidates.len();
		if cached_len > 0 {
			let head_selected = &previous_selected_candidates[0];
			let tail_selected = &previous_selected_candidates[cached_len - 1];

			// out of round index
			if round_index < head_selected.0 || round_index > tail_selected.0 {
				return Err(PrecompileFailure::Error {
					exit_status: ExitError::Other("Out of round index".into()),
				})
			}
			'outer: for selected_candidates in previous_selected_candidates {
				if round_index == selected_candidates.0 {
					for selected_candidate in selected_candidates.1 {
						if candidate == selected_candidate {
							result = true;
							break 'outer
						}
					}
					break
				}
			}
		}
		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(result).build(),
			logs: vec![],
		})
	}

	/// Verifies if each of the address in the given `candidates` parameter
	/// was an active validator at the given `round_index`
	/// @param: `round_index` the round index for which to verify
	/// @param: `candidates` the address for which to verify
	fn is_previous_selected_candidates(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 2)?;
		let round_index = input.read::<u32>(gasometer)?;
		let candidates = input.read::<Vec<Address>>(gasometer)?;

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let mut unique_candidates = candidates
			.clone()
			.into_iter()
			.map(|address| Runtime::AddressMapping::into_account_id(address.0))
			.collect::<Vec<Runtime::AccountId>>();

		let previous_len = unique_candidates.len();
		unique_candidates.sort();
		unique_candidates.dedup();
		let current_len = unique_candidates.len();
		if current_len < previous_len {
			return Err(PrecompileFailure::Error {
				exit_status: ExitError::Other("Duplicate candidate address received".into()),
			})
		}
		let mut result: bool = false;

		if candidates.len() > 0 {
			let previous_selected_candidates = <StakingOf<Runtime>>::cached_selected_candidates();

			let cached_len = previous_selected_candidates.len();
			if cached_len > 0 {
				let head_selected = &previous_selected_candidates[0];
				let tail_selected = &previous_selected_candidates[cached_len - 1];

				if round_index < head_selected.0 || round_index > tail_selected.0 {
					return Err(PrecompileFailure::Error {
						exit_status: ExitError::Other("Round index out of bound".into()),
					})
				}
				'outer: for selected_candidates in previous_selected_candidates {
					if round_index == selected_candidates.0 {
						let mutated_candidates: Vec<Address> = selected_candidates
							.1
							.into_iter()
							.map(|address| Address(address.into()))
							.collect();
						for candidate in candidates {
							if !mutated_candidates.contains(&candidate) {
								break 'outer
							}
						}
						result = true;
						break
					}
				}
			}
		}

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(result).build(),
			logs: vec![],
		})
	}

	// Common storage getters

	/// Returns the information of the current round
	/// @return: The current rounds index, first session index, current session index,
	///         first round block, first session block, current block, round length, session length
	fn round_info(gasometer: &mut Gasometer) -> EvmResult<PrecompileOutput> {
		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let round_info = StakingOf::<Runtime>::round();

		let output = EvmDataWriter::new()
			.write(round_info.current_round_index)
			.write(round_info.first_session_index)
			.write(round_info.current_session_index)
			.write(round_info.first_round_block)
			.write(round_info.first_session_block)
			.write(round_info.current_block)
			.write(round_info.round_length)
			.write(round_info.session_length)
			.build();

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output,
			logs: vec![],
		})
	}

	/// Returns the latest round index
	/// @return: The latest round index
	fn latest_round(gasometer: &mut Gasometer) -> EvmResult<PrecompileOutput> {
		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let round_info = StakingOf::<Runtime>::round();

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(round_info.current_round_index).build(),
			logs: vec![],
		})
	}

	/// Returns the current rounds active validators majority
	/// @return: The current rounds majority
	fn majority(gasometer: &mut Gasometer) -> EvmResult<PrecompileOutput> {
		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let majority: u32 = StakingOf::<Runtime>::majority();

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(majority).build(),
			logs: vec![],
		})
	}

	/// Returns the given `round_index` rounds active validator majority
	/// @param: `round_index` the round index for which to verify
	/// @return: The given rounds majority
	fn previous_majority(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 1)?;
		let round_index = input.read::<u32>(gasometer)?;

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let mut result: u32 = 0;
		let previous_majority = <StakingOf<Runtime>>::cached_majority();

		let cached_len = previous_majority.len();
		if cached_len > 0 {
			let head_majority = &previous_majority[0];
			let tail_majority = &previous_majority[cached_len - 1];

			if round_index < head_majority.0 || round_index > tail_majority.0 {
				return Err(PrecompileFailure::Error {
					exit_status: ExitError::Other("Round index out of bound".into()),
				})
			}
			for majority in previous_majority {
				if round_index == majority.0 {
					result = majority.1;
					break
				}
			}
		}
		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(result).build(),
			logs: vec![],
		})
	}

	/// Returns total points awarded to all validators in the given `round_index` round
	/// @param: `round_index` the round index for which to verify
	/// @return: The total points awarded to all validators in the round
	fn points(input: &mut EvmDataReader, gasometer: &mut Gasometer) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 1)?;
		let round_index = input.read::<u32>(gasometer)?;

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let points: u32 = StakingOf::<Runtime>::points(round_index);

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(points).build(),
			logs: vec![],
		})
	}

	/// Returns total points awarded to the given `validator` in the given `round_index` round
	/// @param: `round_index` the round index for which to verify
	/// @return: The total points awarded to the validator in the given round
	fn validator_points(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 2)?;
		let round_index = input.read::<u32>(gasometer)?;
		let validator = input.read::<Address>(gasometer)?.0;
		let validator = Runtime::AddressMapping::into_account_id(validator);

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let points = <StakingOf<Runtime>>::awarded_pts(round_index, &validator);

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(points).build(),
			logs: vec![],
		})
	}

	/// Returns the amount of awarded tokens to validators and nominators since genesis
	/// @return: The total amount of awarded tokens
	fn rewards(gasometer: &mut Gasometer) -> EvmResult<PrecompileOutput> {
		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let rewards = <StakingOf<Runtime>>::awarded_tokens();

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(rewards).build(),
			logs: vec![],
		})
	}

	/// Returns total capital locked information of self-bonds and nominations of the given round
	/// @param: `round_index` the round index for which to verify
	/// @return: The total locked information
	fn total(input: &mut EvmDataReader, gasometer: &mut Gasometer) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 1)?;
		let round_index = input.read::<u32>(gasometer)?;

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let mut result = TotalStake::<Runtime>::default();
		if let Some(stake) = <StakingOf<Runtime>>::total_at_stake(round_index) {
			result.set_stake(stake);
		} else {
			return Err(PrecompileFailure::Error {
				exit_status: ExitError::Other("Out of round index".into()),
			})
		}

		let output = EvmDataWriter::new()
			.write(result.total_self_bond)
			.write(result.active_self_bond)
			.write(result.total_nominations)
			.write(result.total_top_nominations)
			.write(result.total_bottom_nominations)
			.write(result.active_nominations)
			.write(result.active_top_nominations)
			.write(result.active_bottom_nominations)
			.write(result.total_nominators)
			.write(result.total_top_nominators)
			.write(result.total_bottom_nominators)
			.write(result.active_nominators)
			.write(result.active_top_nominators)
			.write(result.active_bottom_nominators)
			.write(result.total_stake)
			.write(result.active_stake)
			.write(result.total_voting_power)
			.write(result.active_voting_power)
			.build();

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output,
			logs: vec![],
		})
	}

	/// Returns the annual stake inflation parameters
	/// @return: The annual stake inflation parameters (min, ideal, max)
	fn inflation_config(gasometer: &mut Gasometer) -> EvmResult<PrecompileOutput> {
		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let inflation = <StakingOf<Runtime>>::inflation_config();
		let output = EvmDataWriter::new()
			.write(inflation.annual.min.deconstruct())
			.write(inflation.annual.ideal.deconstruct())
			.write(inflation.annual.max.deconstruct())
			.build();

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output,
			logs: vec![],
		})
	}

	/// Returns the annual stake inflation rate
	/// @return: The annual stake inflation rate according to the current total stake
	fn inflation_rate(gasometer: &mut Gasometer) -> EvmResult<PrecompileOutput> {
		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let inflation = <StakingOf<Runtime>>::inflation_config();
		let total_stake = <StakingOf<Runtime>>::total();

		let output = {
			if total_stake <= inflation.expect.min {
				inflation.annual.max.deconstruct()
			} else if total_stake >= inflation.expect.max {
				inflation.annual.min.deconstruct()
			} else {
				inflation.annual.ideal.deconstruct()
			}
		};

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(output).build(),
			logs: vec![],
		})
	}

	/// Returns the estimated yearly return for the given `nominator`
	/// @param: `candidates` the address vector for which to estimate as the target validator
	/// @param: `amounts` the amount vector for which to estimate as the current stake amount
	/// @return: The estimated yearly return according to the requested data
	fn estimated_yearly_return(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 2)?;

		let candidates = input
			.read::<Vec<Address>>(gasometer)?
			.clone()
			.into_iter()
			.map(|address| Runtime::AddressMapping::into_account_id(address.0))
			.collect::<Vec<Runtime::AccountId>>();
		let amounts = input.read::<Vec<BalanceOf<Runtime>>>(gasometer)?;

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		if candidates.len() < 1 {
			return Err(PrecompileFailure::Error {
				exit_status: ExitError::Other("Empty candidates vector received".into()),
			})
		}
		if amounts.len() < 1 {
			return Err(PrecompileFailure::Error {
				exit_status: ExitError::Other("Empty amounts vector received".into()),
			})
		}
		if candidates.len() != amounts.len() {
			return Err(PrecompileFailure::Error {
				exit_status: ExitError::Other("Request vectors length does not match".into()),
			})
		}

		let selected_candidates = <StakingOf<Runtime>>::selected_candidates();
		if selected_candidates.len() < 1 {
			return Err(PrecompileFailure::Error {
				exit_status: ExitError::Other("Empty selected candidates".into()),
			})
		}

		let total_stake = <StakingOf<Runtime>>::total();
		let round_issuance = <StakingOf<Runtime>>::compute_issuance(total_stake);
		let validator_contribution_pct =
			Perbill::from_percent(100 / (selected_candidates.len() as u32) + 1);
		let total_reward_amount = validator_contribution_pct * round_issuance;

		let rounds_per_year = pallet_bfc_staking::inflation::rounds_per_year::<Runtime>();

		let mut estimated_yearly_return: Vec<BalanceOf<Runtime>> = vec![];
		for (idx, candidate) in candidates.iter().enumerate() {
			if let Some(state) = <StakingOf<Runtime>>::candidate_info(&candidate) {
				let validator_issuance = state.commission * round_issuance;
				let commission = validator_contribution_pct * validator_issuance;
				let amount_due = total_reward_amount - commission;

				let nominator_stake_pct = Perbill::from_rational(amounts[idx], state.voting_power);
				estimated_yearly_return
					.push((nominator_stake_pct * amount_due) * rounds_per_year.into());
			}
		}

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(estimated_yearly_return).build(),
			logs: vec![],
		})
	}

	/// Returns the minimum stake required for a nominator
	/// @return: The minimum stake required for a nominator
	fn min_nomination(gasometer: &mut Gasometer) -> EvmResult<PrecompileOutput> {
		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let min_nomination: u128 = <<Runtime as pallet_bfc_staking::Config>::MinNomination as Get<
			BalanceOf<Runtime>,
		>>::get()
			.try_into()
			.map_err(|_| gasometer.revert("Amount is too large for provided balance type"))?;

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(min_nomination).build(),
			logs: vec![],
		})
	}

	/// Returns the maximum nominations allowed per nominator
	/// @return: The maximum nominations allowed per nominator
	fn max_nominations_per_nominator(gasometer: &mut Gasometer) -> EvmResult<PrecompileOutput> {
		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let max_nominations_per_nominator: u32 =
			<<Runtime as pallet_bfc_staking::Config>::MaxNominationsPerNominator as Get<u32>>::get(
			);

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(max_nominations_per_nominator).build(),
			logs: vec![],
		})
	}

	/// Returns the maximum top and bottom nominations counted per candidate
	/// @return: The tuple of the maximum top and bottom nominations counted per candidate (top,
	/// bottom)
	fn max_nominations_per_candidate(gasometer: &mut Gasometer) -> EvmResult<PrecompileOutput> {
		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let max_top_nominations_per_candidate: u32 =
			<<Runtime as pallet_bfc_staking::Config>::MaxTopNominationsPerCandidate as Get<u32>>::get(
			);
		let max_bottom_nominations_per_candidate: u32 =
			<<Runtime as pallet_bfc_staking::Config>::MaxBottomNominationsPerCandidate as Get<
				u32,
			>>::get();

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new()
				.write(max_top_nominations_per_candidate)
				.write(max_bottom_nominations_per_candidate)
				.build(),
			logs: vec![],
		})
	}

	/// Returns the bond less delay information for candidates
	/// @return: The tuple of bond less delay for candidates (`LeaveCandidatesDelay`,
	/// `CandidateBondLessDelay`)
	fn candidate_bond_less_delay(gasometer: &mut Gasometer) -> EvmResult<PrecompileOutput> {
		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let leave_candidates_delay: RoundIndex =
			<<Runtime as pallet_bfc_staking::Config>::LeaveCandidatesDelay as Get<RoundIndex>>::get(
			);
		let candidate_bond_less_delay: RoundIndex =
			<<Runtime as pallet_bfc_staking::Config>::CandidateBondLessDelay as Get<
			RoundIndex,
			>>::get();

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new()
				.write(leave_candidates_delay)
				.write(candidate_bond_less_delay)
				.build(),
			logs: vec![],
		})
	}

	/// Returns the bond less delay information for nominators
	/// @return: The tuple of bond less delay for nominators (`LeaveNominatorsDelay`,
	/// `RevokeNominationDelay`, `NominationBondLessDelay`)
	fn nominator_bond_less_delay(gasometer: &mut Gasometer) -> EvmResult<PrecompileOutput> {
		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let leave_nominators_delay: RoundIndex =
			<<Runtime as pallet_bfc_staking::Config>::LeaveNominatorsDelay as Get<RoundIndex>>::get(
			);
		let revoke_nomination_delay: RoundIndex =
			<<Runtime as pallet_bfc_staking::Config>::RevokeNominationDelay as Get<
			RoundIndex,
			>>::get();
		let nomination_bond_less_delay: RoundIndex =
			<<Runtime as pallet_bfc_staking::Config>::NominationBondLessDelay as Get<
			RoundIndex,
			>>::get();

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new()
				.write(leave_nominators_delay)
				.write(revoke_nomination_delay)
				.write(nomination_bond_less_delay)
				.build(),
			logs: vec![],
		})
	}

	// Validator storage getters

	/// Returns the count of the current validator candidates
	/// @return: the count of the current validator candidates
	fn candidate_count(gasometer: &mut Gasometer) -> EvmResult<PrecompileOutput> {
		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let candidate_count: u32 = <StakingOf<Runtime>>::candidate_pool().len() as u32;

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(candidate_count).build(),
			logs: vec![],
		})
	}

	/// Returns a vector of the active validators addresses of the current round
	/// @param: `tier` the validator type for which to verify
	/// @return: a vector of the active validators addresses
	fn selected_candidates(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 1)?;
		let raw_tier = input.read::<u32>(gasometer)?;

		let tier = match raw_tier {
			2u32 => TierType::Full,
			1u32 => TierType::Basic,
			_ => TierType::All,
		};

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let raw_selected_candidates = match tier {
			TierType::Full => StakingOf::<Runtime>::selected_full_candidates(),
			TierType::Basic => StakingOf::<Runtime>::selected_basic_candidates(),
			TierType::All => StakingOf::<Runtime>::selected_candidates(),
		};
		let selected_candidates = raw_selected_candidates
			.into_iter()
			.map(|address| Address(address.into()))
			.collect::<Vec<Address>>();

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(selected_candidates).build(),
			logs: vec![],
		})
	}

	/// Returns a vector of the active validators addresses of the given `round_index` round
	/// @param: `round_index` the round index for which to verify
	/// @return: a vector of the active validators addresses
	fn previous_selected_candidates(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 1)?;
		let round_index = input.read::<u32>(gasometer)?;

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let mut result: Vec<Address> = vec![];
		let previous_selected_candidates = <StakingOf<Runtime>>::cached_selected_candidates();

		let cached_len = previous_selected_candidates.len();
		if cached_len > 0 {
			let head_selected = &previous_selected_candidates[0];
			let tail_selected = &previous_selected_candidates[cached_len - 1];

			// out of round index
			if round_index < head_selected.0 || round_index > tail_selected.0 {
				return Err(PrecompileFailure::Error {
					exit_status: ExitError::Other("Out of round index".into()),
				})
			}
			for candidates in previous_selected_candidates {
				if round_index == candidates.0 {
					result =
						candidates.1.into_iter().map(|address| Address(address.into())).collect();
					break
				}
			}
		}
		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(result).build(),
			logs: vec![],
		})
	}

	/// Returns a vector of the validator candidate addresses
	/// @return: a vector of the validator candidate addresses
	fn candidate_pool(gasometer: &mut Gasometer) -> EvmResult<PrecompileOutput> {
		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let candidate_pool = <StakingOf<Runtime>>::candidate_pool();

		let mut candidates: Vec<Address> = vec![];
		let mut bonds: Vec<BalanceOf<Runtime>> = vec![];

		for candidate in candidate_pool {
			candidates.push(Address(candidate.owner.into()));
			bonds.push(candidate.amount.into());
		}

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(candidates).write(bonds).build(),
			logs: vec![],
		})
	}

	/// Returns the state of the given `candidate`
	/// @param: `candidate` the address for which to verify
	/// @return: the state of the given `candidate`
	fn candidate_state(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 1)?;
		let candidate = input.read::<Address>(gasometer)?.0;
		let candidate = Runtime::AddressMapping::into_account_id(candidate);

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let mut candidate_state = CandidateStates::<Runtime>::default();

		let mut is_existed: bool = false;
		if let Some(state) = <StakingOf<Runtime>>::candidate_info(&candidate) {
			let mut new = CandidateState::<Runtime>::default();
			new.set_state(candidate, state);
			candidate_state.insert_state(new);
			is_existed = true;
		};
		if !is_existed {
			candidate_state.insert_empty();
		}

		let output = EvmDataWriter::new()
			.write(candidate_state.controller[0])
			.write(candidate_state.stash[0])
			.write(candidate_state.bond[0])
			.write(candidate_state.initial_bond[0])
			.write(candidate_state.nomination_count[0])
			.write(candidate_state.voting_power[0])
			.write(candidate_state.lowest_top_nomination_amount[0])
			.write(candidate_state.highest_bottom_nomination_amount[0])
			.write(candidate_state.lowest_bottom_nomination_amount[0])
			.write(candidate_state.top_capacity[0])
			.write(candidate_state.bottom_capacity[0])
			.write(candidate_state.status[0])
			.write(candidate_state.is_selected[0])
			.write(candidate_state.commission[0])
			.write(candidate_state.last_block[0])
			.write(candidate_state.blocks_produced[0])
			.write(candidate_state.productivity[0])
			.write(candidate_state.reward_dst[0])
			.write(candidate_state.awarded_tokens[0])
			.write(candidate_state.tier[0])
			.build();

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output,
			logs: vec![],
		})
	}

	/// Returns the state of the entire validator candidates
	/// @param: `tier` the validator type for which to verify
	/// @return: the state of the entire validator candidates
	fn candidate_states(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 1)?;
		let raw_tier = input.read::<u32>(gasometer)?;
		let tier = match raw_tier {
			2u32 => TierType::Full,
			1u32 => TierType::Basic,
			_ => TierType::All,
		};

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let mut sorted_candidates = vec![];
		let mut candidate_states = CandidateStates::<Runtime>::default();
		for candidate in pallet_bfc_staking::CandidateInfo::<Runtime>::iter() {
			let owner: Runtime::AccountId = candidate.0;
			let state = candidate.1;
			let is_tier_identical = match tier {
				TierType::Full | TierType::Basic => state.tier == tier,
				TierType::All => true,
			};
			if is_tier_identical {
				let mut new = CandidateState::<Runtime>::default();
				new.set_state(owner, state);
				sorted_candidates.push(new);
			}
		}
		sorted_candidates.sort_by(|x, y| y.voting_power.cmp(&x.voting_power));
		for candidate in sorted_candidates {
			candidate_states.insert_state(candidate);
		}

		let output = EvmDataWriter::new()
			.write(candidate_states.controller)
			.write(candidate_states.stash)
			.write(candidate_states.bond)
			.write(candidate_states.initial_bond)
			.write(candidate_states.nomination_count)
			.write(candidate_states.voting_power)
			.write(candidate_states.lowest_top_nomination_amount)
			.write(candidate_states.highest_bottom_nomination_amount)
			.write(candidate_states.lowest_bottom_nomination_amount)
			.write(candidate_states.top_capacity)
			.write(candidate_states.bottom_capacity)
			.write(candidate_states.status)
			.write(candidate_states.is_selected)
			.write(candidate_states.commission)
			.write(candidate_states.last_block)
			.write(candidate_states.blocks_produced)
			.write(candidate_states.productivity)
			.write(candidate_states.reward_dst)
			.write(candidate_states.awarded_tokens)
			.write(candidate_states.tier)
			.build();

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output,
			logs: vec![],
		})
	}

	/// Returns the state of the validator candidates filtered by selection
	/// @param: `tier` the validator type for which to verify
	/// @param: `is_selected` which filters the candidates whether selected for the current round
	/// @return: the state of the filtered validator candidates
	fn candidate_states_by_selection(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 2)?;
		let raw_tier = input.read::<u32>(gasometer)?;
		let tier = match raw_tier {
			2u32 => TierType::Full,
			1u32 => TierType::Basic,
			_ => TierType::All,
		};
		let is_selected = input.read::<bool>(gasometer)?;

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let mut sorted_candidates = vec![];
		let mut candidate_states = CandidateStates::<Runtime>::default();
		for candidate in pallet_bfc_staking::CandidateInfo::<Runtime>::iter() {
			let owner: Runtime::AccountId = candidate.0;
			let state = candidate.1;
			if is_selected == state.is_selected {
				let is_tier_identical = match tier {
					TierType::Full | TierType::Basic => state.tier == tier,
					TierType::All => true,
				};
				if is_tier_identical {
					let mut new = CandidateState::<Runtime>::default();
					new.set_state(owner, state);
					sorted_candidates.push(new);
				}
			}
		}
		sorted_candidates.sort_by(|x, y| y.voting_power.cmp(&x.voting_power));
		for candidate in sorted_candidates {
			candidate_states.insert_state(candidate);
		}

		let output = EvmDataWriter::new()
			.write(candidate_states.controller)
			.write(candidate_states.stash)
			.write(candidate_states.bond)
			.write(candidate_states.initial_bond)
			.write(candidate_states.nomination_count)
			.write(candidate_states.voting_power)
			.write(candidate_states.lowest_top_nomination_amount)
			.write(candidate_states.highest_bottom_nomination_amount)
			.write(candidate_states.lowest_bottom_nomination_amount)
			.write(candidate_states.top_capacity)
			.write(candidate_states.bottom_capacity)
			.write(candidate_states.status)
			.write(candidate_states.is_selected)
			.write(candidate_states.commission)
			.write(candidate_states.last_block)
			.write(candidate_states.blocks_produced)
			.write(candidate_states.productivity)
			.write(candidate_states.reward_dst)
			.write(candidate_states.awarded_tokens)
			.write(candidate_states.tier)
			.build();

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output,
			logs: vec![],
		})
	}

	/// Returns the request state of the given `candidate`
	/// @param: `candidate` the address for which to verify
	/// @return: the request state of the given `candidate`
	fn candidate_request(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 1)?;
		let candidate = input.read::<Address>(gasometer)?.0;
		let candidate = Runtime::AddressMapping::into_account_id(candidate);

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let zero = 0u32;
		let mut amount: BalanceOf<Runtime> = zero.into();
		let mut when_executable: BalanceOf<Runtime> = zero.into();

		if let Some(state) = <StakingOf<Runtime>>::candidate_info(&candidate) {
			if let Some(request) = state.request {
				amount = request.amount.into();
				when_executable = request.when_executable.into();
			};
		};
		let output = EvmDataWriter::new()
			.write(Address(candidate.into()))
			.write(amount)
			.write(when_executable)
			.build();

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output,
			logs: vec![],
		})
	}

	/// Returns the top nominations information of the given `candidate`
	/// @param: `candidate` the address for which to verify
	/// @return: the top nominations of the given `candidate`
	fn candidate_top_nominations(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 1)?;
		let candidate = input.read::<Address>(gasometer)?.0;
		let candidate = Runtime::AddressMapping::into_account_id(candidate);

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let mut total: BalanceOf<Runtime> = 0u32.into();
		let mut nominators: Vec<Address> = vec![];
		let mut nomination_amounts: Vec<BalanceOf<Runtime>> = vec![];

		if let Some(top_nominations) = <StakingOf<Runtime>>::top_nominations(&candidate) {
			for nomination in top_nominations.nominations {
				nominators.push(Address(nomination.owner.into()));
				nomination_amounts.push(nomination.amount.into());
			}
			total = top_nominations.total.into();
		}

		let output = EvmDataWriter::new()
			.write(Address(candidate.into()))
			.write(total)
			.write(nominators)
			.write(nomination_amounts)
			.build();

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output,
			logs: vec![],
		})
	}

	/// Returns the bottom nominations information of the given `candidate`
	/// @param: `candidate` the address for which to verify
	/// @return: the bottom nominations of the given `candidate`
	fn candidate_bottom_nominations(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 1)?;
		let candidate = input.read::<Address>(gasometer)?.0;
		let candidate = Runtime::AddressMapping::into_account_id(candidate);

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let mut total: BalanceOf<Runtime> = 0u32.into();
		let mut nominators: Vec<Address> = vec![];
		let mut nomination_amounts: Vec<BalanceOf<Runtime>> = vec![];

		if let Some(bottom_nominations) = <StakingOf<Runtime>>::bottom_nominations(&candidate) {
			for nomination in bottom_nominations.nominations {
				nominators.push(Address(nomination.owner.into()));
				nomination_amounts.push(nomination.amount.into());
			}
			total = bottom_nominations.total.into();
		}

		let output = EvmDataWriter::new()
			.write(Address(candidate.into()))
			.write(total)
			.write(nominators)
			.write(nomination_amounts)
			.build();

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output,
			logs: vec![],
		})
	}

	/// Returns the count of nominations of the given `candidate`
	/// @param: `candidate` the address for which to verify
	/// @return: the count of nominations of the given `candidate`
	fn candidate_nomination_count(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 1)?;
		let candidate = input.read::<Address>(gasometer)?.0;
		let candidate = Runtime::AddressMapping::into_account_id(candidate);

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let result = if let Some(state) = <StakingOf<Runtime>>::candidate_info(&candidate) {
			let candidate_nomination_count: u32 = state.nomination_count;
			candidate_nomination_count
		} else {
			0u32
		};

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(result).build(),
			logs: vec![],
		})
	}

	// Nominator storage getters

	/// Returns the state of the given `nominator`
	/// @param: `nominator` the address for which to verify
	/// @return: the state of the given `nominator`
	fn nominator_state(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 1)?;
		let nominator = input.read::<Address>(gasometer)?.0;
		let nominator = Runtime::AddressMapping::into_account_id(nominator);

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let mut nominator_state = NominatorState::<Runtime>::default();

		if let Some(state) = <StakingOf<Runtime>>::nominator_state(&nominator) {
			nominator_state.set_state(state);
		};

		let output = EvmDataWriter::new()
			.write(Address(nominator.into()))
			.write(nominator_state.total)
			.write(nominator_state.status)
			.write(nominator_state.request_revocations_count)
			.write(nominator_state.request_less_total)
			.write(nominator_state.candidates)
			.write(nominator_state.nominations)
			.write(nominator_state.initial_nominations)
			.write(nominator_state.reward_dst)
			.write(nominator_state.awarded_tokens)
			.write(nominator_state.awarded_tokens_per_candidate)
			.build();

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output,
			logs: vec![],
		})
	}

	/// Returns the request state of the given `nominator`
	/// @param: `nominator` the address for which to verify
	/// @return: the request state of the given `nominator`
	fn nominator_requests(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 1)?;
		let nominator = input.read::<Address>(gasometer)?.0;
		let nominator = Runtime::AddressMapping::into_account_id(nominator);

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let zero = 0u32;
		let mut revocations_count: u32 = zero.into();
		let mut less_total: BalanceOf<Runtime> = zero.into();
		let mut candidates: Vec<Address> = vec![];
		let mut amounts: Vec<BalanceOf<Runtime>> = vec![];
		let mut when_executables: Vec<u32> = vec![];
		let mut actions: Vec<u32> = vec![];

		if let Some(state) = <StakingOf<Runtime>>::nominator_state(&nominator) {
			revocations_count = state.requests.revocations_count.into();
			less_total = state.requests.less_total.into();

			for (candidate, request) in state.requests.requests {
				candidates.push(Address(candidate.into()));
				amounts.push(request.amount.into());
				when_executables.push(request.when_executable.into());

				let action: u32 = match request.action {
					NominationChange::Revoke => 1u32.into(),
					NominationChange::Decrease => 2u32.into(),
				};
				actions.push(action.into());
			}
		}

		let output = EvmDataWriter::new()
			.write(Address(nominator.into()))
			.write(revocations_count)
			.write(less_total)
			.write(candidates)
			.write(amounts)
			.write(when_executables)
			.write(actions)
			.build();

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output,
			logs: vec![],
		})
	}

	/// Returns the count of nominations of the given `nominator`
	/// @param: `nominator` the address for which to verify
	/// @return: the count of nominations of the given `nominator`
	fn nominator_nomination_count(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 1)?;
		let nominator = input.read::<Address>(gasometer)?.0;
		let nominator = Runtime::AddressMapping::into_account_id(nominator);

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let result = if let Some(state) = <StakingOf<Runtime>>::nominator_state(&nominator) {
			let nominator_nomination_count: u32 = state.nominations.0.len() as u32;
			nominator_nomination_count
		} else {
			0u32
		};

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(result).build(),
			logs: vec![],
		})
	}

	// Common dispatchable methods

	fn go_offline(
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, StakingCall<Runtime>)> {
		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call = StakingCall::<Runtime>::go_offline {};

		Ok((Some(origin).into(), call))
	}

	fn go_online(
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, StakingCall<Runtime>)> {
		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call = StakingCall::<Runtime>::go_online {};

		Ok((Some(origin).into(), call))
	}

	// Validator dispatchable methods

	fn join_candidates(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, StakingCall<Runtime>)> {
		input.expect_arguments(gasometer, 4)?;

		let controller = input.read::<Address>(gasometer)?.0;
		let controller = Runtime::AddressMapping::into_account_id(controller);
		let relayer = input.read::<Address>(gasometer)?.0;
		let bond: BalanceOf<Runtime> = input.read(gasometer)?;
		let candidate_count = input.read(gasometer)?;

		let zero_address = Address(Default::default()).0;

		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call = {
			if relayer != zero_address {
				StakingCall::<Runtime>::join_candidates {
					controller,
					relayer: Some(Runtime::AddressMapping::into_account_id(relayer)),
					bond,
					candidate_count,
				}
			} else {
				StakingCall::<Runtime>::join_candidates {
					controller,
					relayer: None,
					bond,
					candidate_count,
				}
			}
		};

		Ok((Some(origin).into(), call))
	}

	fn candidate_bond_more(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, StakingCall<Runtime>)> {
		input.expect_arguments(gasometer, 1)?;
		let more: BalanceOf<Runtime> = input.read(gasometer)?;

		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call = StakingCall::<Runtime>::candidate_bond_more { more };

		Ok((Some(origin).into(), call))
	}

	fn schedule_leave_candidates(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, StakingCall<Runtime>)> {
		input.expect_arguments(gasometer, 1)?;
		let candidate_count = input.read(gasometer)?;

		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call = StakingCall::<Runtime>::schedule_leave_candidates { candidate_count };

		Ok((Some(origin).into(), call))
	}

	fn schedule_candidate_bond_less(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, StakingCall<Runtime>)> {
		input.expect_arguments(gasometer, 1)?;
		let less: BalanceOf<Runtime> = input.read(gasometer)?;

		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call = StakingCall::<Runtime>::schedule_candidate_bond_less { less };

		Ok((Some(origin).into(), call))
	}

	fn execute_leave_candidates(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, StakingCall<Runtime>)> {
		input.expect_arguments(gasometer, 1)?;
		let candidate_nomination_count = input.read(gasometer)?;

		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call = StakingCall::<Runtime>::execute_leave_candidates { candidate_nomination_count };

		Ok((Some(origin).into(), call))
	}

	fn execute_candidate_bond_less(
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, StakingCall<Runtime>)> {
		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call = StakingCall::<Runtime>::execute_candidate_bond_less {};

		Ok((Some(origin).into(), call))
	}

	fn cancel_leave_candidates(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, StakingCall<Runtime>)> {
		input.expect_arguments(gasometer, 1)?;
		let candidate_count = input.read(gasometer)?;

		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call = StakingCall::<Runtime>::cancel_leave_candidates { candidate_count };

		Ok((Some(origin).into(), call))
	}

	fn cancel_candidate_bond_less(
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, StakingCall<Runtime>)> {
		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call = StakingCall::<Runtime>::cancel_candidate_bond_less {};

		Ok((Some(origin).into(), call))
	}

	fn set_validator_commission(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, StakingCall<Runtime>)> {
		input.expect_arguments(gasometer, 1)?;
		let new = input.read(gasometer)?;

		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call =
			StakingCall::<Runtime>::set_validator_commission { new: Perbill::from_parts(new) };

		Ok((Some(origin).into(), call))
	}

	fn set_controller(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, StakingCall<Runtime>)> {
		input.expect_arguments(gasometer, 1)?;
		let new = Runtime::AddressMapping::into_account_id(input.read::<Address>(gasometer)?.0);

		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call = StakingCall::<Runtime>::set_controller { new };

		Ok((Some(origin).into(), call))
	}

	fn set_candidate_reward_dst(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, StakingCall<Runtime>)> {
		input.expect_arguments(gasometer, 1)?;
		let reward_dst: u8 = input.read(gasometer)?;

		let new_reward_dst = match reward_dst {
			0 => RewardDestination::Staked,
			1 => RewardDestination::Account,
			_ =>
				return Err(PrecompileFailure::Error {
					exit_status: ExitError::Other("Reward destination out of bound".into()),
				}),
		};

		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call = StakingCall::<Runtime>::set_candidate_reward_dst { new_reward_dst };

		Ok((Some(origin).into(), call))
	}

	// Nominator dispatchable methods

	fn nominate(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, StakingCall<Runtime>)> {
		input.expect_arguments(gasometer, 4)?;
		let candidate =
			Runtime::AddressMapping::into_account_id(input.read::<Address>(gasometer)?.0);
		let amount: BalanceOf<Runtime> = input.read(gasometer)?;
		let candidate_nomination_count = input.read(gasometer)?;
		let nomination_count = input.read(gasometer)?;

		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call = StakingCall::<Runtime>::nominate {
			candidate,
			amount,
			candidate_nomination_count,
			nomination_count,
		};

		Ok((Some(origin).into(), call))
	}

	fn nominator_bond_more(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, StakingCall<Runtime>)> {
		input.expect_arguments(gasometer, 2)?;
		let candidate = input.read::<Address>(gasometer)?.0;
		let candidate = Runtime::AddressMapping::into_account_id(candidate);
		let more: BalanceOf<Runtime> = input.read(gasometer)?;

		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call = StakingCall::<Runtime>::nominator_bond_more { candidate, more };

		Ok((Some(origin).into(), call))
	}

	fn schedule_leave_nominators(
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, StakingCall<Runtime>)> {
		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call = StakingCall::<Runtime>::schedule_leave_nominators {};

		Ok((Some(origin).into(), call))
	}

	fn schedule_revoke_nomination(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, StakingCall<Runtime>)> {
		input.expect_arguments(gasometer, 1)?;
		let validator = input.read::<Address>(gasometer)?.0;
		let validator = Runtime::AddressMapping::into_account_id(validator);

		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call = StakingCall::<Runtime>::schedule_revoke_nomination { validator };

		Ok((Some(origin).into(), call))
	}

	fn schedule_nominator_bond_less(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, StakingCall<Runtime>)> {
		input.expect_arguments(gasometer, 2)?;
		let candidate = input.read::<Address>(gasometer)?.0;
		let candidate = Runtime::AddressMapping::into_account_id(candidate);
		let less: BalanceOf<Runtime> = input.read(gasometer)?;

		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call = StakingCall::<Runtime>::schedule_nominator_bond_less { candidate, less };

		Ok((Some(origin).into(), call))
	}

	fn execute_leave_nominators(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, StakingCall<Runtime>)> {
		input.expect_arguments(gasometer, 1)?;
		let nomination_count = input.read(gasometer)?;

		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call = StakingCall::<Runtime>::execute_leave_nominators { nomination_count };

		Ok((Some(origin).into(), call))
	}

	fn execute_nomination_request(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, StakingCall<Runtime>)> {
		input.expect_arguments(gasometer, 1)?;
		let candidate = input.read::<Address>(gasometer)?.0;
		let candidate = Runtime::AddressMapping::into_account_id(candidate);

		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call = StakingCall::<Runtime>::execute_nomination_request { candidate };

		Ok((Some(origin).into(), call))
	}

	fn cancel_leave_nominators(
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, StakingCall<Runtime>)> {
		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call = StakingCall::<Runtime>::cancel_leave_nominators {};

		Ok((Some(origin).into(), call))
	}

	fn cancel_nomination_request(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, StakingCall<Runtime>)> {
		input.expect_arguments(gasometer, 1)?;
		let candidate = input.read::<Address>(gasometer)?.0;
		let candidate = Runtime::AddressMapping::into_account_id(candidate);

		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call = StakingCall::<Runtime>::cancel_nomination_request { candidate };

		Ok((Some(origin).into(), call))
	}

	fn set_nominator_reward_dst(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, StakingCall<Runtime>)> {
		input.expect_arguments(gasometer, 1)?;
		let reward_dst: u8 = input.read(gasometer)?;

		let new_reward_dst = match reward_dst {
			0 => RewardDestination::Staked,
			1 => RewardDestination::Account,
			_ =>
				return Err(PrecompileFailure::Error {
					exit_status: ExitError::Other("Reward destination out of bound".into()),
				}),
		};

		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call = StakingCall::<Runtime>::set_nominator_reward_dst { new_reward_dst };

		Ok((Some(origin).into(), call))
	}

	// Util methods

	fn compare_selected_candidates(
		candidates: Vec<Address>,
		tier: TierType,
		is_complete: bool,
	) -> bool {
		let mut result: bool = true;
		if candidates.len() < 1 {
			result = false;
		} else {
			let raw_selected_candidates = match tier {
				TierType::Full => StakingOf::<Runtime>::selected_full_candidates(),
				TierType::Basic => StakingOf::<Runtime>::selected_basic_candidates(),
				TierType::All => StakingOf::<Runtime>::selected_candidates(),
			};
			let selected_candidates = raw_selected_candidates
				.into_iter()
				.map(|address| Address(address.into()))
				.collect::<Vec<Address>>();
			if is_complete {
				if selected_candidates.len() != candidates.len() {
					result = false;
				} else {
					for selected_candidate in &selected_candidates {
						if !candidates.contains(&selected_candidate) {
							result = false;
							break
						}
					}
				}
			} else {
				for candidate in &candidates {
					if !selected_candidates.contains(&candidate) {
						result = false;
						break
					}
				}
			}
		}
		result
	}
}
