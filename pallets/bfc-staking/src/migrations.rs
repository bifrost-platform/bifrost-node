use super::*;

pub mod v6 {
	use frame_support::traits::OnRuntimeUpgrade;

	use super::*;

	pub struct MigrateToV6<T>(PhantomData<T>);

	#[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo)]
	/// The change request for a specific nomination.
	pub struct OldNominationRequest<AccountId, Balance> {
		/// The validator who owns this nomination
		pub validator: AccountId,
		/// The total unbonding amount of this request
		pub amount: Balance,
		/// The unbonding amount for each round.
		/// `Decrease` requests are allowed to be pending for multiple rounds.
		pub when_executable: RoundIndex,
		/// The requested unbonding action
		pub action: NominationChange,
	}

	#[derive(Clone, Encode, PartialEq, Decode, RuntimeDebug, TypeInfo)]
	/// Pending requests to mutate nominations for each nominator
	pub struct OldPendingNominationRequests<AccountId, Balance> {
		/// Number of pending revocations (necessary for determining whether revoke is exit)
		pub revocations_count: u32,
		/// Map from validator -> Request (enforces at most 1 pending request per nomination)
		pub requests: BTreeMap<AccountId, OldNominationRequest<AccountId, Balance>>,
		/// Total amount of pending requests.
		pub less_total: Balance,
	}

	#[derive(Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
	/// Nominator state
	pub struct OldNominator<AccountId, Balance> {
		/// Nominator account
		pub id: AccountId,
		/// Current state of all nominations
		pub nominations: BTreeMap<AccountId, Balance>,
		/// Initial state of all nominations
		pub initial_nominations: BTreeMap<AccountId, Balance>,
		/// Total balance locked for this nominator
		pub total: Balance,
		/// Requests to change nominations (decrease, revoke, and leave)
		pub requests: OldPendingNominationRequests<AccountId, Balance>,
		/// Status for this nominator
		pub status: NominatorStatus,
		/// The destination for round rewards
		pub reward_dst: RewardDestination,
		/// The total amount of awarded tokens to this nominator
		pub awarded_tokens: Balance,
		/// The amount of awarded tokens to this nominator per candidate
		pub awarded_tokens_per_candidate: BTreeMap<AccountId, Balance>,
	}

	impl<T: Config> OnRuntimeUpgrade for MigrateToV6<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();

			let current = Pallet::<T>::in_code_storage_version();
			let onchain = Pallet::<T>::on_chain_storage_version();

