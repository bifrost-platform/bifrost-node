use super::pallet::*;

use crate::{
	inflation::Range, weights::WeightInfo, BalanceOf, Bond, DelayedCommissionSet,
	DelayedControllerSet, DelayedPayout, ProductivityStatus, RewardDestination, RoundIndex,
	TierType, TotalSnapshot, ValidatorSnapshot, ValidatorSnapshotOf,
};

use pallet_session::ShouldEndSession;

use bp_staking::{
	traits::{OffenceHandler, RelayManager},
	Offence,
};
use sp_runtime::{
	traits::{Convert, Saturating, Zero},
	Perbill, Permill,
};
use sp_staking::{
	offence::{DisableStrategy, OffenceDetails, OnOffenceHandler},
	SessionIndex,
};
use sp_std::{vec, vec::Vec};

use frame_support::{
	pallet_prelude::*,
	traits::{Currency, EstimateNextSessionRotation, Get, Imbalance, ReservableCurrency},
	weights::Weight,
};

impl<T: Config> Pallet<T> {
	/// Verifies if the given account is a nominator
	pub fn is_nominator(acc: &T::AccountId) -> bool {
		<NominatorState<T>>::get(acc).is_some()
	}

	/// Verifies if the given account is a candidate
	pub fn is_candidate(acc: &T::AccountId, tier: TierType) -> bool {
		let mut is_candidate = false;
		if let Some(state) = <CandidateInfo<T>>::get(acc) {
			is_candidate = match tier {
				TierType::Full | TierType::Basic => state.tier == tier,
				TierType::All => true,
			};
		}
		is_candidate
	}

	/// Verifies if the given account is a selected candidate for the current round
	pub fn is_selected_candidate(acc: &T::AccountId, tier: TierType) -> bool {
		let mut is_selected_candidate = false;
		match <SelectedCandidates<T>>::get().binary_search(acc) {
			Ok(_) => {
				is_selected_candidate = Self::is_candidate(acc, tier);
			},
			Err(_) => (),
		};
		is_selected_candidate
	}

	/// Verifies if the given account has already requested for controller account update
	pub fn is_controller_set_requested(controller: T::AccountId) -> bool {
		let round = <Round<T>>::get();
		let controller_sets = <DelayedControllerSets<T>>::get(round.current_round_index);
		if controller_sets.is_empty() {
			return false
		}
		return controller_sets.into_iter().any(|c| c.old == controller)
	}

	/// Verifies if the given account has already requested for commission rate update
	pub fn is_commission_set_requested(who: &T::AccountId) -> bool {
		let round = <Round<T>>::get();
		let commission_sets = <DelayedCommissionSets<T>>::get(round.current_round_index);
		if commission_sets.is_empty() {
			return false
		}
		return commission_sets.into_iter().any(|c| c.who == *who)
	}

	/// Adds a new controller set request. The state reflection will be applied in the next round.
	pub fn add_to_controller_sets(stash: T::AccountId, old: T::AccountId, new: T::AccountId) {
		let round = <Round<T>>::get();
		let mut controller_sets = <DelayedControllerSets<T>>::get(round.current_round_index);
		controller_sets
			.try_push(DelayedControllerSet::new(stash, old, new))
			.expect("DelayedControllerSets out of bound");
		<DelayedControllerSets<T>>::insert(round.current_round_index, controller_sets);
	}

	/// Adds a new commission set request. The state reflection will be applied in the next round.
	pub fn add_to_commission_sets(who: &T::AccountId, old: Perbill, new: Perbill) {
		let round = <Round<T>>::get();
		let mut commission_sets = <DelayedCommissionSets<T>>::get(round.current_round_index);
		commission_sets
			.try_push(DelayedCommissionSet::new(who.clone(), old, new))
			.expect("DelayedCommissionSets out of bound");
		<DelayedCommissionSets<T>>::insert(round.current_round_index, commission_sets);
	}

	/// Remove the given `who` from the `DelayedControllerSets` of the current round.
	pub fn remove_controller_set(who: &T::AccountId) {
		let round = <Round<T>>::get();
		let mut controller_sets =
			<DelayedControllerSets<T>>::get(round.current_round_index).into_inner();
		controller_sets = controller_sets.into_iter().filter(|c| c.old != *who).collect();
		<DelayedControllerSets<T>>::insert(
			round.current_round_index,
			BoundedVec::try_from(controller_sets).expect("DelayedControllerSets out of bound"),
		);
	}

	/// Remove the given `who` from the `DelayedCommissionSets` of the current round.
	pub fn remove_commission_set(who: &T::AccountId) {
		let round = <Round<T>>::get();
		let mut commission_sets =
			<DelayedCommissionSets<T>>::get(round.current_round_index).into_inner();
		commission_sets = commission_sets.into_iter().filter(|c| c.who != *who).collect();
		<DelayedCommissionSets<T>>::insert(
			round.current_round_index,
			BoundedVec::try_from(commission_sets).expect("DelayedCommissionSets out of bound"),
		);
	}

	/// Updates the given candidates voting power persisted in the `CandidatePool`
	pub(crate) fn update_active(candidate: &T::AccountId, total: BalanceOf<T>) {
		let mut pool = <CandidatePool<T>>::get().into_inner();
		pool = pool
			.iter()
			.map(|c| {
				if &c.owner == candidate {
					Bond { owner: candidate.clone(), amount: total }
				} else {
					c.clone()
				}
			})
			.collect();
		<CandidatePool<T>>::put(BoundedVec::try_from(pool).expect("CandidatePool out of bound"));
	}

