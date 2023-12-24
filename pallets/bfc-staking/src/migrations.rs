use super::*;

pub mod v4 {
	use super::*;
	use frame_support::storage_alias;
	use frame_support::traits::OnRuntimeUpgrade;

	#[cfg(feature = "try-runtime")]
	use sp_runtime::TryRuntimeError;

	#[storage_alias]
	pub type StorageVersion<T: Config> = StorageValue<Pallet<T>, Releases, ValueQuery>;

	pub struct MigrateToV4<T>(PhantomData<T>);

	impl<T: Config> OnRuntimeUpgrade for MigrateToV4<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();

			let current = Pallet::<T>::current_storage_version();
			let onchain = StorageVersion::<T>::get();

			if current == 4 && onchain == Releases::V3_0_0 {
				StorageVersion::<T>::kill();
				current.put::<Pallet<T>>();

				log!(info, "bfc-staking storage migration passes v4 update ✅");
				weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 2));
			} else {
				log!(warn, "Skipping bfc-staking migration v4, should be removed");
				weight = weight.saturating_add(T::DbWeight::get().reads(1));
			}

			weight
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
			ensure!(
				StorageVersion::<T>::get() == Releases::V3_0_0,
				"Required v3_0_0 before upgrading to v4"
			);

			Ok(Default::default())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(_state: Vec<u8>) -> Result<(), TryRuntimeError> {
			ensure!(Pallet::<T>::on_chain_storage_version() == 4, "v4 not applied");

			ensure!(!StorageVersion::<T>::exists(), "Storage version not migrated correctly");

			Ok(())
		}
	}
}

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