			if current == 6 && onchain == 5 {
				// 1. create `UnstakingNominations` for each validator
				for (who, _) in CandidateInfo::<T>::iter() {
					<UnstakingNominations<T>>::insert(&who, Nominations::default());
				}

				// 2. translate `NominatorState` & add requests to `UnstakingNominations`
				NominatorState::<T>::translate(
					|_, old: OldNominator<T::AccountId, BalanceOf<T>>| {
						// 2.1. remove `requests.revocations_count` field
						// 2.2. update `requests.requests.when_executable` from `RoundIndex` to `BTreeMap<RoundIndex, Balance>`
						let mut new_requests: BTreeMap<
							T::AccountId,
							NominationRequest<T::AccountId, BalanceOf<T>>,
						> = old.requests
							.requests
							.into_iter()
							.map(|(validator, request)| {
								(
									validator.clone(),
									NominationRequest {
										validator,
										amount: request.amount,
										when_executable: BTreeMap::from([(
											request.when_executable,
											request.amount,
										)]),
										action: request.action,
									},
								)
							})
							.collect();

						// 2.3. add leave requests to `requests.requests`
						match old.status {
							NominatorStatus::Leaving(round_index) => {
								for (validator, amount) in old.nominations.clone() {
									new_requests.insert(
										validator.clone(),
										NominationRequest {
											validator,
											amount,
											when_executable: BTreeMap::from([(
												round_index,
												amount,
											)]),
											action: NominationChange::Leave,
										},
									);
								}
							},
							_ => (),
						}

						// 2.4. decrease `nominations`, `total`, `requests.less_total`
						let mut new_nominations = old.nominations.clone();
						let mut new_total = old.total.clone();
						let mut new_less_total = BalanceOf::<T>::zero();
						for (validator, request) in new_requests.iter() {
							if let Some(amount) = new_nominations.remove(validator) {
								new_nominations.insert(
									validator.clone(),
									amount.saturating_sub(request.amount),
								);
								new_total = new_total.saturating_sub(request.amount);
								new_less_total = new_less_total.saturating_add(request.amount);

								// 2.5. add to `UnstakingNominations`
								let _ = Pallet::<T>::add_to_unstaking_nominations(
									validator.clone(),
									Bond { owner: old.id.clone(), amount: request.amount },
								);

								// 2.6. decrease `Total`
								<Total<T>>::mutate(|total| {
									*total = total.saturating_sub(request.amount);
								});

								// 2.7. decrease `CandidateInfo.voting_power`
								let mut candidate_info =
									CandidateInfo::<T>::get(&validator).expect("CandidateInfo DNE");
								candidate_info.voting_power = candidate_info
									.clone()
									.voting_power
									.saturating_sub(request.amount);

								// 2.8. decrease or remove from `TopNominations` | `BottomNominations`
								// this will reorganize the following fields
								// - `lowest_top_nomination_amount`
								// - `highest_bottom_nomination_amount`
								// - `lowest_bottom_nomination_amount`
								// - `top_capacity`
								// - `bottom_capacity`
								// - `CandidatePool`
								match request.action {
									NominationChange::Decrease => {
										candidate_info
											.decrease_nomination::<T>(
												&validator,
												old.id.clone(),
												amount,
												request.amount,
											)
											.expect("decrease_nomination failed");
									},
									NominationChange::Revoke | NominationChange::Leave => {
										candidate_info
											.rm_nomination_if_exists::<T>(
												&validator,
												old.id.clone(),
												request.amount,
											)
											.expect("rm_nomination_if_exists failed");
									},
								}
								<CandidateInfo<T>>::insert(&validator, candidate_info);
							}
						}

						Some(Nominator {
							id: old.id,
							nominations: new_nominations,
							initial_nominations: old.initial_nominations.clone(),
							total: new_total,
							requests: PendingNominationRequests {
								requests: new_requests,
								less_total: new_less_total,
							},
							status: old.status,
							reward_dst: old.reward_dst,
							awarded_tokens: old.awarded_tokens,
							awarded_tokens_per_candidate: old.awarded_tokens_per_candidate,
						})
					},
				);
				log!(info, "bfc-staking storage migration v6 completed successfully ✅");
			} else {
				log!(warn, "Skipping bfc-staking storage migration v6 💤");
				weight = weight.saturating_add(T::DbWeight::get().reads(1));
			}
			weight
		}
	}
}

pub mod v5 {
	use frame_support::traits::OnRuntimeUpgrade;

	use super::*;

	pub struct MigrateToV5<T>(PhantomData<T>);

	impl<T: Config> OnRuntimeUpgrade for MigrateToV5<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();

			let current = Pallet::<T>::in_code_storage_version();
			let onchain = Pallet::<T>::on_chain_storage_version();

			if current == 5 && onchain == 4 {
				for (who, _) in CandidateInfo::<T>::iter() {
					if let Some(bottom) = BottomNominations::<T>::get(&who) {
						if !bottom.nominations.is_empty() {
							for bottom in bottom.nominations {
								let mut candidate_info =
									CandidateInfo::<T>::get(&who).expect("CandidateInfo DNE");
								// should be added to top
								match candidate_info.add_top_nomination::<T>(
									&who,
									Bond { owner: bottom.owner.clone(), amount: bottom.amount },
								) {
									Ok(_) => {
										log!(
											info,
											"Nominator({:?}) for Candidate({:?}) has been moved to Top",
											bottom.owner.clone(),
											who,
										);
										<CandidateInfo<T>>::insert(&who, candidate_info);
									},
									Err(_) => {
										log!(
											error,
											"Failed to move Nominator({:?}) for Candidate({:?}) to Top",
											bottom.owner,
											who
										);
									},
								}
							}
							let mut after_candidate_info =
								CandidateInfo::<T>::get(&who).expect("CandidateInfo DNE");
							after_candidate_info.nomination_count = <TopNominations<T>>::get(&who)
								.expect("TopNomination DNE")
								.nominations
								.len() as u32;
							after_candidate_info.reset_bottom_data::<T>(&Nominations::default());
							<CandidateInfo<T>>::insert(&who, after_candidate_info);
							<BottomNominations<T>>::insert(&who, Nominations::default());
						}
					}
				}
				current.put::<Pallet<T>>();
				weight = weight.saturating_add(T::DbWeight::get().writes(1));

				log!(info, "bfc-staking storage migration v5 completed successfully ✅");
			} else {
				log!(warn, "Skipping bfc-staking storage migration v5 💤");
				weight = weight.saturating_add(T::DbWeight::get().reads(1));
			}
			weight
		}
	}
}