	/// Sort `CandidatePool` candidates by voting power in descending order
	pub fn sort_candidates_by_voting_power() {
		let mut pool = <CandidatePool<T>>::get().into_inner();
		pool.sort_by(|x, y| y.amount.cmp(&x.amount));
		pool.dedup_by(|x, y| x.owner == y.owner);
		<CandidatePool<T>>::put(BoundedVec::try_from(pool).expect("CandidatePool out of bound"));
	}

	/// Removes the given `candidate` from the `CandidatePool`. Returns `true` if a candidate has
	/// been removed.
	pub fn remove_from_candidate_pool(candidate: &T::AccountId) -> bool {
		let mut pool = <CandidatePool<T>>::get();
		let prev_len = pool.len();
		pool.retain(|c| c.owner != *candidate);
		let curr_len = pool.len();
		<CandidatePool<T>>::put(pool);
		curr_len < prev_len
	}

	/// Replace the bonded `old` account to the given `new` account from the `CandidatePool`
	pub fn replace_from_candidate_pool(old: &T::AccountId, new: &T::AccountId) {
		let mut pool = <CandidatePool<T>>::get().into_inner();
		pool = pool
			.iter()
			.map(|c| {
				if &c.owner == old {
					Bond { owner: new.clone(), amount: c.amount }
				} else {
					c.clone()
				}
			})
			.collect();
		<CandidatePool<T>>::put(BoundedVec::try_from(pool).expect("CandidatePool out of bound"));
	}

	/// Adds the given `candidate` to the `SelectedCandidates`. Depends on the given `tier` whether
	/// it's added to the `SelectedFullCandidates` or `SelectedBasicCandidates`.
	fn add_to_selected_candidates(candidate: T::AccountId, tier: TierType) {
		let mut selected_candidates = <SelectedCandidates<T>>::get();
		selected_candidates
			.try_push(candidate.clone())
			.expect("SelectedCandidates out of bound");
		<SelectedCandidates<T>>::put(selected_candidates);
		match tier {
			TierType::Full => {
				let mut selected_full_candidates = <SelectedFullCandidates<T>>::get();
				selected_full_candidates
					.try_push(candidate.clone())
					.expect("SelectedFullCandidates out of bound");
				<SelectedFullCandidates<T>>::put(selected_full_candidates);
			},
			_ => {
				let mut selected_basic_candidates = <SelectedBasicCandidates<T>>::get();
				selected_basic_candidates
					.try_push(candidate.clone())
					.expect("SelectedBasicCandidates out of bound");
				<SelectedBasicCandidates<T>>::put(selected_basic_candidates);
			},
		};
	}

	/// Removes the given `candidate` from the `SelectedCandidates`. Depends on the given `tier`
	/// whether it's removed from the `SelectedFullCandidates` or `SelectedBasicCandidates`.
	fn remove_from_selected_candidates(candidate: &T::AccountId, tier: TierType) {
		let mut selected_candidates = <SelectedCandidates<T>>::get();
		selected_candidates.retain(|c| c != candidate);
		<SelectedCandidates<T>>::put(selected_candidates);
		match tier {
			TierType::Full => {
				let mut selected_full_candidates = <SelectedFullCandidates<T>>::get();
				selected_full_candidates.retain(|c| c != candidate);
				<SelectedFullCandidates<T>>::put(selected_full_candidates);
			},
			_ => {
				let mut selected_basic_candidates = <SelectedBasicCandidates<T>>::get();
				selected_basic_candidates.retain(|c| c != candidate);
				<SelectedBasicCandidates<T>>::put(selected_basic_candidates);
			},
		};
	}

	/// Replaces the bonded `old` candidate to the given `new` from the `SelectedCandidates`.
	fn replace_from_selected_candidates(old: &T::AccountId, new: &T::AccountId, tier: TierType) {
		Self::remove_from_selected_candidates(old, tier);
		Self::add_to_selected_candidates(new.clone(), tier);
		Self::refresh_latest_cached_selected_candidates();
	}

	/// Compute round issuance based on the total amount of stake of the current round
	pub fn compute_issuance(staked: BalanceOf<T>) -> BalanceOf<T> {
		let config = <InflationConfig<T>>::get();
		let round_issuance = Range {
			min: config.round.min * staked,
			ideal: config.round.ideal * staked,
			max: config.round.max * staked,
		};
		if staked <= config.expect.min {
			round_issuance.max
		} else if staked >= config.expect.max {
			round_issuance.min
		} else {
			round_issuance.ideal
		}
	}

	/// Compute the majority of the selected candidates
	pub fn compute_majority() -> u32 {
		let selected_candidates = <SelectedCandidates<T>>::get();
		let half = (selected_candidates.len() as u32) / 2;
		return half + 1
	}

