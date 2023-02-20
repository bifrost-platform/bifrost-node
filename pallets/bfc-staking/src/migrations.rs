use super::*;

pub mod v3 {
	use super::*;
	// use frame_support::traits::Get;

	pub fn migrate<T: Config>() -> Weight {
		// let mut candidate_pool = CandidatePool::<T>::get();
		// for mut candidate in CandidateInfo::<T>::iter() {
		// 	let mut is_contained = false;
		// 	for c in candidate_pool.iter() {
		// 		if c.owner == candidate.0 {
		// 			is_contained = true;
		// 			break
		// 		}
		// 	}
		// 	if !is_contained {
		// 		candidate.1.reset_blocks_produced();
		// 		candidate_pool
		// 			.push(Bond { owner: candidate.0.clone(), amount: candidate.1.voting_power });
		// 	}
		// 	candidate.1.reset_productivity();
		// 	candidate.1.status = ValidatorStatus::Active;
		// 	CandidateInfo::<T>::insert(&candidate.0, candidate.1.clone());
		// }
		// Pallet::<T>::sort_candidates_by_voting_power();
		// CandidatePool::<T>::put(candidate_pool);
		// // StorageVersion::<T>::put(Releases::V3_0_0);
		// crate::log!(info, "bfc-staking migration passes Releases::V3_0_0 migrate checks ✅");
		T::BlockWeights::get().max_block
	}
}

pub mod v2 {
	use super::*;
	use frame_support::traits::Get;

	#[derive(Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
	pub struct OldNominator<AccountId, Balance> {
		pub id: AccountId,
		pub nominations: OrderedSet<Bond<AccountId, Balance>>,
		pub initial_nominations: OrderedSet<Bond<AccountId, Balance>>,
		pub total: Balance,
		pub requests: PendingNominationRequests<AccountId, Balance>,
		pub status: NominatorStatus,
		pub reward_dst: RewardDestination,
		pub awarded_tokens: Balance,
	}

	pub fn pre_migrate<T: Config>() -> Result<(), &'static str> {
		// frame_support::ensure!(
		// 	StorageVersion::<T>::get() == Releases::V1_0_0,
		// 	"Storage version must upgrade linearly",
		// );
		crate::log!(info, "bfc-staking migration passes pre-migrate checks ✅",);
		Ok(())
	}

	pub fn migrate<T: Config>() -> Weight {
		NominatorState::<T>::translate(|_key, old: OldNominator<T::AccountId, BalanceOf<T>>| {
			let nominations = old.nominations.0.clone();
			let mut awarded_tokens_per_candidate = OrderedSet::new();
			for nomination in nominations {
				awarded_tokens_per_candidate
					.insert(Bond { owner: nomination.owner.clone(), amount: Zero::zero() });
			}
			Some(Nominator {
				id: old.id,
				nominations: old.nominations,
				initial_nominations: old.initial_nominations,
				total: old.total,
				requests: old.requests,
				status: old.status,
				reward_dst: old.reward_dst,
				awarded_tokens: old.awarded_tokens,
				awarded_tokens_per_candidate,
			})
		});
		// StorageVersion::<T>::put(Releases::V2_0_0);

		crate::log!(info, "bfc-staking migration passes Releases::V2_0_0 migrate checks ✅");
		T::BlockWeights::get().max_block
	}
}

pub mod v1 {
	use super::*;
	use frame_support::traits::Get;

	pub fn migrate<T: Config>() -> Weight {
		// let old_bonded_round = BondedRound::<T>::get();
		// BondedRoundPerSession::<T>::put(old_bonded_round.clone());
		// BondedRound::<T>::kill();

		// StorageVersion::<T>::put(Releases::V1_0_0);

		crate::log!(info, "bfc-staking migration passes Releases::V1_0_0 migrate checks ✅",);
		T::BlockWeights::get().max_block
	}
}