pub mod v4 {
	use super::*;
	use bp_staking::MAX_AUTHORITIES;
	use frame_support::{
		storage_alias, traits::OnRuntimeUpgrade, BoundedBTreeMap, BoundedBTreeSet,
	};

	#[cfg(feature = "try-runtime")]
	use sp_runtime::TryRuntimeError;

	#[storage_alias]
	pub type StorageVersion<T: Config> = StorageValue<Pallet<T>, Releases, ValueQuery>;

	#[storage_alias]
	pub type MinTotalSelected<T: Config> = StorageValue<Pallet<T>, u32, ValueQuery>;

	// #[derive(Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
	// /// Nominator state
	// pub struct OrderedSetNominator<AccountId, Balance> {
	// 	pub id: AccountId,
	// 	pub nominations: OrderedSet<Bond<AccountId, Balance>>,
	// 	pub initial_nominations: OrderedSet<Bond<AccountId, Balance>>,
	// 	pub total: Balance,
	// 	pub requests: PendingNominationRequests<AccountId, Balance>,
	// 	pub status: NominatorStatus,
	// 	pub reward_dst: RewardDestination,
	// 	pub awarded_tokens: Balance,
	// 	pub awarded_tokens_per_candidate: OrderedSet<Bond<AccountId, Balance>>,
	// }

	pub struct MigrateToV4<T>(PhantomData<T>);

	impl<T: Config> OnRuntimeUpgrade for MigrateToV4<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();

			let current = Pallet::<T>::in_code_storage_version();
			// (previous) let onchain = StorageVersion::<T>::get();
			let onchain = Pallet::<T>::on_chain_storage_version();