	/// Remove nomination from candidate state
	/// Amount input should be retrieved from nominator and it informs the storage lookups
	pub fn nominator_leaves_candidate(
		candidate: T::AccountId,
		nominator: T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult {
		let mut state = <CandidateInfo<T>>::get(&candidate).ok_or(Error::<T>::CandidateDNE)?;
		state.rm_nomination_if_exists::<T>(&candidate, nominator.clone(), amount)?;
		T::Currency::unreserve(&nominator, amount);
		let new_total_locked = <Total<T>>::get().saturating_sub(amount);
		<Total<T>>::put(new_total_locked);
		let new_total = state.voting_power;
		<CandidateInfo<T>>::insert(&candidate, state);
		Self::deposit_event(Event::NominatorLeftCandidate {
			nominator,
			candidate,
			unstaked_amount: amount,
			total_candidate_staked: new_total,
		});
		Ok(())
	}

	/// Generates a delayed payout for staking rewards
	pub fn prepare_staking_payouts(now: RoundIndex) {
		// payout is now - delay rounds ago => now - delay > 0 else return early
		let delay = T::RewardPaymentDelay::get();
		if now <= delay {
			return
		}
		let round_to_payout = now - delay;
		let total_points = <Points<T>>::get(round_to_payout);
		if total_points.is_zero() {
			return
		}
		// total staked amount for the given round
		let total_staked = <Staked<T>>::take(round_to_payout);

		// total issuance for the given round
		let round_issuance = Self::compute_issuance(total_staked);

		let payout = DelayedPayout {
			round_issuance,
			total_staking_reward: round_issuance,
			validator_commission: <DefaultBasicValidatorCommission<T>>::get(),
		};
		<DelayedPayouts<T>>::insert(round_to_payout, payout);
	}

	/// Handle validators auto-compoundable round rewards
	/// If the reward destination is set to `Staked`, it will be auto-compounded
	pub fn handle_validator_auto_compounding(
		validator: T::AccountId,
		reward: BalanceOf<T>,
	) -> Result<(), DispatchError> {
		if let Some(mut validator_state) = <CandidateInfo<T>>::get(&validator) {
			// increment the awarded tokens of this validator
			validator_state.increment_awarded_tokens(reward);

			if validator_state.reward_dst == RewardDestination::Staked {
				validator_state.bond_more::<T>(
					validator_state.stash.clone(),
					validator.clone(),
					reward,
				)?;
				Self::update_active(&validator, validator_state.voting_power);
				Self::sort_candidates_by_voting_power();
			}
			<CandidateInfo<T>>::insert(&validator, validator_state);
		}
		Ok(().into())
	}

	/// Handle nominators auto-compoundable round rewards
	/// If the reward destination is set to `Staked`, it will be auto-compounded
	pub fn handle_nominator_auto_compounding(
		validator: T::AccountId,
		nominator: T::AccountId,
		reward: BalanceOf<T>,
	) -> Result<(), DispatchError> {
		if let Some(mut nominator_state) = <NominatorState<T>>::get(&nominator) {
			match nominator_state.reward_dst {
				RewardDestination::Staked => {
					// increment the awarded tokens of this nominator
					nominator_state.increment_awarded_tokens(&validator, reward);
					// auto-compound nomination
					if nominator_state.increase_nomination::<T>(validator.clone(), reward).is_ok() {
						<NominatorState<T>>::insert(&nominator, nominator_state);
						Self::sort_candidates_by_voting_power();
					}
				},
				RewardDestination::Account => {
					// increment the awarded tokens of this nominator
					nominator_state.increment_awarded_tokens(&validator, reward);
					<NominatorState<T>>::insert(&nominator, nominator_state);
				},
			}
		}
		Ok(().into())
	}

	/// Wrapper around pay_one_validator_reward which handles the following logic:
	/// * whether or not a payout needs to be made
	/// * cleaning up when payouts are done
	/// * returns the weight consumed by pay_one_validator_reward if applicable
	/// * runs at every block
	pub fn handle_delayed_payouts(now: RoundIndex) -> Weight {
		// now: current round index
		let delay = T::RewardPaymentDelay::get();

		// don't underflow uint
		if now < delay {
			return Weight::from_ref_time(0u64)
		}
		let round_to_payout = now - delay;

		if let Some(payout_info) = <DelayedPayouts<T>>::get(round_to_payout) {
			let result = Self::pay_one_validator_reward(round_to_payout, payout_info);
			if result.0.is_none() {
				// result.0 indicates whether or not a payout was made
				// clean up storage items that we no longer need
				<DelayedPayouts<T>>::remove(round_to_payout);
				<Points<T>>::remove(round_to_payout);
			}
			// weight consumed by pay_one_validator_reward
			result.1
		} else {
			return Weight::from_ref_time(0u64)
		}
	}

	/// Replace each nominators nominated candidate's account from `old` to `new`. This method will
	/// also replace the pending requests.
	fn replace_nominator_nominations(
		nominators: &Vec<T::AccountId>,
		old: &T::AccountId,
		new: &T::AccountId,
	) {
		nominators.into_iter().for_each(|n| {
			if let Some(mut nominator) = <NominatorState<T>>::get(n) {
				nominator.replace_nominations(old, new);
				nominator.replace_requests(old, new);
				<NominatorState<T>>::insert(n, nominator);
			}
		});
	}

	pub fn handle_delayed_commission_sets(now: RoundIndex) {
		let delayed_round = now - 1;
		let commission_sets = <DelayedCommissionSets<T>>::take(delayed_round);
		commission_sets.into_iter().for_each(|c| {
			if let Some(mut candidate) = <CandidateInfo<T>>::get(&c.who) {
				candidate.set_commission(c.new);
				<CandidateInfo<T>>::insert(&c.who, candidate);
			}
		});
	}

	/// Apply the delayed controller set requests. Replaces the entire bonded storage values from
	/// the old to new.
	pub fn handle_delayed_controller_sets(now: RoundIndex) {
		let delayed_round = now - 1;
		let controller_sets = <DelayedControllerSets<T>>::take(delayed_round);
		if !controller_sets.is_empty() {
			controller_sets.into_iter().for_each(|c| {
				if let Some(candidate) = <CandidateInfo<T>>::get(&c.old) {
					// replace `CandidateInfo`
					<CandidateInfo<T>>::remove(&c.old);
					<CandidateInfo<T>>::insert(&c.new, candidate.clone());
					// replace `BondedStash`
					<BondedStash<T>>::insert(&c.stash, c.new.clone());
					// replace `CandidatePool`
					Self::replace_from_candidate_pool(&c.old, &c.new);
					// replace `SelectedCandidates`
					if candidate.is_selected {
						Self::replace_from_selected_candidates(&c.old, &c.new, candidate.tier);
						T::RelayManager::replace_bonded_controller(c.old.clone(), c.new.clone());
					}
					// replace `TopNominations`
					if let Some(top_nominations) = <TopNominations<T>>::take(&c.old) {
						Self::replace_nominator_nominations(
							&top_nominations.clone().nominators(),
							&c.old,
							&c.new,
						);
						<TopNominations<T>>::insert(&c.new, top_nominations);
					}
					// replace `BottomNominations`
					if let Some(bottom_nominations) = <BottomNominations<T>>::get(&c.old) {
						Self::replace_nominator_nominations(
							&bottom_nominations.clone().nominators(),
							&c.old,
							&c.new,
						);
						<BottomNominations<T>>::insert(&c.new, bottom_nominations);
					}
					// replace `AwardedPts`
					let points = <AwardedPts<T>>::take(now, &c.old);
					<AwardedPts<T>>::insert(now, &c.new, points);
					// replace `AtStake`
					let at_stake = <AtStake<T>>::take(now, &c.old);
					<AtStake<T>>::insert(now, &c.new, at_stake);
				}
			});
		}
	}

	/// Payout a single validator from the given round.
	///
	/// Returns an optional tuple of (Validator's AccountId, total paid)
	/// or None if there were no more payouts to be made for the round.
	pub(crate) fn pay_one_validator_reward(
		round_to_payout: RoundIndex,
		payout_info: DelayedPayout<BalanceOf<T>>,
	) -> (Option<(T::AccountId, BalanceOf<T>)>, Weight) {
		let total_points = <Points<T>>::get(round_to_payout);
		if total_points.is_zero() {
			return (None, Weight::from_ref_time(0u64))
		}
		// reward minter
		let mint = |amt: BalanceOf<T>, to: T::AccountId| {
			if let Ok(amount_transferred) = T::Currency::deposit_into_existing(&to, amt) {
				let mut awarded_tokens = <AwardedTokens<T>>::get();
				awarded_tokens += amount_transferred.peek();
				<AwardedTokens<T>>::put(awarded_tokens);
				Self::deposit_event(Event::Rewarded {
					account: to.clone(),
					rewards: amount_transferred.peek(),
				});
			}
		};

		if let Some((validator, pts)) = <AwardedPts<T>>::iter_prefix(round_to_payout).drain().next()
		{
			if let Some(state) = <CandidateInfo<T>>::get(&validator) {
				let validator_issuance = state.commission * payout_info.round_issuance;

				// compute contribution percentage from given round total points
				let validator_contribution_pct = Perbill::from_rational(pts, total_points);
				// total reward amount for this validator and nominators
				let total_reward_amount =
					validator_contribution_pct * payout_info.total_staking_reward;

				// Take the snapshot of block author and nominations
				let snapshot = <AtStake<T>>::take(round_to_payout, &validator);
				let num_nominators = snapshot.nominations.len();

				if snapshot.nominations.is_empty() {
					// solo validator with no nominators
					mint(total_reward_amount, state.stash.clone());
					// handle auto-compounding
					Self::handle_validator_auto_compounding(validator.clone(), total_reward_amount)
						.expect("Graceful validator reward auto-compound");
				} else {
					// pay validator first; commission + due_portion
					let validator_stake_pct = Perbill::from_rational(snapshot.bond, snapshot.total);
					let commission = validator_contribution_pct * validator_issuance;
					let amount_due = total_reward_amount - commission;
					let validator_reward = (validator_stake_pct * amount_due) + commission;

					mint(validator_reward, state.stash.clone());
					Self::handle_validator_auto_compounding(validator.clone(), validator_reward)
						.expect("Graceful validator reward auto-compound");
					// pay nominators due portion
					for Bond { owner, amount } in snapshot.nominations {
						let nominator_stake_pct = Perbill::from_rational(amount, snapshot.total);
						let nominator_reward = nominator_stake_pct * amount_due;
						if !nominator_reward.is_zero() {
							mint(nominator_reward, owner.clone());
							Self::handle_nominator_auto_compounding(
								validator.clone(),
								owner.clone(),
								nominator_reward,
							)
							.expect("Graceful nominator reward auto-compound");
						}
					}
				}

				(
					Some((validator, total_reward_amount)),
					T::WeightInfo::pay_one_validator_reward(num_nominators as u32),
				)
			} else {
				(None, Weight::from_ref_time(0u64))
			}
		} else {
			// Note that we don't clean up storage here; it is cleaned up in
			// handle_delayed_payouts()
			(None, Weight::from_ref_time(0u64))
		}
	}

	/// Compute the top full and basic candidates in the CandidatePool and return
	/// a vector of their AccountIds (in the order of selection)
	pub fn compute_top_candidates() -> (Vec<T::AccountId>, Vec<T::AccountId>) {
		let candidates = <CandidatePool<T>>::get();
		let mut full_candidates = vec![];
		let mut basic_candidates = vec![];

		for candidate in candidates {
			if let Some(state) = <CandidateInfo<T>>::get(&candidate.owner) {
				let bond = Bond { owner: candidate.owner.clone(), amount: state.voting_power };
				match state.tier {
					TierType::Full =>
						if state.bond >= T::MinFullCandidateStk::get() {
							full_candidates.push(bond);
						},
					_ =>
						if state.bond >= T::MinBasicCandidateStk::get() {
							basic_candidates.push(bond);
						},
				}
			}
		}

		let full_validators = Self::compute_top_full_candidates(full_candidates);
		let basic_validators = Self::compute_top_basic_candidates(basic_candidates);

		let length = (full_validators.len() as u32) + (basic_validators.len() as u32);

		if <MinTotalSelected<T>>::get() > length {
			(vec![], vec![])
		} else {
			(full_validators, basic_validators)
		}
	}

	/// Compute the top full candidates based on their voting power
	pub fn compute_top_full_candidates(
		mut candidates: Vec<Bond<T::AccountId, BalanceOf<T>>>,
	) -> Vec<T::AccountId> {
		// order full candidates by voting power (least to greatest so requires `rev()`)
		candidates.sort_by(|a, b| a.amount.cmp(&b.amount));
		let top_n = <MaxFullSelected<T>>::get() as usize;

		// choose the top MaxFullSelected qualified candidates, ordered by voting power
		let mut validators = candidates
			.into_iter()
			.rev()
			.filter(|x| x.amount >= T::MinFullValidatorStk::get())
			.take(top_n)
			.map(|x| x.owner)
			.collect::<Vec<T::AccountId>>();
		validators.sort();
		validators
	}

	/// Compute the top basic candidates based on their voting power
	pub fn compute_top_basic_candidates(
		mut candidates: Vec<Bond<T::AccountId, BalanceOf<T>>>,
	) -> Vec<T::AccountId> {
		// order candidates by voting power (least to greatest so requires `rev()`)
		candidates.sort_by(|a, b| a.amount.cmp(&b.amount));
		let top_n = <MaxBasicSelected<T>>::get() as usize;

		// choose the top MaxBasicSelected qualified candidates, ordered by voting power
		let mut validators = candidates
			.into_iter()
			.rev()
			.filter(|x| x.amount >= T::MinBasicValidatorStk::get())
			.take(top_n)
			.map(|x| x.owner)
			.collect::<Vec<T::AccountId>>();
		validators.sort();
		validators
	}

	/// Take the snapshot of the given validators
	pub fn collect_validator_snapshot(
		now: RoundIndex,
		validators: Vec<T::AccountId>,
	) -> (u32, u32, BalanceOf<T>) {
		let (mut validator_count, mut nomination_count, mut total) =
			(0u32, 0u32, BalanceOf::<T>::zero());
		// snapshot exposure for round for weighting reward distribution
		for validator in validators.iter() {
			let mut state = <CandidateInfo<T>>::get(validator)
				.expect("all members of CandidateQ must be candidates");
			let top_nominations = <TopNominations<T>>::get(validator)
				.expect("all members of CandidateQ must be candidates");
			let bottom_nominations = BottomNominations::<T>::get(validator)
				.expect("all members of CandidateQ must be candidates");

			// Nominators for each validator
			let mut nominators = vec![];
			nominators.append(&mut top_nominations.nominations.clone());
			nominators.append(&mut bottom_nominations.nominations.clone());

			validator_count += 1u32;
			nomination_count += state.nomination_count;
			total += state.voting_power;

			let snapshot_total = state.voting_power;
			let snapshot = ValidatorSnapshot {
				bond: state.bond,
				nominations: top_nominations.nominations,
				total: state.voting_power,
			};
			<AtStake<T>>::insert(now, validator, snapshot);
			state.set_is_selected(true);
			<CandidateInfo<T>>::insert(&validator, state);

			Self::deposit_event(Event::ValidatorChosen {
				round: now,
				validator_account: validator.clone(),
				total_exposed_amount: snapshot_total,
			});
		}
		(validator_count, nomination_count, total)
	}

	/// Best as in most cumulatively supported in terms of stake
	/// Returns [validator_count, nomination_count, total staked]
	pub fn update_top_candidates(
		now: RoundIndex,
		full_validators: Vec<T::AccountId>,
		basic_validators: Vec<T::AccountId>,
	) -> (u32, u32, BalanceOf<T>) {
		let (mut validator_count, mut nomination_count, mut total) =
			(0u32, 0u32, BalanceOf::<T>::zero());
		// choose the top qualified full and basic candidates, ordered by their voting power
		let full_snapshot = Self::collect_validator_snapshot(now, full_validators.clone());
		let basic_snapshot = Self::collect_validator_snapshot(now, basic_validators.clone());

		validator_count += full_snapshot.0 + basic_snapshot.0;
		nomination_count += full_snapshot.1 + basic_snapshot.1;
		total += full_snapshot.2 + basic_snapshot.2;

		let mut validators = [full_validators.clone(), basic_validators.clone()].concat();
		validators.sort();

		// reset active validator set
		<SelectedCandidates<T>>::put(
			BoundedVec::try_from(validators.clone()).expect("SelectedCandidates out of bound"),
		);
		<SelectedFullCandidates<T>>::put(
			BoundedVec::try_from(full_validators.clone())
				.expect("SelectedFullCandidates out of bound"),
		);
		<SelectedBasicCandidates<T>>::put(
			BoundedVec::try_from(basic_validators.clone())
				.expect("SelectedBasicCandidates out of bound"),
		);
		Self::refresh_cached_selected_candidates(now, validators.clone());

		// refresh active relayer set
		T::RelayManager::refresh_selected_relayers(now, full_validators.clone());

		// active validators count
		// total nominators count (top + bottom) of active validators
		// active stake of active validators
		(validator_count, nomination_count, total)
	}

	/// Updates the block productivity and increases block points of the block author
	pub(crate) fn note_author(author: &T::AccountId) {
		let round = <Round<T>>::get();
		let round_index = round.current_round_index;
		let current_block = round.current_block;

		if let Some(mut state) = <CandidateInfo<T>>::get(author) {
			// rounds current block increases after block authoring
			state.set_last_block(current_block + T::BlockNumber::from(1u32));
			state.increment_blocks_produced();
			<CandidateInfo<T>>::insert(author, state);
		}

		let score_plus_5 = <AwardedPts<T>>::get(round_index, &author) + 5;
		<AwardedPts<T>>::insert(round_index, author, score_plus_5);
		<Points<T>>::mutate(round_index, |x| *x += 5);
	}

	/// Reset every `per round` related parameters of every candidates
	pub fn reset_candidate_states() {
		for candidate in <CandidateInfo<T>>::iter() {
			let owner = candidate.0;
			let mut state = candidate.1;
			state.reset_blocks_produced();
			state.reset_productivity();
			state.set_is_selected(false);
			<CandidateInfo<T>>::insert(&owner, state);
		}
	}

	/// Refresh the `CachedSelectedCandidates` adding the new selected candidates
	pub fn refresh_cached_selected_candidates(now: RoundIndex, validators: Vec<T::AccountId>) {
		let mut cached_selected_candidates = <CachedSelectedCandidates<T>>::get();
		if <StorageCacheLifetime<T>>::get() <= cached_selected_candidates.len() as u32 {
			cached_selected_candidates.remove(0);
		}
		cached_selected_candidates.push((now, validators));
		<CachedSelectedCandidates<T>>::put(cached_selected_candidates);
	}

	/// Refresh the latest rounds cached selected candidates to the current state
	fn refresh_latest_cached_selected_candidates() {
		let round = <Round<T>>::get();
		let selected_candidates = <SelectedCandidates<T>>::get();
		let mut cached_selected_candidates = <CachedSelectedCandidates<T>>::get();
		cached_selected_candidates.retain(|r| r.0 != round.current_round_index);
		cached_selected_candidates
			.push((round.current_round_index, selected_candidates.into_inner()));
		<CachedSelectedCandidates<T>>::put(cached_selected_candidates);
	}

	/// Refresh the latest rounds cached majority to the current state
	fn refresh_latest_cached_majority() {
		let round = <Round<T>>::get();
		let majority = <Majority<T>>::get();
		let mut cached_majority = <CachedMajority<T>>::get();
		cached_majority.retain(|r| r.0 != round.current_round_index);
		cached_majority.push((round.current_round_index, majority));
		<CachedMajority<T>>::put(cached_majority);
	}

	/// Refresh the `Majority` and `CachedMajority` based on the new selected candidates
	pub fn refresh_majority(now: RoundIndex) {
		let mut cached_majority = <CachedMajority<T>>::get();
		if <StorageCacheLifetime<T>>::get() <= cached_majority.len() as u32 {
			cached_majority.remove(0);
		}
		let majority: u32 = Self::compute_majority();
		cached_majority.push((now, majority));
		<CachedMajority<T>>::put(cached_majority);
		<Majority<T>>::put(majority);
	}

	/// Compute block productivity of the current validators
	/// - decrease the productivity if the validator produced zero blocks in the current session
	pub fn compute_productivity(session_validators: Vec<T::AccountId>) {
		for validator in session_validators {
			if let Some(mut state) = <CandidateInfo<T>>::get(&validator) {
				if state.productivity_status == ProductivityStatus::Idle {
					state.decrement_productivity::<T>();
				}
				<CandidateInfo<T>>::insert(&validator, state);
			}
		}
	}

	/// Refresh the `ProductivityPerBlock` based on the current round length
	pub fn refresh_productivity_per_block(validator_count: u32, round_length: u32) {
		let blocks_per_validator = {
			if validator_count == 0 {
				0u32
			} else {
				(round_length / validator_count) + 1
			}
		};
		let productivity_per_block = {
			if blocks_per_validator == 0 {
				Perbill::zero()
			} else {
				Perbill::from_percent((100 / blocks_per_validator) + 1)
			}
		};
		<ProductivityPerBlock<T>>::put(productivity_per_block);
	}

	/// Refresh the current staking state of the network of the current round
	pub fn refresh_total_snapshot(now: RoundIndex) {
		let selected_candidates = <SelectedCandidates<T>>::get();
		let mut snapshot: TotalSnapshot<BalanceOf<T>> = TotalSnapshot::default();
		for candidate in <CandidateInfo<T>>::iter() {
			let owner = candidate.0;
			let state = candidate.1;

			let top_nominations =
				<TopNominations<T>>::get(&owner).expect("Candidate must have top nominations");
			let bottom_nominations = <BottomNominations<T>>::get(&owner)
				.expect("Candidate must have bottom nominations");

			if selected_candidates.contains(&owner) {
				snapshot.increment_active_self_bond(state.bond);
				snapshot
					.increment_active_nominations(top_nominations.total + bottom_nominations.total);
				snapshot.increment_active_top_nominations(top_nominations.total);
				snapshot.increment_active_bottom_nominations(bottom_nominations.total);
				snapshot.increment_active_nominators(
					top_nominations.count() + bottom_nominations.count(),
				);
				snapshot.increment_active_top_nominators(top_nominations.count());
				snapshot.increment_active_bottom_nominators(bottom_nominations.count());
				snapshot.increment_active_stake(
					state.bond + top_nominations.total + bottom_nominations.total,
				);
				snapshot.increment_active_voting_power(state.voting_power);
			}

			snapshot.increment_total_self_bond(state.bond);
			snapshot.increment_total_nominations(top_nominations.total + bottom_nominations.total);
			snapshot.increment_total_top_nominations(top_nominations.total);
			snapshot.increment_total_bottom_nominations(bottom_nominations.total);
			snapshot
				.increment_total_nominators(top_nominations.count() + bottom_nominations.count());
			snapshot.increment_total_top_nominators(top_nominations.count());
			snapshot.increment_total_bottom_nominators(bottom_nominations.count());
			snapshot.increment_total_stake(
				state.bond + top_nominations.total + bottom_nominations.total,
			);
			snapshot.increment_total_voting_power(state.voting_power);
		}
		<TotalAtStake<T>>::insert(now, snapshot);
	}

	/// kick out validator from the active validator set
	pub fn kickout_validator(who: &T::AccountId) {
		// remove from candidate pool
		Self::remove_from_candidate_pool(who);
		// update candidate info
		let mut candidate_state = CandidateInfo::<T>::get(who).expect("CandidateInfo must exist");
		candidate_state.kick_out();
		CandidateInfo::<T>::insert(who, &candidate_state);
		// remove from selected candidates
		Self::remove_from_selected_candidates(who, candidate_state.tier);
		// refresh latest cached selected candidates
		Self::refresh_latest_cached_selected_candidates();
		// refresh majority
		let majority: u32 = Self::compute_majority();
		<Majority<T>>::put(majority);
		Self::refresh_latest_cached_majority();
		if candidate_state.tier == TierType::Full {
			// kickout relayer
			T::RelayManager::kickout_relayer(who);
		}
		Pallet::<T>::deposit_event(Event::<T>::KickedOut(who.clone()));
	}

	/// Updates the self-bond related storage of the given validator. This will update it's
	/// self-bond, voting power, total locked stake. It will also cancel the pending request if the
	/// result after the slash creates an integer underflow.
	pub fn slash_reserved_bonds(
		offender: &T::AccountId,
		offender_slash: BalanceOf<T>,
		_nominators_slash: &Vec<(T::AccountId, BalanceOf<T>)>,
	) {
		let mut candidate_state =
			CandidateInfo::<T>::get(offender).expect("CandidateInfo must exist");
		candidate_state.slash_bond(offender_slash);
		candidate_state.slash_voting_power(offender_slash);
		// remove validator bond less request amount to prevent integer underflow
		if let Some(request) = candidate_state.request {
			let mut request_underflow = false;
			// bond less amount exceeds current self bond
			if candidate_state.bond <= request.amount {
				// remove request
				candidate_state.request = None;
				request_underflow = true;
			}
			if !request_underflow {
				let mut minimum_self_bond = T::MinBasicCandidateStk::get();
				if candidate_state.tier == TierType::Full {
					minimum_self_bond = T::MinFullCandidateStk::get();
				}
				// bond less results to insufficient self bond
				if (candidate_state.bond - request.amount) < minimum_self_bond {
					// remove request
					candidate_state.request = None;
				}
			}
		}
		let new_total_locked = <Total<T>>::get().saturating_sub(offender_slash);
		<Total<T>>::put(new_total_locked);
		CandidateInfo::<T>::insert(offender, candidate_state.clone());
	}

	/// Update to the new round. This method will refresh the candidate states and some other
	/// metadata, and will also apply the new top candidates selected for the new round.
	pub fn new_round(
		now: T::BlockNumber,
		full_validators: Vec<T::AccountId>,
		basic_validators: Vec<T::AccountId>,
	) {
		// update round
		let mut round = <Round<T>>::get();
		round.update_round::<T>(now);
		let current_round = round.current_round_index;
		// reset candidate states
		Pallet::<T>::reset_candidate_states();
		// pay all stakers for T::RewardPaymentDelay rounds ago
		Self::prepare_staking_payouts(current_round);
		// select top validator candidates for the next round
		let (validator_count, _, total_staked) =
			Self::update_top_candidates(current_round, full_validators, basic_validators);
		// start next round
		<Round<T>>::put(round);
		// refresh majority
		Self::refresh_majority(current_round);
		T::RelayManager::refresh_majority(current_round);
		// refresh productivity rate per block
		Self::refresh_productivity_per_block(validator_count, round.round_length);
		// snapshot total stake and storage state
		<Staked<T>>::insert(current_round, <Total<T>>::get());
		<TotalAtStake<T>>::remove(current_round - 1);
		// handle delayed set requests
		Self::handle_delayed_controller_sets(current_round);
		Self::handle_delayed_commission_sets(current_round);
		Self::deposit_event(Event::NewRound {
			starting_block: round.first_round_block,
			round: current_round,
			selected_validators_number: validator_count,
			total_balance: total_staked,
		});
	}
}

impl<T> pallet_authorship::EventHandler<T::AccountId, T::BlockNumber> for Pallet<T>
where
	T: Config + pallet_authorship::Config + pallet_session::Config,
	T: pallet_session::Config<ValidatorId = <T as frame_system::Config>::AccountId>,
{
	/// Add reward points to block authors:
	/// * 5 points to the block producer for producing a block in the chain
	fn note_author(author: T::AccountId) {
		Pallet::<T>::note_author(&author);

		if let Some(mut state) = <CandidateInfo<T>>::get(&author) {
			state.productivity_status = ProductivityStatus::Active;
			<CandidateInfo<T>>::insert(&author, state);
		}
	}

	fn note_uncle(_author: T::AccountId, _age: T::BlockNumber) {}
}

impl<T: Config> pallet_session::SessionManager<T::AccountId> for Pallet<T>
where
	T: pallet_session::Config<ValidatorId = <T as frame_system::Config>::AccountId>,
{
	/// 1. A new session starts.
	/// 2. In hook new_session: Read the current top n candidates from the
	///    TopCandidates and assign this set to author blocks for the next
	///    session.
	/// 3. AURA queries the authorities from the session pallet for
	///    this session and picks authors on round-robin-basis from list of
	///    authorities.
	fn new_session(new_index: SessionIndex) -> Option<Vec<T::AccountId>> {
		// `new_session` is called before actual selected candidate update
		let now = <frame_system::Pallet<T>>::block_number();

		// Retrieve the current session validators and update their block productivity
		let session_validators: Vec<T::AccountId> =
			pallet_session::Pallet::<T>::validators().into();
		Self::compute_productivity(session_validators);

		// Update to a new session
		let new_session = new_index - 1;
		Session::<T>::put(new_session);
		let mut round = Pallet::<T>::round();
		round.update_session::<T>(now, new_session);
		Round::<T>::put(round);

		// Check if the round should update
		if round.should_update(now) {
			// Compute the new validators (full, basic) for the new round
			let (mut full_validators, mut basic_validators) = Self::compute_top_candidates();
			// Filter and verify if each validator has an on-chain session key registered
			let session_key_verifier = |validators: Vec<T::AccountId>| {
				validators
					.into_iter()
					.filter(|v| pallet_session::Pallet::<T>::load_keys(v).is_some())
					.collect::<Vec<T::AccountId>>()
			};
			full_validators = session_key_verifier(full_validators);
			basic_validators = session_key_verifier(basic_validators);
			// Update to the new round
			Self::new_round(now, full_validators, basic_validators);
		}

		// Check and refresh if any validator offences has expired
		T::OffenceHandler::refresh_offences(new_index - 1);

		let validators = Self::selected_candidates();
		if validators.is_empty() {
			if new_index <= 1 {
				None
			} else {
				// This would brick the chain in the next session
				log::error!("💥 empty validator set received");
				Some(validators.into_inner())
			}
		} else {
			Some(validators.into_inner())
		}
	}

	fn end_session(_end_index: SessionIndex) {
		T::RelayManager::collect_heartbeats();
	}

	fn start_session(_start_index: SessionIndex) {}
}

impl<T: Config> ShouldEndSession<T::BlockNumber> for Pallet<T> {
	fn should_end_session(now: T::BlockNumber) -> bool {
		let round = <Round<T>>::get();
		// always update when a new round should start
		round.should_update(now)
	}
}

impl<T: Config> EstimateNextSessionRotation<T::BlockNumber> for Pallet<T> {
	fn average_session_length() -> T::BlockNumber {
		let session_period = T::DefaultBlocksPerSession::get();
		T::BlockNumber::from(session_period)
	}

