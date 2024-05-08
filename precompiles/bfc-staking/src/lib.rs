#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{
	dispatch::{GetDispatchInfo, PostDispatchInfo},
	pallet_prelude::ConstU32,
	traits::Get,
	BoundedBTreeSet,
};

use pallet_bfc_staking::{
	BlockNumberFor, Call as StakingCall, NominationChange, RewardDestination,
};
use pallet_evm::AddressMapping;

use precompile_utils::prelude::*;

use bp_staking::{RoundIndex, TierType, MAX_AUTHORITIES};
use fp_evm::PrecompileHandle;
use sp_core::{H160, U256};
use sp_runtime::{traits::Dispatchable, Perbill, Saturating};
use sp_std::{
	collections::btree_set::BTreeSet, convert::TryInto, marker::PhantomData, vec, vec::Vec,
};

mod types;
use types::{
	BalanceOf, CandidateState, CandidateStates, EvmCandidatePoolOf, EvmCandidateStateOf,
	EvmCandidateStatesOf, EvmNominatorRequestsOf, EvmNominatorStateOf, EvmRoundInfoOf, EvmTotalOf,
	NominatorState, StakingOf, TotalStake,
};

/// A precompile to wrap the functionality from pallet_bfc_staking.
pub struct BfcStakingPrecompile<Runtime>(PhantomData<Runtime>);