			// (previous: if current == 4 && onchain == Releases::V3_0_0)
			if current == 4 && onchain == 3 {
				MinTotalSelected::<T>::kill();
				weight = weight.saturating_add(T::DbWeight::get().reads_writes(0, 1));

				// translate `BoundedVec<Bond<T::AccountId, BalanceOf<T>>, ConstU32<MAX_AUTHORITIES>>` to `BoundedBTreeMap<T::AccountId, BalanceOf<T>, ConstU32<MAX_AUTHORITIES>>`
				<CandidatePool<T>>::translate::<
					BoundedVec<Bond<T::AccountId, BalanceOf<T>>, ConstU32<MAX_AUTHORITIES>>,
					_,
				>(|old_pool| {
					let new_pool = old_pool
						.expect("")
						.into_iter()
						.map(|bond| (bond.owner, bond.amount))
						.collect::<BTreeMap<T::AccountId, BalanceOf<T>>>();

					Some(BoundedBTreeMap::try_from(new_pool).expect(""))
				})
				.expect("");
				weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));

				// closure for translate `BoundedVec<T::AccountId, ConstU32<MAX_AUTHORITIES>>` to `BoundedBTreeSet<T::AccountId, ConstU32<MAX_AUTHORITIES>>`
				let vec_to_bset =
					|old: Option<BoundedVec<T::AccountId, ConstU32<MAX_AUTHORITIES>>>| {
						let new: BoundedBTreeSet<T::AccountId, ConstU32<MAX_AUTHORITIES>> = old
							.expect("")
							.into_iter()
							.collect::<BTreeSet<T::AccountId>>()
							.try_into()
							.expect("");
						Some(new)
					};
				<SelectedCandidates<T>>::translate::<
					BoundedVec<T::AccountId, ConstU32<MAX_AUTHORITIES>>,
					_,
				>(vec_to_bset)
				.expect("");
				<SelectedFullCandidates<T>>::translate::<
					BoundedVec<T::AccountId, ConstU32<MAX_AUTHORITIES>>,
					_,
				>(vec_to_bset)
				.expect("");
				<SelectedBasicCandidates<T>>::translate::<
					BoundedVec<T::AccountId, ConstU32<MAX_AUTHORITIES>>,
					_,
				>(vec_to_bset)
				.expect("");
				weight = weight.saturating_add(T::DbWeight::get().reads_writes(3, 3));

				// translate `Vec<(RoundIndex, Vec<T::AccountId>)>` to `BTreeMap<RoundIndex, BoundedBTreeSet<T::AccountId, ConstU32<MAX_AUTHORITIES>>>`
				<CachedSelectedCandidates<T>>::translate::<Vec<(RoundIndex, Vec<T::AccountId>)>, _>(
					|old| {
						Some(
							old.expect("")
								.into_iter()
								.map(|(round_index, candidates)| {
									let bset: BoundedBTreeSet<T::AccountId, ConstU32<MAX_AUTHORITIES>> =
										candidates
											.into_iter()
											.collect::<BTreeSet<T::AccountId>>()
											.try_into()
											.expect("");
									(round_index, bset)
								})
								.collect(),
						)
					},
				)
					.expect("");
				weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));

				// // translate old `Nominator` which using ordered set to new Nominator
				// <NominatorState<T>>::translate(
				// 	|_, old: OrderedSetNominator<T::AccountId, BalanceOf<T>>| {
				// 		weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));

				// 		let nominations: BTreeMap<_, _> = old
				// 			.nominations
				// 			.0
				// 			.into_iter()
				// 			.map(|bond| (bond.owner, bond.amount))
				// 			.collect();

				// 		let initial_nominations: BTreeMap<_, _> = old
				// 			.initial_nominations
				// 			.0
				// 			.into_iter()
				// 			.map(|bond| (bond.owner, bond.amount))
				// 			.collect();

				// 		let awarded_tokens_per_candidate: BTreeMap<_, _> = old
				// 			.awarded_tokens_per_candidate
				// 			.0
				// 			.clone()
				// 			.iter()
				// 			.map(|bond| (bond.owner.clone(), bond.amount))
				// 			.collect();

				// 		Some(Nominator {
				// 			id: old.id,
				// 			nominations,
				// 			initial_nominations,
				// 			total: old.total,
				// 			requests: old.requests,
				// 			status: old.status,
				// 			reward_dst: old.reward_dst,
				// 			awarded_tokens: old.awarded_tokens,
				// 			awarded_tokens_per_candidate,
				// 		})
				// 	},
				// );

				// translate `Vec<(RoundIndex, u32)>` to `BTreeMap<RoundIndex, u32>`
				<CachedMajority<T>>::translate::<Vec<(RoundIndex, u32)>, _>(|old| {
					Some(old.expect("").into_iter().collect::<BTreeMap<RoundIndex, u32>>())
				})
				.expect("");
				weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));

				// migrate to new standard storage version
				StorageVersion::<T>::kill();
				current.put::<Pallet<T>>();

				log!(info, "bfc-staking storage migration passes v4 update ✅");
				weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 2));
			} else {
				log!(warn, "Skipping bfc-staking storage migration v4 💤");
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

	// #[derive(Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
	// pub struct OldNominator<AccountId, Balance> {
	// 	pub id: AccountId,
	// 	pub nominations: OrderedSet<Bond<AccountId, Balance>>,
	// 	pub initial_nominations: OrderedSet<Bond<AccountId, Balance>>,
	// 	pub total: Balance,
	// 	pub requests: PendingNominationRequests<AccountId, Balance>,
	// 	pub status: NominatorStatus,
	// 	pub reward_dst: RewardDestination,
	// 	pub awarded_tokens: Balance,
	// }

	pub fn pre_migrate<T: Config>() -> Result<(), &'static str> {
		// frame_support::ensure!(
		// 	StorageVersion::<T>::get() == Releases::V1_0_0,
		// 	"Storage version must upgrade linearly",
		// );
		crate::log!(info, "bfc-staking migration passes pre-migrate checks ✅",);
		Ok(())
	}

	pub fn migrate<T: Config>() -> Weight {
		// NominatorState::<T>::translate(|_key, old: OldNominator<T::AccountId, BalanceOf<T>>| {
		// 	let nominations = old.nominations.0.clone();
		// 	let mut awarded_tokens_per_candidate = OrderedSet::new();
		// 	for nomination in nominations {
		// 		awarded_tokens_per_candidate
		// 			.insert(Bond { owner: nomination.owner.clone(), amount: Zero::zero() });
		// 	}
		// 	Some(Nominator {
		// 		id: old.id,
		// 		nominations: old.nominations,
		// 		initial_nominations: old.initial_nominations,
		// 		total: old.total,
		// 		requests: old.requests,
		// 		status: old.status,
		// 		reward_dst: old.reward_dst,
		// 		awarded_tokens: old.awarded_tokens,
		// 		awarded_tokens_per_candidate,
		// 	})
		// });
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