	fn estimate_current_session_progress(now: T::BlockNumber) -> (Option<Permill>, Weight) {
		let session_period = T::DefaultBlocksPerSession::get();
		let passed_blocks = now % T::BlockNumber::from(session_period);
		(
			Some(Permill::from_rational(passed_blocks, T::BlockNumber::from(session_period))),
			// One read for the round info, blocknumber is read free
			T::DbWeight::get().reads(1),
		)
	}

	fn estimate_next_session_rotation(_now: T::BlockNumber) -> (Option<T::BlockNumber>, Weight) {
		let round = <Round<T>>::get();

		(
			Some(round.first_round_block + round.round_length.into()),
			// One read for the round info, blocknumber is read free
			T::DbWeight::get().reads(1),
		)
	}
}

impl<T: Config>
	OnOffenceHandler<T::AccountId, pallet_session::historical::IdentificationTuple<T>, Weight>
	for Pallet<T>
where
	T: pallet_session::Config<ValidatorId = <T as frame_system::Config>::AccountId>,
	T: pallet_session::historical::Config<
		FullIdentification = ValidatorSnapshot<
			<T as frame_system::Config>::AccountId,
			BalanceOf<T>,
		>,
		FullIdentificationOf = ValidatorSnapshotOf<T>,
	>,
	T::SessionHandler: pallet_session::SessionHandler<<T as frame_system::Config>::AccountId>,
	T::SessionManager: pallet_session::SessionManager<<T as frame_system::Config>::AccountId>,
	T::ValidatorIdOf: Convert<
		<T as frame_system::Config>::AccountId,
		Option<<T as frame_system::Config>::AccountId>,
	>,
	T::AccountId: Copy,
{
	fn on_offence(
		offenders: &[OffenceDetails<
			T::AccountId,
			pallet_session::historical::IdentificationTuple<T>,
		>],
		slash_fraction: &[Perbill],
		slash_session: SessionIndex,
		_disable_strategy: DisableStrategy,
	) -> Weight {
		let round = Round::<T>::get();
		for (details, slash_fraction) in offenders.iter().zip(slash_fraction) {
			let (controller, _snapshot) = &details.offender;
			if let Some(candidate_state) = CandidateInfo::<T>::get(controller) {
				// prevent offence handling if the validator is already kicked out (due to session
				// update delay)
				if candidate_state.is_kicked_out() {
					continue
				}
				let offender_slash = *slash_fraction * candidate_state.bond;
				let offence = Offence::new(
					round.current_round_index,
					slash_session,
					candidate_state.bond,
					offender_slash,
					offender_slash,
					BalanceOf::<T>::zero(), // disable slashing nominators for now
					*slash_fraction,
				);
				let (is_slashed, slash_amount) = T::OffenceHandler::try_handle_offence(
					&controller,
					&candidate_state.stash,
					candidate_state.tier,
					offence,
				);
				if is_slashed {
					// kick out validator from active set
					Pallet::<T>::kickout_validator(&controller);
					// update stake related storage
					Pallet::<T>::slash_reserved_bonds(&controller, slash_amount, &vec![]);
				}
			}
		}
		let consumed_weight: Weight = Weight::from_ref_time(0u64);
		consumed_weight
	}
}