#[precompile_utils::precompile]
impl<Runtime> BfcStakingPrecompile<Runtime>
where
	Runtime: pallet_bfc_staking::Config + pallet_evm::Config,
	Runtime::AccountId: Into<H160>,
	Runtime::RuntimeCall: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	<Runtime::RuntimeCall as Dispatchable>::RuntimeOrigin: From<Option<Runtime::AccountId>>,
	Runtime::RuntimeCall: From<StakingCall<Runtime>>,
	BalanceOf<Runtime>: TryFrom<U256> + Into<U256>,
	BlockNumberFor<Runtime>: Into<U256>,
{
	// Role Verifiers

	/// Verifies if the given `nominator` parameter is a nominator
	/// @param: `nominator` the address for which to verify
	#[precompile::public("isNominator(address)")]
	#[precompile::public("is_nominator(address)")]
	#[precompile::view]
	fn is_nominator(handle: &mut impl PrecompileHandle, nominator: Address) -> EvmResult<bool> {
		let nominator = Runtime::AddressMapping::into_account_id(nominator.0);

		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let is_nominator = StakingOf::<Runtime>::is_nominator(&nominator);

		Ok(is_nominator)
	}

	/// Verifies if the given `candidate` parameter is an validator candidate
	/// @param: `candidate` the address for which to verify
	/// @param: `tier` the validator type for which to verify
	#[precompile::public("isCandidate(address,uint256)")]
	#[precompile::public("is_candidate(address,uint256)")]
	#[precompile::view]
	fn is_candidate(
		handle: &mut impl PrecompileHandle,
		candidate: Address,
		tier: u32,
	) -> EvmResult<bool> {
		let candidate = Runtime::AddressMapping::into_account_id(candidate.0);

		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let is_candidate = StakingOf::<Runtime>::is_candidate(
			&candidate,
			match tier {
				2 => TierType::Full,
				1 => TierType::Basic,
				_ => TierType::All,
			},
		);

		Ok(is_candidate)
	}

	/// Verifies if the given `candidate` parameter is an active validator for the current round
	/// @param: `candidate` the address for which to verify
	/// @param: `tier` the validator type for which to verify
	#[precompile::public("isSelectedCandidate(address,uint256)")]
	#[precompile::public("is_selected_candidate(address,uint256)")]
	#[precompile::view]
	fn is_selected_candidate(
		handle: &mut impl PrecompileHandle,
		candidate: Address,
		tier: u32,
	) -> EvmResult<bool> {
		let candidate = Runtime::AddressMapping::into_account_id(candidate.0);

		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let is_selected_candidate = StakingOf::<Runtime>::is_selected_candidate(
			&candidate,
			match tier {
				2 => TierType::Full,
				1 => TierType::Basic,
				_ => TierType::All,
			},
		);

		Ok(is_selected_candidate)
	}

	/// Verifies if each of the address in the given `candidates` vector parameter
	/// is a active validator for the current round
	/// @param: `candidates` the address vector for which to verify
	/// @param: `tier` the validator type for which to verify
	#[precompile::public("isSelectedCandidates(address[],uint256)")]
	#[precompile::public("is_selected_candidates(address[],uint256)")]
	#[precompile::view]
	fn is_selected_candidates(
		handle: &mut impl PrecompileHandle,
		candidates: Vec<Address>,
		tier: u32,
	) -> EvmResult<bool> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		Ok(Self::compare_selected_candidates(
			Self::get_unique_candidates(&candidates)?,
			match tier {
				2 => TierType::Full,
				1 => TierType::Basic,
				_ => TierType::All,
			},
			false,
		))
	}

	/// Verifies if each of the address in the given `candidates` vector parameter
	/// matches with the exact active validators for the current round
	/// @param: `candidates` the address vector for which to verify
	/// @param: `tier` the validator type for which to verify
	#[precompile::public("isCompleteSelectedCandidates(address[],uint256)")]
	#[precompile::public("is_complete_selected_candidates(address[],uint256)")]
	#[precompile::view]
	fn is_complete_selected_candidates(
		handle: &mut impl PrecompileHandle,
		candidates: Vec<Address>,
		tier: u32,
	) -> EvmResult<bool> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		Ok(Self::compare_selected_candidates(
			Self::get_unique_candidates(&candidates)?,
			match tier {
				2 => TierType::Full,
				1 => TierType::Basic,
				_ => TierType::All,
			},
			true,
		))
	}

	/// Verifies if the given `candidate` parameter was an active validator at the given
	/// `round_index`.
	///
	/// @param: `round_index` the round index for which to verify
	/// @param: `candidate` the address for which to verify
	#[precompile::public("isPreviousSelectedCandidate(uint256,address)")]
	#[precompile::public("is_previous_selected_candidate(uint256,address)")]
	#[precompile::view]
	fn is_previous_selected_candidate(
		handle: &mut impl PrecompileHandle,
		round_index: RoundIndex,
		candidate: Address,
	) -> EvmResult<bool> {
		let candidate = Runtime::AddressMapping::into_account_id(candidate.0);

		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		Ok(Self::get_previous_selected_candidates(&round_index)?.contains(&candidate))
	}

	/// Verifies if each of the address in the given `candidates` parameter
	/// was an active validator at the given `round_index`.
	///
	/// @param: `round_index` the round index for which to verify
	/// @param: `candidates` the address for which to verify
	#[precompile::public("isPreviousSelectedCandidates(uint256,address[])")]
	#[precompile::public("is_previous_selected_candidates(uint256,address[])")]
	#[precompile::view]
	fn is_previous_selected_candidates(
		handle: &mut impl PrecompileHandle,
		round_index: RoundIndex,
		candidates: Vec<Address>,
	) -> EvmResult<bool> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let previous_selected_candidates = Self::get_previous_selected_candidates(&round_index)?;
		Ok(Self::get_unique_candidates(&candidates)?
			.iter()
			.all(|candidate| previous_selected_candidates.contains(candidate)))
	}

	// Common storage getters

	#[precompile::public("validatorSeats()")]
	#[precompile::public("validator_seats()")]
	#[precompile::view]
	fn validator_seats(handle: &mut impl PrecompileHandle) -> EvmResult<(U256, U256)> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let full_validator_seats: U256 = <StakingOf<Runtime>>::max_full_selected().into();
		let basic_validator_seats: U256 = <StakingOf<Runtime>>::max_basic_selected().into();

		Ok((full_validator_seats, basic_validator_seats))
	}

	#[precompile::public("candidateMinimumSelfBond()")]
	#[precompile::public("candidate_minimum_self_bond()")]
	#[precompile::view]
	fn candidate_minimum_self_bond(handle: &mut impl PrecompileHandle) -> EvmResult<(U256, U256)> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let full_candidate_stk: U256 =
			<<Runtime as pallet_bfc_staking::Config>::MinFullCandidateStk as Get<
				BalanceOf<Runtime>,
			>>::get()
			.try_into()
			.map_err(|_| revert("Amount is too large for provided balance type"))?;

		let basic_candidate_stk: U256 =
			<<Runtime as pallet_bfc_staking::Config>::MinBasicCandidateStk as Get<
				BalanceOf<Runtime>,
			>>::get()
			.try_into()
			.map_err(|_| revert("Amount is too large for provided balance type"))?;

		Ok((full_candidate_stk, basic_candidate_stk))
	}

	#[precompile::public("candidateMinimumVotingPower()")]
	#[precompile::public("candidate_minimum_voting_power()")]
	#[precompile::view]
	fn candidate_minimum_voting_power(
		handle: &mut impl PrecompileHandle,
	) -> EvmResult<(U256, U256)> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let full_validator_stk: U256 =
			<<Runtime as pallet_bfc_staking::Config>::MinFullValidatorStk as Get<
				BalanceOf<Runtime>,
			>>::get()
			.try_into()
			.map_err(|_| revert("Amount is too large for provided balance type"))?;

		let basic_validator_stk: U256 =
			<<Runtime as pallet_bfc_staking::Config>::MinBasicValidatorStk as Get<
				BalanceOf<Runtime>,
			>>::get()
			.try_into()
			.map_err(|_| revert("Amount is too large for provided balance type"))?;

		Ok((full_validator_stk, basic_validator_stk))
	}

	/// Returns the information of the current round
	/// @return: The current rounds index, first session index, current session index,
	///          first round block, first session block, current block, round length, session length
	#[precompile::public("roundInfo()")]
	#[precompile::public("round_info()")]
	#[precompile::view]
	fn round_info(handle: &mut impl PrecompileHandle) -> EvmResult<EvmRoundInfoOf> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let round_info = StakingOf::<Runtime>::round();

		Ok((
			round_info.current_round_index,
			round_info.first_session_index,
			round_info.current_session_index,
			round_info.first_round_block.into(),
			round_info.first_session_block.into(),
			round_info.current_block.into(),
			round_info.round_length,
			round_info.session_length,
		))
	}

	/// Returns the latest round index
	/// @return: The latest round index
	#[precompile::public("latestRound()")]
	#[precompile::public("latest_round()")]
	#[precompile::view]
	fn latest_round(handle: &mut impl PrecompileHandle) -> EvmResult<u32> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let round_info = StakingOf::<Runtime>::round();

		Ok(round_info.current_round_index)
	}

	/// Returns the current rounds active validators majority
	/// @return: The current rounds majority
	#[precompile::public("majority()")]
	#[precompile::view]
	fn majority(handle: &mut impl PrecompileHandle) -> EvmResult<u32> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let majority: u32 = StakingOf::<Runtime>::majority();

		Ok(majority)
	}

	/// Returns the given `round_index` rounds active validator majority
	/// @param: `round_index` the round index for which to verify
	/// @return: The given rounds majority
	#[precompile::public("previousMajority(uint256)")]
	#[precompile::public("previous_majority(uint256)")]
	#[precompile::view]
	fn previous_majority(
		handle: &mut impl PrecompileHandle,
		round_index: RoundIndex,
	) -> EvmResult<u32> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		if let Some(previous_majority) = <StakingOf<Runtime>>::cached_majority().get(&round_index) {
			Ok(previous_majority.clone())
		} else {
			Err(RevertReason::read_out_of_bounds("round_index").into())
		}
	}

	/// Returns total points awarded to all validators in the given `round_index` round
	/// @param: `round_index` the round index for which to verify
	/// @return: The total points awarded to all validators in the round
	#[precompile::public("points(uint256)")]
	#[precompile::view]
	fn points(handle: &mut impl PrecompileHandle, round_index: RoundIndex) -> EvmResult<u32> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let points: u32 = StakingOf::<Runtime>::points(round_index);

		Ok(points)
	}

	/// Returns total points awarded to the given `validator` in the given `round_index` round
	/// @param: `round_index` the round index for which to verify
	/// @return: The total points awarded to the validator in the given round
	#[precompile::public("validatorPoints(uint256,address)")]
	#[precompile::public("validator_points(uint256,address)")]
	#[precompile::view]
	fn validator_points(
		handle: &mut impl PrecompileHandle,
		round_index: RoundIndex,
		validator: Address,
	) -> EvmResult<u32> {
		let validator = Runtime::AddressMapping::into_account_id(validator.0);

		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let points = <StakingOf<Runtime>>::awarded_pts(round_index, &validator);

		Ok(points)
	}

	/// Returns the amount of awarded tokens to validators and nominators since genesis
	/// @return: The total amount of awarded tokens
	#[precompile::public("rewards()")]
	#[precompile::view]
	fn rewards(handle: &mut impl PrecompileHandle) -> EvmResult<u128> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let rewards = <StakingOf<Runtime>>::awarded_tokens()
			.try_into()
			.map_err(|_| revert("Amount is too large for provided balance type"))?;

		Ok(rewards)
	}

	/// Returns total capital locked information of self-bonds and nominations of the given round
	/// @param: `round_index` the round index for which to verify
	/// @return: The total locked information
	#[precompile::public("total(uint256)")]
	#[precompile::view]
	fn total(handle: &mut impl PrecompileHandle, round_index: RoundIndex) -> EvmResult<EvmTotalOf> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let mut total = TotalStake::<Runtime>::default();
		if let Some(stake) = <StakingOf<Runtime>>::total_at_stake(round_index) {
			total.set_stake(stake);
		} else {
			return Err(RevertReason::read_out_of_bounds("round_index").into());
		}

		Ok(total.into())
	}

	/// Returns the annual stake inflation parameters
	/// @return: The annual stake inflation parameters (min, ideal, max)
	#[precompile::public("inflationConfig()")]
	#[precompile::public("inflation_config()")]
	#[precompile::view]
	fn inflation_config(handle: &mut impl PrecompileHandle) -> EvmResult<(u32, u32, u32)> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let inflation = <StakingOf<Runtime>>::inflation_config();

		Ok((
			inflation.annual.min.deconstruct(),
			inflation.annual.ideal.deconstruct(),
			inflation.annual.max.deconstruct(),
		))
	}

	/// Returns the annual stake inflation rate
	/// @return: The annual stake inflation rate according to the current total stake
	#[precompile::public("inflationRate()")]
	#[precompile::public("inflation_rate()")]
	#[precompile::view]
	fn inflation_rate(handle: &mut impl PrecompileHandle) -> EvmResult<u32> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let inflation = <StakingOf<Runtime>>::inflation_config();
		let total_stake = <StakingOf<Runtime>>::total();

		let inflation_rate = {
			if total_stake <= inflation.expect.min {
				inflation.annual.max.deconstruct()
			} else if total_stake >= inflation.expect.max {
				inflation.annual.min.deconstruct()
			} else {
				inflation.annual.ideal.deconstruct()
			}
		};

		Ok(inflation_rate)
	}

	/// Returns the estimated yearly return for the given `nominator`
	/// @param: `candidates` the address vector for which to estimate as the target validator
	/// @param: `amounts` the amount vector for which to estimate as the current stake amount
	/// @return: The estimated yearly return according to the requested data
	#[precompile::public("estimatedYearlyReturn(address[],uint256[])")]
	#[precompile::public("estimated_yearly_return(address[],uint256[])")]
	#[precompile::view]
	fn estimated_yearly_return(
		handle: &mut impl PrecompileHandle,
		candidates: Vec<Address>,
		amounts: Vec<U256>,
	) -> EvmResult<Vec<u128>> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let candidates = candidates
			.clone()
			.into_iter()
			.map(|address| Runtime::AddressMapping::into_account_id(address.0))
			.collect::<Vec<Runtime::AccountId>>();
		let amounts = Self::u256_array_to_amount_array(amounts)?;
		if candidates.len() < 1 {
			return Err(RevertReason::custom("Empty candidates vector received").into());
		}
		if amounts.len() < 1 {
			return Err(RevertReason::custom("Empty amounts vector received").into());
		}
		if candidates.len() != amounts.len() {
			return Err(RevertReason::custom("Request vectors length does not match").into());
		}

		let selected_candidates = <StakingOf<Runtime>>::selected_candidates();
		if selected_candidates.len() < 1 {
			return Err(RevertReason::custom("Empty selected candidates").into());
		}

		let total_stake = <StakingOf<Runtime>>::total();
		let round_issuance = <StakingOf<Runtime>>::compute_issuance(total_stake);
		let validator_contribution_pct =
			Perbill::from_percent(100 / (selected_candidates.len() as u32) + 1);
		let total_reward_amount = validator_contribution_pct * round_issuance;

		let rounds_per_year = pallet_bfc_staking::inflation::rounds_per_year::<Runtime>();

		let mut estimated_yearly_return: Vec<u128> = vec![];
		for (idx, candidate) in candidates.iter().enumerate() {
			if let Some(state) = <StakingOf<Runtime>>::candidate_info(&candidate) {
				let validator_issuance = state.commission * round_issuance;
				let commission = validator_contribution_pct * validator_issuance;
				let amount_due = total_reward_amount.saturating_sub(commission);

				let nominator_stake_pct = Perbill::from_rational(
					amounts[idx],
					state.voting_power.saturating_add(amounts[idx]),
				);
				estimated_yearly_return.push(
					((nominator_stake_pct * amount_due) * rounds_per_year.into())
						.try_into()
						.map_err(|_| revert("Amount is too large for provided balance type"))?,
				);
			} else {
				return Err(RevertReason::custom("Candidate does not exist").into());
			}
		}

		Ok(estimated_yearly_return)
	}

	#[precompile::public("estimatedYearlyReturnOnBondLess(address,address[],uint256[])")]
	#[precompile::public("estimated_yearly_return_on_bond_less(address,address[],uint256[])")]
	#[precompile::view]
	fn estimated_yearly_return_on_bond_less(
		handle: &mut impl PrecompileHandle,
		nominator: Address,
		candidates: Vec<Address>,
		amounts: Vec<U256>,
	) -> EvmResult<Vec<u128>> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let nominator = Runtime::AddressMapping::into_account_id(nominator.0);
		let candidates = candidates
			.clone()
			.into_iter()
			.map(|address| Runtime::AddressMapping::into_account_id(address.0))
			.collect::<Vec<Runtime::AccountId>>();
		let amounts = Self::u256_array_to_amount_array(amounts)?;
		if candidates.len() < 1 {
			return Err(RevertReason::custom("Empty candidates vector received").into());
		}
		if amounts.len() < 1 {
			return Err(RevertReason::custom("Empty amounts vector received").into());
		}
		if candidates.len() != amounts.len() {
			return Err(RevertReason::custom("Request vectors length does not match").into());
		}

		let nominator_state = <StakingOf<Runtime>>::nominator_state(&nominator)
			.ok_or(RevertReason::custom("Nominator does not exist"))?;

		let selected_candidates = <StakingOf<Runtime>>::selected_candidates();
		if selected_candidates.len() < 1 {
			return Err(RevertReason::custom("Empty selected candidates").into());
		}

		let total_stake = <StakingOf<Runtime>>::total();
		let round_issuance = <StakingOf<Runtime>>::compute_issuance(total_stake);
		let validator_contribution_pct =
			Perbill::from_percent(100 / (selected_candidates.len() as u32) + 1);
		let total_reward_amount = validator_contribution_pct * round_issuance;

		let rounds_per_year = pallet_bfc_staking::inflation::rounds_per_year::<Runtime>();

		let mut estimated_yearly_return: Vec<u128> = vec![];
		for (idx, candidate) in candidates.iter().enumerate() {
			if let Some(candidate_state) = <StakingOf<Runtime>>::candidate_info(&candidate) {
				let validator_issuance = candidate_state.commission * round_issuance;
				let commission = validator_contribution_pct * validator_issuance;
				let amount_due = total_reward_amount.saturating_sub(commission);

				if let Some(nomination) = nominator_state.nominations.get(&candidate) {
					let bond_less = nomination.saturating_sub(amounts[idx]);
					if bond_less.into() == U256::zero() {
						return Err(RevertReason::custom(
							"Amount is larger or equal than current nomination",
						)
						.into());
					}
					let nominator_stake_pct = Perbill::from_rational(
						bond_less,
						candidate_state.voting_power.saturating_sub(amounts[idx]),
					);
					estimated_yearly_return.push(
						((nominator_stake_pct * amount_due) * rounds_per_year.into())
							.try_into()
							.map_err(|_| revert("Amount is too large for provided balance type"))?,
					);
				} else {
					return Err(RevertReason::custom("Nomination does not exist").into());
				}
			} else {
				return Err(RevertReason::custom("Candidate does not exist").into());
			}
		}
		Ok(estimated_yearly_return)
	}

	/// Returns the minimum stake required for a nominator
	/// @return: The minimum stake required for a nominator
	#[precompile::public("minNomination()")]
	#[precompile::public("min_nomination()")]
	#[precompile::view]
	fn min_nomination(handle: &mut impl PrecompileHandle) -> EvmResult<u128> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let min_nomination: u128 = <<Runtime as pallet_bfc_staking::Config>::MinNomination as Get<
			BalanceOf<Runtime>,
		>>::get()
		.try_into()
		.map_err(|_| revert("Amount is too large for provided balance type"))?;

		Ok(min_nomination)
	}

	/// Returns the maximum nominations allowed per nominator
	/// @return: The maximum nominations allowed per nominator
	#[precompile::public("maxNominationsPerNominator()")]
	#[precompile::public("max_nominations_per_nominator()")]
	#[precompile::view]
	fn max_nominations_per_nominator(handle: &mut impl PrecompileHandle) -> EvmResult<u32> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let max_nominations_per_nominator: u32 =
			<<Runtime as pallet_bfc_staking::Config>::MaxNominationsPerNominator as Get<u32>>::get(
			);

		Ok(max_nominations_per_nominator)
	}

	/// Returns the maximum top and bottom nominations counted per candidate
	/// @return: The tuple of the maximum top and bottom nominations counted per candidate (top,
	/// bottom)
	#[precompile::public("maxNominationsPerCandidate()")]
	#[precompile::public("max_nominations_per_candidate()")]
	#[precompile::view]
	fn max_nominations_per_candidate(handle: &mut impl PrecompileHandle) -> EvmResult<(u32, u32)> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let max_top_nominations_per_candidate: u32 =
		<<Runtime as pallet_bfc_staking::Config>::MaxTopNominationsPerCandidate as Get<u32>>::get(
		);
		let max_bottom_nominations_per_candidate: u32 =
			<<Runtime as pallet_bfc_staking::Config>::MaxBottomNominationsPerCandidate as Get<
				u32,
			>>::get();

		Ok((max_top_nominations_per_candidate, max_bottom_nominations_per_candidate))
	}

	/// Returns the bond less delay information for candidates
	/// @return: The tuple of bond less delay for candidates (`LeaveCandidatesDelay`,
	/// `CandidateBondLessDelay`)
	#[precompile::public("candidateBondLessDelay()")]
	#[precompile::public("candidate_bond_less_delay()")]
	#[precompile::view]
	fn candidate_bond_less_delay(handle: &mut impl PrecompileHandle) -> EvmResult<(u32, u32)> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let leave_candidates_delay: RoundIndex =
			<<Runtime as pallet_bfc_staking::Config>::LeaveCandidatesDelay as Get<RoundIndex>>::get(
			);
		let candidate_bond_less_delay: RoundIndex =
			<<Runtime as pallet_bfc_staking::Config>::CandidateBondLessDelay as Get<
			RoundIndex,
			>>::get();

		Ok((leave_candidates_delay, candidate_bond_less_delay))
	}

	/// Returns the bond less delay information for nominators
	/// @return: The tuple of bond less delay for nominators (`LeaveNominatorsDelay`,
	/// `RevokeNominationDelay`, `NominationBondLessDelay`)
	#[precompile::public("nominatorBondLessDelay()")]
	#[precompile::public("nominator_bond_less_delay()")]
	#[precompile::view]
	fn nominator_bond_less_delay(handle: &mut impl PrecompileHandle) -> EvmResult<(u32, u32, u32)> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
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

		Ok((leave_nominators_delay, revoke_nomination_delay, nomination_bond_less_delay))
	}

	// Validator storage getters

	/// Returns the count of the current validator candidates
	/// @return: the count of the current validator candidates
	#[precompile::public("candidateCount()")]
	#[precompile::public("candidate_count()")]
	#[precompile::view]
	fn candidate_count(handle: &mut impl PrecompileHandle) -> EvmResult<u32> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let candidate_count: u32 = <StakingOf<Runtime>>::candidate_pool().len() as u32;

		Ok(candidate_count)
	}

	/// Returns a vector of the active validators addresses of the current round
	/// @param: `tier` the validator type for which to verify
	/// @return: a vector of the active validators addresses
	#[precompile::public("selectedCandidates(uint256)")]
	#[precompile::public("selected_candidates(uint256)")]
	#[precompile::view]
	fn selected_candidates(
		handle: &mut impl PrecompileHandle,
		tier: u32,
	) -> EvmResult<Vec<Address>> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let raw_selected_candidates = match tier {
			2 => StakingOf::<Runtime>::selected_full_candidates().into_inner(),
			1 => StakingOf::<Runtime>::selected_basic_candidates().into_inner(),
			_ => StakingOf::<Runtime>::selected_candidates().into_inner(),
		};
		let selected_candidates = raw_selected_candidates
			.into_iter()
			.map(|address| Address(address.into()))
			.collect::<Vec<Address>>();

		Ok(selected_candidates)
	}

	/// Returns a vector of the active validators addresses of the given `round_index` round
	/// @param: `round_index` the round index for which to verify
	/// @return: a vector of the active validators addresses
	#[precompile::public("previousSelectedCandidates(uint256)")]
	#[precompile::public("previous_selected_candidates(uint256)")]
	#[precompile::view]
	fn previous_selected_candidates(
		handle: &mut impl PrecompileHandle,
		round_index: RoundIndex,
	) -> EvmResult<Vec<Address>> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		Ok(Self::get_previous_selected_candidates(&round_index)?
			.into_iter()
			.map(|candidate| Address(candidate.into()))
			.collect::<Vec<Address>>())
	}

	/// Returns a vector of the validator candidate addresses
	/// @return: a vector of the validator candidate addresses
	#[precompile::public("candidatePool()")]
	#[precompile::public("candidate_pool()")]
	#[precompile::view]
	fn candidate_pool(handle: &mut impl PrecompileHandle) -> EvmResult<EvmCandidatePoolOf> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let candidate_pool = <StakingOf<Runtime>>::get_sorted_candidates();

		let mut candidates: Vec<Address> = vec![];
		let mut bonds: Vec<U256> = vec![];

		for candidate in candidate_pool {
			candidates.push(Address(candidate.owner.into()));
			bonds.push(candidate.amount.into());
		}

		Ok((candidates, bonds))
	}

	/// Returns the state of the given `candidate`
	/// @param: `candidate` the address for which to verify
	/// @return: the state of the given `candidate`
	#[precompile::public("candidateState(address)")]
	#[precompile::public("candidate_state(address)")]
	#[precompile::view]
	fn candidate_state(
		handle: &mut impl PrecompileHandle,
		candidate: Address,
	) -> EvmResult<EvmCandidateStateOf> {
		let candidate = Runtime::AddressMapping::into_account_id(candidate.0);

		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
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

		Ok(candidate_state.into())
	}

	/// Returns the state of the entire validator candidates
	/// @param: `tier` the validator type for which to verify
	/// @return: the state of the entire validator candidates
	#[precompile::public("candidateStates(uint256)")]
	#[precompile::public("candidate_states(uint256)")]
	#[precompile::view]
	fn candidate_states(
		handle: &mut impl PrecompileHandle,
		tier: u32,
	) -> EvmResult<EvmCandidateStatesOf> {
		let tier = match tier {
			2 => TierType::Full,
			1 => TierType::Basic,
			_ => TierType::All,
		};

		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let mut sorted_candidates = vec![];
		let mut candidate_states = CandidateStates::<Runtime>::default();
		for candidate in pallet_bfc_staking::CandidateInfo::<Runtime>::iter() {
			let owner: Runtime::AccountId = candidate.0;
			let state = candidate.1;
			let is_tier_identical = match tier {
				TierType::Full | TierType::Basic => state.tier == tier,
				_ => true,
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

		Ok(candidate_states.into())
	}

	/// Returns the state of the validator candidates filtered by selection
	/// @param: `tier` the validator type for which to verify
	/// @param: `is_selected` which filters the candidates whether selected for the current round
	/// @return: the state of the filtered validator candidates
	#[precompile::public("candidateStatesBySelection(uint256,bool)")]
	#[precompile::public("candidate_states_by_selection(uint256,bool)")]
	#[precompile::view]
	fn candidate_states_by_selection(
		handle: &mut impl PrecompileHandle,
		tier: u32,
		is_selected: bool,
	) -> EvmResult<EvmCandidateStatesOf> {
		let tier = match tier {
			2 => TierType::Full,
			1 => TierType::Basic,
			_ => TierType::All,
		};

		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
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

		Ok(candidate_states.into())
	}

	/// Returns the request state of the given `candidate`
	/// @param: `candidate` the address for which to verify
	/// @return: the request state of the given `candidate`
	#[precompile::public("candidateRequest(address)")]
	#[precompile::public("candidate_request(address)")]
	#[precompile::view]
	fn candidate_request(
		handle: &mut impl PrecompileHandle,
		candidate: Address,
	) -> EvmResult<(Address, U256, u32)> {
		let candidate = Runtime::AddressMapping::into_account_id(candidate.0);

		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let zero = 0u32;
		let mut amount: U256 = zero.into();
		let mut when_executable: u32 = zero.into();

		if let Some(state) = <StakingOf<Runtime>>::candidate_info(&candidate) {
			if let Some(request) = state.request {
				amount = request.amount.into();
				when_executable = request.when_executable.into();
			};
		};

		Ok((Address(candidate.into()), amount, when_executable))
	}

	/// Returns the top nominations information of the given `candidate`
	/// @param: `candidate` the address for which to verify
	/// @return: the top nominations of the given `candidate`
	#[precompile::public("candidateTopNominations(address)")]
	#[precompile::public("candidate_top_nominations(address)")]
	#[precompile::view]
	fn candidate_top_nominations(
		handle: &mut impl PrecompileHandle,
		candidate: Address,
	) -> EvmResult<(Address, U256, Vec<Address>, Vec<U256>)> {
		let candidate = Runtime::AddressMapping::into_account_id(candidate.0);

		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let mut total: U256 = 0u32.into();
		let mut nominators: Vec<Address> = vec![];
		let mut nomination_amounts: Vec<U256> = vec![];

		if let Some(top_nominations) = <StakingOf<Runtime>>::top_nominations(&candidate) {
			for nomination in top_nominations.nominations {
				nominators.push(Address(nomination.owner.into()));
				nomination_amounts.push(nomination.amount.into());
			}
			total = top_nominations.total.into();
		}

		Ok((Address(candidate.into()), total, nominators, nomination_amounts))
	}

	/// Returns the bottom nominations information of the given `candidate`
	/// @param: `candidate` the address for which to verify
	/// @return: the bottom nominations of the given `candidate`
	#[precompile::public("candidateBottomNominations(address)")]
	#[precompile::public("candidate_bottom_nominations(address)")]
	#[precompile::view]
	fn candidate_bottom_nominations(
		handle: &mut impl PrecompileHandle,
		candidate: Address,
	) -> EvmResult<(Address, U256, Vec<Address>, Vec<U256>)> {
		let candidate = Runtime::AddressMapping::into_account_id(candidate.0);

		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let mut total: U256 = 0u32.into();
		let mut nominators: Vec<Address> = vec![];
		let mut nomination_amounts: Vec<U256> = vec![];

		if let Some(bottom_nominations) = <StakingOf<Runtime>>::bottom_nominations(&candidate) {
			for nomination in bottom_nominations.nominations {
				nominators.push(Address(nomination.owner.into()));
				nomination_amounts.push(nomination.amount.into());
			}
			total = bottom_nominations.total.into();
		}

		Ok((Address(candidate.into()), total, nominators, nomination_amounts))
	}

	/// Returns the count of nominations of the given `candidate`
	/// @param: `candidate` the address for which to verify
	/// @return: the count of nominations of the given `candidate`
	#[precompile::public("candidateNominationCount(address)")]
	#[precompile::public("candidate_nomination_count(address)")]
	#[precompile::view]
	fn candidate_nomination_count(
		handle: &mut impl PrecompileHandle,
		candidate: Address,
	) -> EvmResult<u32> {
		let candidate = Runtime::AddressMapping::into_account_id(candidate.0);

		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let result = if let Some(state) = <StakingOf<Runtime>>::candidate_info(&candidate) {
			let candidate_nomination_count: u32 = state.nomination_count;
			candidate_nomination_count
		} else {
			0u32
		};

		Ok(result)
	}

	// Nominator storage getters

	/// Returns the state of the given `nominator`
	/// @param: `nominator` the address for which to verify
	/// @return: the state of the given `nominator`
	#[precompile::public("nominatorState(address)")]
	#[precompile::public("nominator_state(address)")]
	#[precompile::view]
	fn nominator_state(
		handle: &mut impl PrecompileHandle,
		nominator: Address,
	) -> EvmResult<EvmNominatorStateOf> {
		let nominator = Runtime::AddressMapping::into_account_id(nominator.0);

		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let mut nominator_state = NominatorState::<Runtime>::default();

		if let Some(state) = <StakingOf<Runtime>>::nominator_state(&nominator) {
			nominator_state.set_state(state);
		};

		Ok(nominator_state.from_owner(Address(nominator.into())))
	}

	/// Returns the request state of the given `nominator`
	/// @param: `nominator` the address for which to verify
	/// @return: the request state of the given `nominator`
	#[precompile::public("nominatorRequests(address)")]
	#[precompile::public("nominator_requests(address)")]
	#[precompile::view]
	fn nominator_requests(
		handle: &mut impl PrecompileHandle,
		nominator: Address,
	) -> EvmResult<EvmNominatorRequestsOf> {
		let nominator = Runtime::AddressMapping::into_account_id(nominator.0);

		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let zero = 0u32;
		let mut revocations_count: u32 = zero.into();
		let mut less_total: U256 = zero.into();
		let mut candidates: Vec<Address> = vec![];
		let mut amounts: Vec<U256> = vec![];
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

		Ok((
			Address(nominator.into()),
			revocations_count,
			less_total,
			candidates,
			amounts.into(),
			when_executables,
			actions,
		))
	}

	/// Returns the count of nominations of the given `nominator`
	/// @param: `nominator` the address for which to verify
	/// @return: the count of nominations of the given `nominator`
	#[precompile::public("nominatorNominationCount(address)")]
	#[precompile::public("nominator_nomination_count(address)")]
	#[precompile::view]
	fn nominator_nomination_count(
		handle: &mut impl PrecompileHandle,
		nominator: Address,
	) -> EvmResult<u32> {
		let nominator = Runtime::AddressMapping::into_account_id(nominator.0);

		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let result = if let Some(state) = <StakingOf<Runtime>>::nominator_state(&nominator) {
			let nominator_nomination_count: u32 = state.nominations.len() as u32;
			nominator_nomination_count
		} else {
			0u32
		};

		Ok(result)
	}

	// Common dispatchable methods

	#[precompile::public("goOffline()")]
	#[precompile::public("go_offline()")]
	fn go_offline(handle: &mut impl PrecompileHandle) -> EvmResult {
		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = StakingCall::<Runtime>::go_offline {};

		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}

	#[precompile::public("goOnline()")]
	#[precompile::public("go_online()")]
	fn go_online(handle: &mut impl PrecompileHandle) -> EvmResult {
		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = StakingCall::<Runtime>::go_online {};

		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}

	// Validator dispatchable methods

	#[precompile::public("joinCandidates(address,address,uint256,uint256)")]
	#[precompile::public("join_candidates(address,address,uint256,uint256)")]
	fn join_candidates(
		handle: &mut impl PrecompileHandle,
		controller: Address,
		relayer: Address,
		bond: U256,
		candidate_count: u32,
	) -> EvmResult {
		let bond = Self::u256_to_amount(bond).in_field("bond")?;
		let zero_address = Address(Default::default());

		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = {
			let controller = Runtime::AddressMapping::into_account_id(controller.into());
			if relayer != zero_address {
				StakingCall::<Runtime>::join_candidates {
					controller,
					relayer: Some(Runtime::AddressMapping::into_account_id(relayer.into())),
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

		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}

	#[precompile::public("candidateBondMore(uint256)")]
	#[precompile::public("candidate_bond_more(uint256)")]
	fn candidate_bond_more(handle: &mut impl PrecompileHandle, more: U256) -> EvmResult {
		let more = Self::u256_to_amount(more).in_field("more")?;

		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = StakingCall::<Runtime>::candidate_bond_more { more };

		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}

	#[precompile::public("scheduleLeaveCandidates(uint256)")]
	#[precompile::public("schedule_leave_candidates(uint256)")]
	fn schedule_leave_candidates(
		handle: &mut impl PrecompileHandle,
		candidate_count: u32,
	) -> EvmResult {
		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = StakingCall::<Runtime>::schedule_leave_candidates { candidate_count };

		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}

	#[precompile::public("scheduleCandidateBondLess(uint256)")]
	#[precompile::public("schedule_candidate_bond_less(uint256)")]
	fn schedule_candidate_bond_less(handle: &mut impl PrecompileHandle, less: U256) -> EvmResult {
		let less = Self::u256_to_amount(less).in_field("less")?;

		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = StakingCall::<Runtime>::schedule_candidate_bond_less { less };

		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}

	#[precompile::public("executeLeaveCandidates(uint256)")]
	#[precompile::public("execute_leave_candidates(uint256)")]
	fn execute_leave_candidates(
		handle: &mut impl PrecompileHandle,
		candidate_nomination_count: u32,
	) -> EvmResult {
		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = StakingCall::<Runtime>::execute_leave_candidates { candidate_nomination_count };

		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}

	#[precompile::public("executeCandidateBondLess()")]
	#[precompile::public("execute_candidate_bond_less()")]
	fn execute_candidate_bond_less(handle: &mut impl PrecompileHandle) -> EvmResult {
		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = StakingCall::<Runtime>::execute_candidate_bond_less {};

		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}

	#[precompile::public("cancelLeaveCandidates(uint256)")]
	#[precompile::public("cancel_leave_candidates(uint256)")]
	fn cancel_leave_candidates(
		handle: &mut impl PrecompileHandle,
		candidate_count: u32,
	) -> EvmResult {
		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = StakingCall::<Runtime>::cancel_leave_candidates { candidate_count };

		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}

	#[precompile::public("cancelCandidateBondLess()")]
	#[precompile::public("cancel_candidate_bond_less()")]
	fn cancel_candidate_bond_less(handle: &mut impl PrecompileHandle) -> EvmResult {
		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = StakingCall::<Runtime>::cancel_candidate_bond_less {};

		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}

	#[precompile::public("setValidatorCommission(uint256)")]
	#[precompile::public("set_validator_commission(uint256)")]
	fn set_validator_commission(handle: &mut impl PrecompileHandle, new: u32) -> EvmResult {
		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call =
			StakingCall::<Runtime>::set_validator_commission { new: Perbill::from_parts(new) };

		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}

	#[precompile::public("setController(address)")]
	#[precompile::public("set_controller(address)")]
	fn set_controller(handle: &mut impl PrecompileHandle, new: Address) -> EvmResult {
		let new = Runtime::AddressMapping::into_account_id(new.0);

		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = StakingCall::<Runtime>::set_controller { new };

		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}

	#[precompile::public("setCandidateRewardDst(uint256)")]
	#[precompile::public("set_candidate_reward_dst(uint256)")]
	fn set_candidate_reward_dst(handle: &mut impl PrecompileHandle, reward_dst: u8) -> EvmResult {
		let new_reward_dst = match reward_dst {
			0 => RewardDestination::Staked,
			1 => RewardDestination::Account,
			_ => return Err(RevertReason::read_out_of_bounds("reward_dst").into()),
		};

		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = StakingCall::<Runtime>::set_candidate_reward_dst { new_reward_dst };

		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}

	// Nominator dispatchable methods

	#[precompile::public("nominate(address,uint256,uint256,uint256)")]
	fn nominate(
		handle: &mut impl PrecompileHandle,
		candidate: Address,
		amount: U256,
		candidate_nomination_count: u32,
		nomination_count: u32,
	) -> EvmResult {
		let amount = Self::u256_to_amount(amount).in_field("amount")?;
		let candidate = Runtime::AddressMapping::into_account_id(candidate.0);

		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = StakingCall::<Runtime>::nominate {
			candidate,
			amount,
			candidate_nomination_count,
			nomination_count,
		};

		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}

	#[precompile::public("nominatorBondMore(address,uint256)")]
	#[precompile::public("nominator_bond_more(address,uint256)")]
	fn nominator_bond_more(
		handle: &mut impl PrecompileHandle,
		candidate: Address,
		more: U256,
	) -> EvmResult {
		let more = Self::u256_to_amount(more).in_field("more")?;
		let candidate = Runtime::AddressMapping::into_account_id(candidate.0);

		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = StakingCall::<Runtime>::nominator_bond_more { candidate, more };

		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}

	#[precompile::public("scheduleLeaveNominators()")]
	#[precompile::public("schedule_leave_nominators()")]
	fn schedule_leave_nominators(handle: &mut impl PrecompileHandle) -> EvmResult {
		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = StakingCall::<Runtime>::schedule_leave_nominators {};

		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}

	#[precompile::public("scheduleRevokeNomination(address)")]
	#[precompile::public("schedule_revoke_nomination(address)")]
	fn schedule_revoke_nomination(
		handle: &mut impl PrecompileHandle,
		candidate: Address,
	) -> EvmResult {
		let candidate = Runtime::AddressMapping::into_account_id(candidate.0);

		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = StakingCall::<Runtime>::schedule_revoke_nomination { validator: candidate };

		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}

	#[precompile::public("scheduleNominatorBondLess(address,uint256)")]
	#[precompile::public("schedule_nominator_bond_less(address,uint256)")]
	fn schedule_nominator_bond_less(
		handle: &mut impl PrecompileHandle,
		candidate: Address,
		less: U256,
	) -> EvmResult {
		let less = Self::u256_to_amount(less).in_field("less")?;
		let candidate = Runtime::AddressMapping::into_account_id(candidate.0);

		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = StakingCall::<Runtime>::schedule_nominator_bond_less { candidate, less };

		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}

	#[precompile::public("executeLeaveNominators(uint256)")]
	#[precompile::public("execute_leave_nominators(uint256)")]
	fn execute_leave_nominators(
		handle: &mut impl PrecompileHandle,
		nomination_count: u32,
	) -> EvmResult {
		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = StakingCall::<Runtime>::execute_leave_nominators { nomination_count };

		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}

	#[precompile::public("executeNominationRequest(address)")]
	#[precompile::public("execute_nomination_request(address)")]
	fn execute_nomination_request(
		handle: &mut impl PrecompileHandle,
		candidate: Address,
	) -> EvmResult {
		let candidate = Runtime::AddressMapping::into_account_id(candidate.0);

		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = StakingCall::<Runtime>::execute_nomination_request { candidate };

		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}

	#[precompile::public("cancelLeaveNominators()")]
	#[precompile::public("cancel_leave_nominators()")]
	fn cancel_leave_nominators(handle: &mut impl PrecompileHandle) -> EvmResult {
		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = StakingCall::<Runtime>::cancel_leave_nominators {};

		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}

	#[precompile::public("cancelNominationRequest(address)")]
	#[precompile::public("cancel_nomination_request(address)")]
	fn cancel_nomination_request(
		handle: &mut impl PrecompileHandle,
		candidate: Address,
	) -> EvmResult {
		let candidate = Runtime::AddressMapping::into_account_id(candidate.0);

		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = StakingCall::<Runtime>::cancel_nomination_request { candidate };

		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}

	#[precompile::public("setNominatorRewardDst(uint256)")]
	#[precompile::public("set_nominator_reward_dst(uint256)")]
	fn set_nominator_reward_dst(handle: &mut impl PrecompileHandle, reward_dst: u8) -> EvmResult {
		let new_reward_dst = match reward_dst {
			0 => RewardDestination::Staked,
			1 => RewardDestination::Account,
			_ => return Err(RevertReason::read_out_of_bounds("reward_dst").into()),
		};

		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = StakingCall::<Runtime>::set_nominator_reward_dst { new_reward_dst };

		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}

	// Util methods

	fn get_unique_candidates(candidates: &Vec<Address>) -> EvmResult<BTreeSet<Runtime::AccountId>> {
		let unique_candidates: BTreeSet<Runtime::AccountId> = candidates
			.iter()
			.map(|address| Runtime::AddressMapping::into_account_id(address.0))
			.collect();
		if unique_candidates.len() != candidates.len() {
			return Err(RevertReason::custom("Duplicate candidate address received").into());
		}

		Ok(unique_candidates)
	}

	fn get_previous_selected_candidates(
		round_index: &RoundIndex,
	) -> EvmResult<BoundedBTreeSet<Runtime::AccountId, ConstU32<MAX_AUTHORITIES>>> {
		let previous_selected_candidates = <StakingOf<Runtime>>::cached_selected_candidates();
		if let Some(previous_selected_candidates) = previous_selected_candidates.get(round_index) {
			Ok(previous_selected_candidates.clone())
		} else {
			Err(RevertReason::read_out_of_bounds("round_index").into())
		}
	}

	fn compare_selected_candidates(
		candidates: BTreeSet<Runtime::AccountId>,
		tier: TierType,
		is_complete: bool,
	) -> bool {
		if candidates.is_empty() {
			return false;
		}

		let selected_candidates: BoundedBTreeSet<Runtime::AccountId, ConstU32<MAX_AUTHORITIES>> =
			match tier {
				TierType::Full => StakingOf::<Runtime>::selected_full_candidates(),
				TierType::Basic => StakingOf::<Runtime>::selected_basic_candidates(),
				TierType::All => StakingOf::<Runtime>::selected_candidates(),
			};

		return if is_complete {
			if selected_candidates.len() != candidates.len() {
				return false;
			}
			candidates.iter().all(|candidate| selected_candidates.contains(candidate))
		} else {
			candidates.iter().all(|candidate| selected_candidates.contains(candidate))
		};
	}

	fn u256_array_to_amount_array(values: Vec<U256>) -> MayRevert<Vec<BalanceOf<Runtime>>> {
		let mut amounts = vec![];
		for value in values {
			let amount = Self::u256_to_amount(value)?;
			amounts.push(amount);
		}
		Ok(amounts)
	}

	fn u256_to_amount(value: U256) -> MayRevert<BalanceOf<Runtime>> {
		value
			.try_into()
			.map_err(|_| RevertReason::value_is_too_large("balance type").into())
	}
}
