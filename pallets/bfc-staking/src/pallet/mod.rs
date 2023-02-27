mod impls;
pub use impls::*;

use crate::{
	BalanceOf, BlockNumberOf, Bond, CandidateMetadata, DelayedCommissionSet, DelayedControllerSet,
	DelayedPayout, InflationInfo, NominationChange, NominationRequest, Nominations, Nominator,
	NominatorAdded, Range, Releases, RewardDestination, RewardPoint, RoundIndex, RoundInfo,
	TierType, TotalSnapshot, ValidatorSnapshot, WeightInfo,
};

use bp_staking::{
	traits::{OffenceHandler, RelayManager},
	MAX_AUTHORITIES,
};
use frame_support::{
	dispatch::DispatchResultWithPostInfo,
	pallet_prelude::*,
	traits::{Currency, Get, ReservableCurrency},
	Twox64Concat,
};
use frame_system::pallet_prelude::*;
use sp_runtime::{
	traits::{Saturating, Zero},
	Perbill,
};
use sp_staking::SessionIndex;
use sp_std::{collections::btree_map::BTreeMap, prelude::*};

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	/// Pallet for bfc staking
	#[pallet::pallet]
	#[pallet::generate_store(pub(crate) trait Store)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	/// Configuration trait of this pallet.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Overarching event type
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		/// The currency type
		type Currency: Currency<Self::AccountId> + ReservableCurrency<Self::AccountId>;
		/// The origin for monetary governance
		type MonetaryGovernanceOrigin: EnsureOrigin<Self::RuntimeOrigin>;
		/// The relay manager type
		type RelayManager: RelayManager<Self::AccountId>;
		/// The offence handler type
		type OffenceHandler: OffenceHandler<Self::AccountId, BalanceOf<Self>>;
		/// The default number of blocks per session at genesis
		#[pallet::constant]
		type DefaultBlocksPerSession: Get<u32>;
		/// The default number of blocks per round at genesis
		#[pallet::constant]
		type DefaultBlocksPerRound: Get<u32>;
		/// The default minimum number of blocks per round at genesis
		#[pallet::constant]
		type MinBlocksPerRound: Get<u32>;
		/// The max lifetime in rounds for certain storage data to be cached
		#[pallet::constant]
		type StorageCacheLifetimeInRounds: Get<u32>;
		/// Number of rounds that candidates remain bonded before exit request is executable
		#[pallet::constant]
		type LeaveCandidatesDelay: Get<RoundIndex>;
		/// Number of rounds candidate requests to decrease self-bond must wait to be executable
		#[pallet::constant]
		type CandidateBondLessDelay: Get<RoundIndex>;
		/// Number of rounds that nominators remain bonded before exit request is executable
		#[pallet::constant]
		type LeaveNominatorsDelay: Get<RoundIndex>;
		/// Number of rounds that nominations remain bonded before revocation request is executable
		#[pallet::constant]
		type RevokeNominationDelay: Get<RoundIndex>;
		/// Number of rounds that nomination less requests must wait before executable
		#[pallet::constant]
		type NominationBondLessDelay: Get<RoundIndex>;
		/// Number of rounds after which block authors are rewarded
		#[pallet::constant]
		type RewardPaymentDelay: Get<RoundIndex>;
		/// Default maximum number of selected full node candidates every round
		#[pallet::constant]
		type DefaultMaxSelectedFullCandidates: Get<u32>;
		/// Default maximum number of selected basic node candidates every round
		#[pallet::constant]
		type DefaultMaxSelectedBasicCandidates: Get<u32>;
		/// Default minimum number of selected candidates (full and basic) every round
		#[pallet::constant]
		type DefaultMinSelectedCandidates: Get<u32>;
		/// Maximum top nominations counted per candidate
		#[pallet::constant]
		type MaxTopNominationsPerCandidate: Get<u32>;
		/// Maximum bottom nominations (not counted) per candidate
		#[pallet::constant]
		type MaxBottomNominationsPerCandidate: Get<u32>;
		/// Maximum nominations per nominator
		#[pallet::constant]
		type MaxNominationsPerNominator: Get<u32>;
		/// The default commission rate for a full validator
		#[pallet::constant]
		type DefaultFullValidatorCommission: Get<Perbill>;
		/// The default commission rate for a basic validator
		#[pallet::constant]
		type DefaultBasicValidatorCommission: Get<Perbill>;
		/// The maxmimum commission rate available for a full validator
		#[pallet::constant]
		type MaxFullValidatorCommission: Get<Perbill>;
		/// The maxmimum commission rate available for a basic validator
		#[pallet::constant]
		type MaxBasicValidatorCommission: Get<Perbill>;
		/// Minimum stake required for any full node candidate to be in `SelectedCandidates` for the
		/// round
		#[pallet::constant]
		type MinFullValidatorStk: Get<BalanceOf<Self>>;
		/// Minimum stake required for any basic node candidate to be in `SelectedCandidates` for
		/// the round
		#[pallet::constant]
		type MinBasicValidatorStk: Get<BalanceOf<Self>>;
		/// Minimum stake required for any account to be a full validator candidate
		#[pallet::constant]
		type MinFullCandidateStk: Get<BalanceOf<Self>>;
		/// Minimum stake required for any account to be a basic validator candidate
		#[pallet::constant]
		type MinBasicCandidateStk: Get<BalanceOf<Self>>;
		/// Minimum stake for any registered on-chain account to nominate
		#[pallet::constant]
		type MinNomination: Get<BalanceOf<Self>>;
		/// Minimum stake for any registered on-chain account to be a nominator
		#[pallet::constant]
		type MinNominatorStk: Get<BalanceOf<Self>>;
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T> {
		NominatorDNE,
		CandidateDNE,
		StashDNE,
		NominationDNE,
		CommissionSetDNE,
		ControllerSetDNE,
		NominatorExists,
		CandidateExists,
		CandidateBondBelowMin,
		InsufficientBalance,
		NominatorBondBelowMin,
		NominationBelowMin,
		AlreadyOffline,
		AlreadyActive,
		AlreadyBonded,
		AlreadyPaired,
		NominatorAlreadyLeaving,
		NominatorNotLeaving,
		NominatorCannotLeaveYet,
		CannotNominateIfLeaving,
		CandidateAlreadyLeaving,
		CandidateNotLeaving,
		CandidateCannotLeaveYet,
		CannotGoOnlineIfLeaving,
		CannotLeaveIfOffline,
		ExceedMaxNominationsPerNominator,
		AlreadyNominatedCandidate,
		AlreadyControllerSetRequested,
		AlreadyCommissionSetRequested,
		InvalidSchedule,
		InvalidTierType,
		CannotSetBelowMin,
		CannotSetBelowOne,
		CannotSetAboveMax,
		RoundLengthMustBeAtLeastTotalSelectedValidators,
		RoundLengthMustBeLongerThanCreatedBlocks,
		NoWritingSameValue,
		TooManyCandidates,
		TooLowCandidateCountWeightHintJoinCandidates,
		TooLowCandidateCountWeightHintCancelLeaveCandidates,
		TooLowCandidateCountToLeaveCandidates,
		TooLowNominationCountToNominate,
		TooLowCandidateNominationCountToNominate,
		TooLowCandidateNominationCountToLeaveCandidates,
		TooLowNominationCountToLeaveNominators,
		PendingCandidateRequestsDNE,
		PendingCandidateRequestAlreadyExists,
		PendingCandidateRequestNotDueYet,
		PendingNominationRequestDNE,
		PendingNominationRequestAlreadyExists,
		PendingNominationRequestNotDueYet,
		CannotNominateLessThanLowestBottomWhenBottomIsFull,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Started a new round.
		NewRound {
			starting_block: T::BlockNumber,
			round: RoundIndex,
			selected_validators_number: u32,
			total_balance: BalanceOf<T>,
		},
		/// Account joined the set of validator candidates.
		JoinedValidatorCandidates {
			account: T::AccountId,
			amount_locked: BalanceOf<T>,
			new_total_amt_locked: BalanceOf<T>,
		},
		/// Active validator set update. Total Exposed Amount includes all nominations.
		ValidatorChosen {
			round: RoundIndex,
			validator_account: T::AccountId,
			total_exposed_amount: BalanceOf<T>,
		},
		/// Candidate requested to decrease a self bond.
		CandidateBondLessRequested {
			candidate: T::AccountId,
			amount_to_decrease: BalanceOf<T>,
			execute_round: RoundIndex,
		},
		/// Candidate has increased a self bond.
		CandidateBondedMore {
			candidate: T::AccountId,
			amount: BalanceOf<T>,
			new_total_bond: BalanceOf<T>,
		},
		/// Candidate has decreased a self bond.
		CandidateBondedLess {
			candidate: T::AccountId,
			amount: BalanceOf<T>,
			new_bond: BalanceOf<T>,
		},
		/// Candidate temporarily left the set of validator candidates without unbonding.
		CandidateWentOffline { candidate: T::AccountId },
		/// Candidate rejoins the set of validator candidates.
		CandidateBackOnline { candidate: T::AccountId },
		/// Candidate has requested to leave the set of candidates.
		CandidateScheduledExit {
			exit_allowed_round: RoundIndex,
			candidate: T::AccountId,
			scheduled_exit: RoundIndex,
		},
		/// Cancelled the request to leave the set of candidates.
		CancelledCandidateExit { candidate: T::AccountId },
		/// Cancelled the request to decrease candidate's bond.
		CancelledCandidateBondLess {
			candidate: T::AccountId,
			amount: BalanceOf<T>,
			execute_round: RoundIndex,
		},
		/// Candidate has left the set of candidates.
		CandidateLeft {
			ex_candidate: T::AccountId,
			unlocked_amount: BalanceOf<T>,
			new_total_amt_locked: BalanceOf<T>,
		},
		/// Nominator requested to decrease a bond for the validator candidate.
		NominationDecreaseScheduled {
			nominator: T::AccountId,
			candidate: T::AccountId,
			amount_to_decrease: BalanceOf<T>,
			execute_round: RoundIndex,
		},
		/// Nomination increased.
		NominationIncreased {
			nominator: T::AccountId,
			candidate: T::AccountId,
			amount: BalanceOf<T>,
			in_top: bool,
		},
		/// Nomination decreased.
		NominationDecreased {
			nominator: T::AccountId,
			candidate: T::AccountId,
			amount: BalanceOf<T>,
			in_top: bool,
		},
		/// Nominator requested to leave the set of nominators.
		NominatorExitScheduled {
			round: RoundIndex,
			nominator: T::AccountId,
			scheduled_exit: RoundIndex,
		},
		/// Nominator requested to revoke nomination.
		NominationRevocationScheduled {
			round: RoundIndex,
			nominator: T::AccountId,
			candidate: T::AccountId,
			scheduled_exit: RoundIndex,
		},
		/// Nominator has left the set of nominators.
		NominatorLeft { nominator: T::AccountId, unstaked_amount: BalanceOf<T> },
		/// Nomination revoked.
		NominationRevoked {
			nominator: T::AccountId,
			candidate: T::AccountId,
			unstaked_amount: BalanceOf<T>,
		},
		/// Nomination kicked.
		NominationKicked {
			nominator: T::AccountId,
			candidate: T::AccountId,
			unstaked_amount: BalanceOf<T>,
		},
		/// Cancelled a pending request to exit the set of nominators.
		NominatorExitCancelled { nominator: T::AccountId },
		/// Cancelled request to change an existing nomination.
		CancelledNominationRequest {
			nominator: T::AccountId,
			cancelled_request: NominationRequest<T::AccountId, BalanceOf<T>>,
		},
		/// New nomination (increase of the existing one).
		Nomination {
			nominator: T::AccountId,
			locked_amount: BalanceOf<T>,
			candidate: T::AccountId,
			nominator_position: NominatorAdded<BalanceOf<T>>,
		},
		/// Nomination from candidate state has been remove.
		NominatorLeftCandidate {
			nominator: T::AccountId,
			candidate: T::AccountId,
			unstaked_amount: BalanceOf<T>,
			total_candidate_staked: BalanceOf<T>,
		},
		/// Paid the account (nominator or validator) the round reward.
		Rewarded { account: T::AccountId, rewards: BalanceOf<T> },
		/// Annual inflation input (first 3) was used to derive new per-round inflation (last 3)
		InflationSet {
			annual_min: Perbill,
			annual_ideal: Perbill,
			annual_max: Perbill,
			round_min: Perbill,
			round_ideal: Perbill,
			round_max: Perbill,
		},
		/// Staking expectations set.
		StakeExpectationsSet {
			expect_min: BalanceOf<T>,
			expect_ideal: BalanceOf<T>,
			expect_max: BalanceOf<T>,
		},
		/// Set the maximum selected full candidates to this value.
		MaxFullSelectedSet { old: u32, new: u32 },
		/// Set the maximum selected basic candidates to this value.
		MaxBasicSelectedSet { old: u32, new: u32 },
		/// Set the minimum selected candidates to this value.
		MinTotalSelectedSet { old: u32, new: u32 },
		/// Set the default validator commission to this value.
		DefaultValidatorCommissionSet { old: Perbill, new: Perbill, tier: TierType },
		/// Set the maximum validator commission to this value.
		MaxValidatorCommissionSet { old: Perbill, new: Perbill, tier: TierType },
		/// Set the validator commission.
		ValidatorCommissionSet { candidate: T::AccountId, old: Perbill, new: Perbill },
		/// Cancel the validator commission set.
		ValidatorCommissionSetCancelled { candidate: T::AccountId },
		/// Set blocks per round.
		BlocksPerRoundSet {
			current_round: RoundIndex,
			first_block: T::BlockNumber,
			old: u32,
			new: u32,
			new_per_round_inflation_min: Perbill,
			new_per_round_inflation_ideal: Perbill,
			new_per_round_inflation_max: Perbill,
		},
		/// Set the storage cache lifetime.
		StorageCacheLifetimeSet { old: u32, new: u32 },
		/// Set the controller account.
		ControllerSet { old: T::AccountId, new: T::AccountId },
		/// Cancel the controller set.
		ControllerSetCancelled { candidate: T::AccountId },
		/// Set the validator reward destination
		ValidatorRewardDstSet {
			candidate: T::AccountId,
			old: RewardDestination,
			new: RewardDestination,
		},
		/// Set the nominator reward destination
		NominatorRewardDstSet {
			nominator: T::AccountId,
			old: RewardDestination,
			new: RewardDestination,
		},
		/// Kick out validator
		KickedOut(T::AccountId),
	}

	#[pallet::storage]
	/// Storage version of the pallet.
	pub(crate) type StorageVersion<T: Config> = StorageValue<_, Releases, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn session)]
	/// Current session index of current round
	pub type Session<T> = StorageValue<_, SessionIndex, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn round)]
	/// Current round index and next round scheduled transition
	pub(crate) type Round<T: Config> = StorageValue<_, RoundInfo<T::BlockNumber>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn storage_cache_lifetime)]
	/// The max storage lifetime for storage data to be cached
	pub type StorageCacheLifetime<T: Config> = StorageValue<_, u32, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn default_full_validator_commission)]
	/// Default commission rate for full validators
	pub type DefaultFullValidatorCommission<T: Config> = StorageValue<_, Perbill, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn default_basic_validator_commission)]
	/// Default commission rate for basic validators
	pub type DefaultBasicValidatorCommission<T: Config> = StorageValue<_, Perbill, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn min_full_validator_commission)]
	/// Maximum commission rate for full validators
	pub type MaxFullValidatorCommission<T: Config> = StorageValue<_, Perbill, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn min_basic_validator_commission)]
	/// Maximum commission rate for basic validators
	pub type MaxBasicValidatorCommission<T: Config> = StorageValue<_, Perbill, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn max_total_selected)]
	/// The maximum node candidates selected every round
	pub type MaxTotalSelected<T: Config> = StorageValue<_, u32, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn max_full_selected)]
	/// The maximum full node candidates selected every round
	pub type MaxFullSelected<T: Config> = StorageValue<_, u32, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn max_basic_selected)]
	/// The maximum basic node candidates selected every round
	pub type MaxBasicSelected<T: Config> = StorageValue<_, u32, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn min_total_selected)]
	/// The minimum candidates selected every round
	pub type MinTotalSelected<T: Config> = StorageValue<_, u32, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn productivity_per_block)]
	/// The productivity rate per block in the current round
	pub type ProductivityPerBlock<T: Config> = StorageValue<_, Perbill, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn nominator_state)]
	/// Get nominator state associated with an account if account is nominating else None
	pub(crate) type NominatorState<T: Config> = StorageMap<
		_,
		Twox64Concat,
		T::AccountId,
		Nominator<T::AccountId, BalanceOf<T>>,
		OptionQuery,
	>;

	#[pallet::storage]
	#[pallet::getter(fn candidate_info)]
	/// Get validator candidate info associated with an account if account is candidate else None
	pub type CandidateInfo<T: Config> = StorageMap<
		_,
		Twox64Concat,
		T::AccountId,
		CandidateMetadata<T::AccountId, BalanceOf<T>, BlockNumberOf<T>>,
		OptionQuery,
	>;

	#[pallet::storage]
	#[pallet::getter(fn bonded_stash)]
	/// Map from all locked "stash" accounts to the controller account.
	pub type BondedStash<T: Config> = StorageMap<_, Twox64Concat, T::AccountId, T::AccountId>;

	#[pallet::storage]
	#[pallet::getter(fn top_nominations)]
	/// Top nominations for validator candidate
	pub(crate) type TopNominations<T: Config> = StorageMap<
		_,
		Twox64Concat,
		T::AccountId,
		Nominations<T::AccountId, BalanceOf<T>>,
		OptionQuery,
	>;

	#[pallet::storage]
	#[pallet::getter(fn bottom_nominations)]
	/// Bottom nominations for validator candidate
	pub(crate) type BottomNominations<T: Config> = StorageMap<
		_,
		Twox64Concat,
		T::AccountId,
		Nominations<T::AccountId, BalanceOf<T>>,
		OptionQuery,
	>;

	#[pallet::storage]
	#[pallet::getter(fn selected_candidates)]
	/// The active validator set (full and basic) selected for the current round
	pub type SelectedCandidates<T: Config> =
		StorageValue<_, BoundedVec<T::AccountId, ConstU32<MAX_AUTHORITIES>>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn selected_full_candidates)]
	/// The active full validator set selected for the current round
	pub type SelectedFullCandidates<T: Config> =
		StorageValue<_, BoundedVec<T::AccountId, ConstU32<MAX_AUTHORITIES>>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn selected_basic_candidates)]
	/// The active basic validator set selected for the current round
	pub type SelectedBasicCandidates<T: Config> =
		StorageValue<_, BoundedVec<T::AccountId, ConstU32<MAX_AUTHORITIES>>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn cached_selected_candidates)]
	/// The cached active validator set selected from previous rounds
	pub type CachedSelectedCandidates<T: Config> =
		StorageValue<_, Vec<(RoundIndex, Vec<T::AccountId>)>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn majority)]
	/// The majority of the current active validator set
	pub type Majority<T: Config> = StorageValue<_, u32, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn cached_majority)]
	/// The cached majority based on the active validator set selected from previous rounds
	pub type CachedMajority<T: Config> = StorageValue<_, Vec<(RoundIndex, u32)>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn total)]
	/// Total capital locked by this staking pallet
	pub(crate) type Total<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn candidate_pool)]
	/// The pool of validator candidates, each with their total voting power
	pub(crate) type CandidatePool<T: Config> = StorageValue<
		_,
		BoundedVec<Bond<T::AccountId, BalanceOf<T>>, ConstU32<MAX_AUTHORITIES>>,
		ValueQuery,
	>;

	#[pallet::storage]
	#[pallet::getter(fn at_stake)]
	/// Snapshot of validator nomination stake at the start of the round
	pub type AtStake<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		RoundIndex,
		Twox64Concat,
		T::AccountId,
		ValidatorSnapshot<T::AccountId, BalanceOf<T>>,
		ValueQuery,
	>;

	#[pallet::storage]
	#[pallet::getter(fn total_at_stake)]
	/// Snapshot of the network state at the start of the round
	pub type TotalAtStake<T: Config> =
		StorageMap<_, Twox64Concat, RoundIndex, TotalSnapshot<BalanceOf<T>>, OptionQuery>;

	#[pallet::storage]
	#[pallet::getter(fn delayed_payouts)]
	/// Delayed reward payouts
	pub type DelayedPayouts<T: Config> =
		StorageMap<_, Twox64Concat, RoundIndex, DelayedPayout<BalanceOf<T>>, OptionQuery>;

	#[pallet::storage]
	#[pallet::getter(fn delayed_controller_sets)]
	/// Delayed new controller account set requests
	pub type DelayedControllerSets<T: Config> = StorageMap<
		_,
		Twox64Concat,
		RoundIndex,
		BoundedVec<DelayedControllerSet<T::AccountId>, ConstU32<MAX_AUTHORITIES>>,
		ValueQuery,
	>;

	#[pallet::storage]
	#[pallet::getter(fn delayed_commission_sets)]
	pub type DelayedCommissionSets<T: Config> = StorageMap<
		_,
		Twox64Concat,
		RoundIndex,
		BoundedVec<DelayedCommissionSet<T::AccountId>, ConstU32<MAX_AUTHORITIES>>,
		ValueQuery,
	>;

	#[pallet::storage]
	#[pallet::getter(fn staked)]
	/// Total counted stake for selected candidates in the round
	pub type Staked<T: Config> = StorageMap<_, Twox64Concat, RoundIndex, BalanceOf<T>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn inflation_config)]
	/// Inflation configuration
	pub type InflationConfig<T: Config> = StorageValue<_, InflationInfo<BalanceOf<T>>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn points)]
	/// Total points awarded to validators for block production in the round
	pub type Points<T: Config> = StorageMap<_, Twox64Concat, RoundIndex, RewardPoint, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn awarded_pts)]
	/// Points for each validator per round
	pub type AwardedPts<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		RoundIndex,
		Twox64Concat,
		T::AccountId,
		RewardPoint,
		ValueQuery,
	>;

	#[pallet::storage]
	#[pallet::getter(fn awarded_tokens)]
	/// The amount of awarded tokens to validators and nominators since genesis
	pub type AwardedTokens<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(n: T::BlockNumber) -> Weight {
			let mut weight = T::WeightInfo::base_on_initialize();

			// Update the current block of the round
			let mut round = <Round<T>>::get();
			round.update_block(n);
			<Round<T>>::put(round);

			// Refresh the current state of the total stake snapshot
			Self::refresh_total_snapshot(round.current_round_index);

			// Handle the delayed payouts for the previous round
			weight += Self::handle_delayed_payouts(round.current_round_index);
			weight
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		/// The initial candidates for network genesis
		pub candidates: Vec<(T::AccountId, T::AccountId, T::AccountId, BalanceOf<T>)>,
		/// The initial nominations for network genesis
		pub nominations: Vec<(T::AccountId, T::AccountId, BalanceOf<T>)>,
		/// The initial inflation configurations for network genesis
		pub inflation_config: InflationInfo<BalanceOf<T>>,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self { candidates: vec![], nominations: vec![], inflation_config: Default::default() }
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			StorageVersion::<T>::put(Releases::V3_0_0);
			<InflationConfig<T>>::put(self.inflation_config.clone());
			// Set validator commission to default config
			<DefaultFullValidatorCommission<T>>::put(T::DefaultFullValidatorCommission::get());
			<DefaultBasicValidatorCommission<T>>::put(T::DefaultBasicValidatorCommission::get());
			// Set maximum validator commission to maximum config
			<MaxFullValidatorCommission<T>>::put(T::MaxFullValidatorCommission::get());
			<MaxBasicValidatorCommission<T>>::put(T::MaxBasicValidatorCommission::get());
			let mut candidate_count = 0u32;
			// Initialize the candidates
			for &(ref stash, ref controller, ref relayer, balance) in &self.candidates {
				assert!(
					T::Currency::free_balance(stash) >= balance,
					"Stash account does not have enough balance to bond as a candidate."
				);
				candidate_count += 1u32;
				if let Err(error) = <Pallet<T>>::join_candidates(
					T::RuntimeOrigin::from(Some(stash.clone()).into()),
					controller.clone(),
					Some(relayer.clone()),
					balance,
					candidate_count,
				) {
					log::warn!("Join candidates failed in genesis with error {:?}", error);
				} else {
					candidate_count += 1u32;
				}
			}
			let mut validator_nominator_count: BTreeMap<T::AccountId, u32> = BTreeMap::new();
			let mut nominator_nomination_count: BTreeMap<T::AccountId, u32> = BTreeMap::new();
			// Initialize the nominations
			for &(ref nominator, ref target, balance) in &self.nominations {
				assert!(
					T::Currency::free_balance(nominator) >= balance,
					"Account does not have enough balance to place nomination."
				);
				let vn_count =
					if let Some(x) = validator_nominator_count.get(target) { *x } else { 0u32 };
				let nn_count =
					if let Some(x) = nominator_nomination_count.get(nominator) { *x } else { 0u32 };
				if let Err(error) = <Pallet<T>>::nominate(
					T::RuntimeOrigin::from(Some(nominator.clone()).into()),
					target.clone(),
					balance,
					vn_count,
					nn_count,
				) {
					log::warn!("Nominate failed in genesis with error {:?}", error);
				} else {
					if let Some(x) = validator_nominator_count.get_mut(target) {
						*x += 1u32;
					} else {
						validator_nominator_count.insert(target.clone(), 1u32);
					};
					if let Some(x) = nominator_nomination_count.get_mut(nominator) {
						*x += 1u32;
					} else {
						nominator_nomination_count.insert(nominator.clone(), 1u32);
					};
				}
			}
			// Set max selected node candidates to maximum config
			<MaxTotalSelected<T>>::put(
				T::DefaultMaxSelectedFullCandidates::get() +
					T::DefaultMaxSelectedBasicCandidates::get(),
			);
			// Set max selected full node candidates to maximum config
			<MaxFullSelected<T>>::put(T::DefaultMaxSelectedFullCandidates::get());
			// Set max selected basic node candidates to maximum config
			<MaxBasicSelected<T>>::put(T::DefaultMaxSelectedBasicCandidates::get());
			// Set min selected candidates to minimum config
			<MinTotalSelected<T>>::put(T::DefaultMinSelectedCandidates::get());
			// Set storage cache lifetime to default config
			<StorageCacheLifetime<T>>::put(T::StorageCacheLifetimeInRounds::get());
			// Choose top MaxFullSelected validator candidates
			let (full_validators, basic_validators) = <Pallet<T>>::compute_top_candidates();
			let (v_count, _, total_staked) =
				<Pallet<T>>::update_top_candidates(1u32, full_validators, basic_validators);
			// Set majority to initial value
			let initial_majority: u32 = <Pallet<T>>::compute_majority();
			<Majority<T>>::put(initial_majority);
			<CachedMajority<T>>::put(vec![(1u32, initial_majority)]);
			T::RelayManager::refresh_majority(1u32);
			// Start Round 1 at Block 0
			let round: RoundInfo<T::BlockNumber> = RoundInfo::new(
				1u32,
				0u32,
				0u32,
				0u32.into(),
				0u32.into(),
				0u32.into(),
				T::DefaultBlocksPerRound::get(),
				T::DefaultBlocksPerSession::get(),
			);
			<Round<T>>::put(round);
			// Set productivity rate per block
			let blocks_per_validator = {
				if v_count == 0 {
					0u32
				} else {
					(round.round_length / v_count) + 1
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
			// Snapshot total stake
			<Staked<T>>::insert(1u32, <Total<T>>::get());
			<TotalAtStake<T>>::insert(1u32, TotalSnapshot::default());
			<Pallet<T>>::deposit_event(Event::NewRound {
				starting_block: T::BlockNumber::zero(),
				round: 1u32,
				selected_validators_number: v_count,
				total_balance: total_staked,
			});
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::set_staking_expectations())]
		/// Set the expectations for total staked. These expectations determine the issuance for
		/// the round according to logic in `fn compute_issuance`
		pub fn set_staking_expectations(
			origin: OriginFor<T>,
			expectations: Range<BalanceOf<T>>,
		) -> DispatchResultWithPostInfo {
			T::MonetaryGovernanceOrigin::ensure_origin(origin)?;
			ensure!(expectations.is_valid(), Error::<T>::InvalidSchedule);
			let mut config = <InflationConfig<T>>::get();
			ensure!(config.expect != expectations, Error::<T>::NoWritingSameValue);
			config.set_expectations(expectations);
			<InflationConfig<T>>::put(&config);
			Self::deposit_event(Event::StakeExpectationsSet {
				expect_min: config.expect.min,
				expect_ideal: config.expect.ideal,
				expect_max: config.expect.max,
			});
			Ok(().into())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::set_inflation())]
		/// Set the annual inflation rate to derive per-round inflation
		pub fn set_inflation(
			origin: OriginFor<T>,
			schedule: Range<Perbill>,
		) -> DispatchResultWithPostInfo {
			T::MonetaryGovernanceOrigin::ensure_origin(origin)?;
			ensure!(schedule.is_valid(), Error::<T>::InvalidSchedule);
			let mut config = <InflationConfig<T>>::get();
			ensure!(config.annual != schedule, Error::<T>::NoWritingSameValue);
			config.annual = schedule;
			config.set_round_from_annual::<T>(schedule);
			<InflationConfig<T>>::put(&config);
			Self::deposit_event(Event::InflationSet {
				annual_min: config.annual.min,
				annual_ideal: config.annual.ideal,
				annual_max: config.annual.max,
				round_min: config.round.min,
				round_ideal: config.round.ideal,
				round_max: config.round.max,
			});
			Ok(().into())
		}

		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::set_max_total_selected())]
		/// Set the maximum number of full validator candidates selected per round
		pub fn set_max_full_selected(origin: OriginFor<T>, new: u32) -> DispatchResultWithPostInfo {
			frame_system::ensure_root(origin)?;
			ensure!(new >= <MinTotalSelected<T>>::get(), Error::<T>::CannotSetBelowMin);
			let old = <MaxFullSelected<T>>::get();
			ensure!(old != new, Error::<T>::NoWritingSameValue);
			ensure!(
				new <= <Round<T>>::get().round_length,
				Error::<T>::RoundLengthMustBeAtLeastTotalSelectedValidators,
			);
			<MaxFullSelected<T>>::put(new);
			<MaxTotalSelected<T>>::put(new + <MaxBasicSelected<T>>::get());
			Self::deposit_event(Event::MaxFullSelectedSet { old, new });
			Ok(().into())
		}

		#[pallet::call_index(3)]
		#[pallet::weight(<T as Config>::WeightInfo::set_max_total_selected())]
		/// Set the maximum number of basic validator candidates selected per round
		pub fn set_max_basic_selected(
			origin: OriginFor<T>,
			new: u32,
		) -> DispatchResultWithPostInfo {
			frame_system::ensure_root(origin)?;
			ensure!(new >= <MinTotalSelected<T>>::get(), Error::<T>::CannotSetBelowMin);
			let old = <MaxBasicSelected<T>>::get();
			ensure!(old != new, Error::<T>::NoWritingSameValue);
			ensure!(
				new <= <Round<T>>::get().round_length,
				Error::<T>::RoundLengthMustBeAtLeastTotalSelectedValidators,
			);
			<MaxBasicSelected<T>>::put(new);
			<MaxTotalSelected<T>>::put(new + <MaxFullSelected<T>>::get());
			Self::deposit_event(Event::MaxBasicSelectedSet { old, new });
			Ok(().into())
		}

		#[pallet::call_index(4)]
		#[pallet::weight(<T as Config>::WeightInfo::set_min_total_selected())]
		/// Set the minimum number of validator candidates selected per round
		pub fn set_min_total_selected(
			origin: OriginFor<T>,
			new: u32,
		) -> DispatchResultWithPostInfo {
			frame_system::ensure_root(origin)?;
			ensure!(new <= <MaxTotalSelected<T>>::get(), Error::<T>::CannotSetAboveMax);
			let old = <MinTotalSelected<T>>::get();
			ensure!(old != new, Error::<T>::NoWritingSameValue);
			ensure!(
				new <= <Round<T>>::get().round_length,
				Error::<T>::RoundLengthMustBeAtLeastTotalSelectedValidators,
			);
			<MinTotalSelected<T>>::put(new);
			Self::deposit_event(Event::MinTotalSelectedSet { old, new });
			Ok(().into())
		}

		#[pallet::call_index(5)]
		#[pallet::weight(<T as Config>::WeightInfo::set_default_validator_commission())]
		/// Set the default commission rate for all validators of the given tier
		pub fn set_default_validator_commission(
			origin: OriginFor<T>,
			new: Perbill,
			tier: TierType,
		) -> DispatchResultWithPostInfo {
			frame_system::ensure_root(origin)?;
			match tier {
				TierType::Full => {
					let old = <DefaultFullValidatorCommission<T>>::get();
					ensure!(old != new, Error::<T>::NoWritingSameValue);
					let max = <MaxFullValidatorCommission<T>>::get();
					ensure!(new <= max, Error::<T>::CannotSetAboveMax);
					<DefaultFullValidatorCommission<T>>::put(new);
					Self::deposit_event(Event::DefaultValidatorCommissionSet { old, new, tier });
				},
				TierType::Basic => {
					let old = <DefaultBasicValidatorCommission<T>>::get();
					ensure!(old != new, Error::<T>::NoWritingSameValue);
					let max = <MaxBasicValidatorCommission<T>>::get();
					ensure!(new <= max, Error::<T>::CannotSetAboveMax);
					<DefaultBasicValidatorCommission<T>>::put(new);
					Self::deposit_event(Event::DefaultValidatorCommissionSet { old, new, tier });
				},
				TierType::All => {
					let old_full = <DefaultFullValidatorCommission<T>>::get();
					ensure!(old_full != new, Error::<T>::NoWritingSameValue);
					let max_full = <MaxFullValidatorCommission<T>>::get();
					ensure!(new <= max_full, Error::<T>::CannotSetAboveMax);

					let old_basic = <DefaultBasicValidatorCommission<T>>::get();
					ensure!(old_basic != new, Error::<T>::NoWritingSameValue);
					let max_basic = <MaxBasicValidatorCommission<T>>::get();
					ensure!(new <= max_basic, Error::<T>::CannotSetAboveMax);

					<DefaultFullValidatorCommission<T>>::put(new);
					<DefaultBasicValidatorCommission<T>>::put(new);

					Self::deposit_event(Event::DefaultValidatorCommissionSet {
						old: old_full,
						new,
						tier: TierType::Full,
					});
					Self::deposit_event(Event::DefaultValidatorCommissionSet {
						old: old_basic,
						new,
						tier: TierType::Basic,
					});
				},
			}
			Ok(().into())
		}

		#[pallet::call_index(6)]
		#[pallet::weight(<T as Config>::WeightInfo::set_max_validator_commission())]
		/// Set the maximum commission rate for all validators of the given tier
		pub fn set_max_validator_commission(
			origin: OriginFor<T>,
			new: Perbill,
			tier: TierType,
		) -> DispatchResultWithPostInfo {
			frame_system::ensure_root(origin)?;
			match tier {
				TierType::Full => {
					let old = <MaxFullValidatorCommission<T>>::get();
					ensure!(old != new, Error::<T>::NoWritingSameValue);
					<MaxFullValidatorCommission<T>>::put(new);
					Self::deposit_event(Event::MaxValidatorCommissionSet { old, new, tier });
				},
				TierType::Basic => {
					let old = <MaxBasicValidatorCommission<T>>::get();
					ensure!(old != new, Error::<T>::NoWritingSameValue);
					<MaxBasicValidatorCommission<T>>::put(new);
					Self::deposit_event(Event::MaxValidatorCommissionSet { old, new, tier });
				},
				TierType::All => {
					let old_full = <MaxFullValidatorCommission<T>>::get();
					ensure!(old_full != new, Error::<T>::NoWritingSameValue);

					let old_basic = <MaxBasicValidatorCommission<T>>::get();
					ensure!(old_basic != new, Error::<T>::NoWritingSameValue);

					<MaxFullValidatorCommission<T>>::put(new);
					<MaxBasicValidatorCommission<T>>::put(new);

					Self::deposit_event(Event::MaxValidatorCommissionSet {
						old: old_full,
						new,
						tier,
					});
					Self::deposit_event(Event::MaxValidatorCommissionSet {
						old: old_basic,
						new,
						tier,
					});
				},
			}
			Ok(().into())
		}

		#[pallet::call_index(7)]
		#[pallet::weight(<T as Config>::WeightInfo::set_validator_commission())]
		/// Set the commission rate of the given validator
		/// - origin should be the controller account
		pub fn set_validator_commission(
			origin: OriginFor<T>,
			new: Perbill,
		) -> DispatchResultWithPostInfo {
			let controller = ensure_signed(origin)?;
			let state = <CandidateInfo<T>>::get(&controller).ok_or(Error::<T>::CandidateDNE)?;
			let old = state.commission;
			ensure!(old != new, Error::<T>::NoWritingSameValue);
			let max = match state.tier {
				TierType::Full => <MaxFullValidatorCommission<T>>::get(),
				_ => <MaxBasicValidatorCommission<T>>::get(),
			};
			ensure!(new <= max, Error::<T>::CannotSetAboveMax);
			ensure!(
				!Self::is_commission_set_requested(&controller),
				Error::<T>::AlreadyCommissionSetRequested,
			);
			Self::add_to_commission_sets(&controller, old, new);
			Self::deposit_event(Event::ValidatorCommissionSet { candidate: controller, old, new });
			Ok(().into())
		}

		#[pallet::call_index(8)]
		#[pallet::weight(<T as Config>::WeightInfo::cancel_validator_commission_set())]
		/// Cancel the request for (re-)setting the commission rate.
		/// - origin should be the controller account.
		pub fn cancel_validator_commission_set(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let controller = ensure_signed(origin)?;
			ensure!(Self::is_candidate(&controller, TierType::All), Error::<T>::CandidateDNE);
			ensure!(Self::is_commission_set_requested(&controller), Error::<T>::CommissionSetDNE);
			Self::remove_commission_set(&controller);
			Self::deposit_event(Event::ValidatorCommissionSetCancelled { candidate: controller });
			Ok(().into())
		}

		#[pallet::call_index(9)]
		#[pallet::weight(<T as Config>::WeightInfo::set_validator_tier())]
		/// Modify validator candidate tier. The actual state reflection will apply at the next
		/// round
		/// - origin should be the stash account
		pub fn set_validator_tier(
			origin: OriginFor<T>,
			more: BalanceOf<T>,
			new: TierType,
			relayer: Option<T::AccountId>,
		) -> DispatchResultWithPostInfo {
			let stash = ensure_signed(origin)?;
			let controller = Self::bonded_stash(&stash).ok_or(Error::<T>::StashDNE)?;
			let mut state = <CandidateInfo<T>>::get(&controller).ok_or(Error::<T>::CandidateDNE)?;
			let old = state.tier;
			ensure!(old != new, Error::<T>::NoWritingSameValue);
			if let Some(relayer) = relayer {
				ensure!(new == TierType::Full, Error::<T>::InvalidTierType);
				ensure!(
					state.bond + more >= T::MinFullCandidateStk::get(),
					Error::<T>::CandidateBondBelowMin
				);
				// check that caller can reserve the amount before any changes to storage
				ensure!(T::Currency::can_reserve(&stash, more), Error::<T>::InsufficientBalance);
				state
					.bond_more::<T>(stash.clone(), controller.clone(), more)
					.and_then(|_| T::RelayManager::join_relayers(relayer, controller.clone()))?;
			} else {
				ensure!(new == TierType::Basic, Error::<T>::InvalidTierType);
				ensure!(
					state.bond + more >= T::MinBasicCandidateStk::get(),
					Error::<T>::CandidateBondBelowMin
				);
				state.bond_more::<T>(stash.clone(), controller.clone(), more)?;
				T::RelayManager::leave_relayers(&controller);
			}
			state.tier = new;
			state.reset_commission::<T>();
			<CandidateInfo<T>>::insert(&controller, state.clone());
			Self::update_active(&controller, state.voting_power);
			Self::sort_candidates_by_voting_power();
			Ok(().into())
		}

		#[pallet::call_index(10)]
		#[pallet::weight(<T as Config>::WeightInfo::set_blocks_per_round())]
		/// Set blocks per round
		/// - the `new` round length will be updated immediately in the next block
		/// - also updates per-round inflation config
		pub fn set_blocks_per_round(origin: OriginFor<T>, new: u32) -> DispatchResultWithPostInfo {
			frame_system::ensure_root(origin)?;
			ensure!(new >= T::MinBlocksPerRound::get(), Error::<T>::CannotSetBelowMin);
			let mut round = <Round<T>>::get();
			let (current_round, now, first, old) = (
				round.current_round_index,
				round.current_block,
				round.first_round_block,
				round.round_length,
			);
			ensure!(old != new, Error::<T>::NoWritingSameValue);
			ensure!(
				now - first < T::BlockNumber::from(new),
				Error::<T>::RoundLengthMustBeLongerThanCreatedBlocks,
			);
			ensure!(
				new >= <MaxTotalSelected<T>>::get(),
				Error::<T>::RoundLengthMustBeAtLeastTotalSelectedValidators,
			);
			round.round_length = new;
			// update per-round inflation given new rounds per year
			let mut inflation_config = <InflationConfig<T>>::get();
			inflation_config.reset_round(new);
			<Round<T>>::put(round);
			<InflationConfig<T>>::put(&inflation_config);
			Self::deposit_event(Event::BlocksPerRoundSet {
				current_round,
				first_block: first,
				old,
				new,
				new_per_round_inflation_min: inflation_config.round.min,
				new_per_round_inflation_ideal: inflation_config.round.ideal,
				new_per_round_inflation_max: inflation_config.round.max,
			});
			Ok(().into())
		}

		#[pallet::call_index(11)]
		#[pallet::weight(<T as Config>::WeightInfo::set_storage_cache_lifetime())]
		/// Set the `StorageCacheLifetime` round length
		pub fn set_storage_cache_lifetime(
			origin: OriginFor<T>,
			new: u32,
		) -> DispatchResultWithPostInfo {
			frame_system::ensure_root(origin)?;
			ensure!(new >= 1u32, Error::<T>::CannotSetBelowOne);
			let old = <StorageCacheLifetime<T>>::get();
			ensure!(old != new, Error::<T>::NoWritingSameValue);
			<StorageCacheLifetime<T>>::put(new);
			Self::deposit_event(Event::StorageCacheLifetimeSet { old, new });
			Ok(().into())
		}

		#[pallet::call_index(12)]
		#[pallet::weight(<T as Config>::WeightInfo::join_candidates(*candidate_count))]
		/// Join the set of validator candidates
		/// - origin should be the stash account
		pub fn join_candidates(
			origin: OriginFor<T>,
			controller: T::AccountId,
			relayer: Option<T::AccountId>,
			bond: BalanceOf<T>,
			candidate_count: u32,
		) -> DispatchResultWithPostInfo {
			let stash = ensure_signed(origin)?;

			// account duplicate check
			ensure!(!<BondedStash<T>>::contains_key(&stash), Error::<T>::AlreadyBonded);
			ensure!(!<CandidateInfo<T>>::contains_key(&controller), Error::<T>::AlreadyPaired);

			ensure!(!Self::is_nominator(&controller), Error::<T>::NominatorExists);
			let mut candidates = <CandidatePool<T>>::get();
			let old_count = candidates.len() as u32;
			ensure!(
				candidate_count >= old_count,
				Error::<T>::TooLowCandidateCountWeightHintJoinCandidates
			);
			// check that caller can reserve the amount before any changes to storage
			ensure!(T::Currency::can_reserve(&stash, bond), Error::<T>::InsufficientBalance);

			let mut tier = TierType::Basic;
			if let Some(relayer) = relayer {
				ensure!(bond >= T::MinFullCandidateStk::get(), Error::<T>::CandidateBondBelowMin);
				// join the set of relayers
				T::Currency::reserve(&stash, bond)
					.and_then(|_| T::RelayManager::join_relayers(relayer, controller.clone()))?;
				tier = TierType::Full;
			} else {
				ensure!(bond >= T::MinBasicCandidateStk::get(), Error::<T>::CandidateBondBelowMin);
				T::Currency::reserve(&stash, bond)?;
			}

			let candidate = CandidateMetadata::new::<T>(stash.clone(), bond, tier);
			<CandidateInfo<T>>::insert(&controller, candidate);
			<BondedStash<T>>::insert(&stash, controller.clone());
			let empty_nominations: Nominations<T::AccountId, BalanceOf<T>> = Default::default();
			// insert empty top nominations
			<TopNominations<T>>::insert(&controller, empty_nominations.clone());
			// insert empty bottom nominations
			<BottomNominations<T>>::insert(&controller, empty_nominations);
			candidates
				.try_push(Bond { owner: controller.clone(), amount: bond })
				.map_err(|_| Error::<T>::TooManyCandidates)?;
			<CandidatePool<T>>::put(candidates);
			Self::sort_candidates_by_voting_power();
			let new_total = <Total<T>>::get().saturating_add(bond);
			<Total<T>>::put(new_total);
			Self::deposit_event(Event::JoinedValidatorCandidates {
				account: controller,
				amount_locked: bond,
				new_total_amt_locked: new_total,
			});
			Ok(().into())
		}

		#[pallet::call_index(13)]
		#[pallet::weight(<T as Config>::WeightInfo::schedule_leave_candidates(*candidate_count))]
		/// Request to leave the set of candidates. If successful, the account is immediately
		/// removed from the candidate pool to prevent selection as a validator.
		/// - origin should be the controller account
		pub fn schedule_leave_candidates(
			origin: OriginFor<T>,
			candidate_count: u32,
		) -> DispatchResultWithPostInfo {
			let controller = ensure_signed(origin)?;
			let mut state = <CandidateInfo<T>>::get(&controller).ok_or(Error::<T>::CandidateDNE)?;
			let (now, when) = state.schedule_leave::<T>()?;
			let candidates = <CandidatePool<T>>::get();
			ensure!(
				candidate_count >= candidates.len() as u32,
				Error::<T>::TooLowCandidateCountToLeaveCandidates,
			);
			ensure!(
				Self::remove_from_candidate_pool(&controller),
				Error::<T>::CannotLeaveIfOffline,
			);
			<CandidateInfo<T>>::insert(&controller, state);
			Self::deposit_event(Event::CandidateScheduledExit {
				exit_allowed_round: now,
				candidate: controller,
				scheduled_exit: when,
			});
			Ok(().into())
		}

		#[pallet::call_index(14)]
		#[pallet::weight(
			<T as Config>::WeightInfo::execute_leave_candidates(*candidate_nomination_count)
		)]
		/// Execute leave candidates request
		/// - origin should be the stash account
		pub fn execute_leave_candidates(
			origin: OriginFor<T>,
			candidate_nomination_count: u32,
		) -> DispatchResultWithPostInfo {
			let stash = ensure_signed(origin)?;
			let controller = Self::bonded_stash(&stash).ok_or(Error::<T>::StashDNE)?;
			let state = <CandidateInfo<T>>::get(&controller).ok_or(Error::<T>::CandidateDNE)?;
			ensure!(
				state.nomination_count <= candidate_nomination_count,
				Error::<T>::TooLowCandidateNominationCountToLeaveCandidates
			);
			state.can_leave::<T>()?;
			let return_stake = |bond: Bond<T::AccountId, BalanceOf<T>>| {
				T::Currency::unreserve(&bond.owner, bond.amount);
				// remove nomination from nominator state
				let mut nominator = NominatorState::<T>::get(&bond.owner).expect(
					"Validator state and nominator state are consistent.
						Validator state has a record of this nomination. Therefore,
						Nominator state also has a record. qed.",
				);
				if let Some(remaining) = nominator.rm_nomination(&controller) {
					if remaining.is_zero() {
						<NominatorState<T>>::remove(&bond.owner);
					} else {
						if let Some(request) = nominator.requests.requests.remove(&controller) {
							nominator.requests.less_total =
								nominator.requests.less_total.saturating_sub(request.amount);
							if matches!(request.action, NominationChange::Revoke) {
								nominator.requests.revocations_count =
									nominator.requests.revocations_count.saturating_sub(1u32);
							}
						}
						<NominatorState<T>>::insert(&bond.owner, nominator);
					}
				}
			};
			// total backing stake is at least the candidate self bond
			let mut total_backing = state.bond;
			// return all top nominations
			let top_nominations =
				<TopNominations<T>>::take(&controller).expect("CandidateInfo existence checked");
			for bond in top_nominations.nominations {
				return_stake(bond);
			}
			total_backing += top_nominations.total;
			// return all bottom nominations
			let bottom_nominations =
				<BottomNominations<T>>::take(&controller).expect("CandidateInfo existence checked");
			for bond in bottom_nominations.nominations {
				return_stake(bond);
			}
			total_backing += bottom_nominations.total;
			// return stake to stash account
			T::Currency::unreserve(&stash, state.bond);
			<CandidateInfo<T>>::remove(&controller);
			<BondedStash<T>>::remove(&stash);
			<TopNominations<T>>::remove(&controller);
			<BottomNominations<T>>::remove(&controller);
			let new_total_staked = <Total<T>>::get().saturating_sub(total_backing);
			<Total<T>>::put(new_total_staked);

			// remove relayer from pool
			T::RelayManager::leave_relayers(&controller);

			Self::deposit_event(Event::CandidateLeft {
				ex_candidate: controller,
				unlocked_amount: total_backing,
				new_total_amt_locked: new_total_staked,
			});
			Ok(().into())
		}

		#[pallet::call_index(15)]
		#[pallet::weight(<T as Config>::WeightInfo::cancel_leave_candidates(*candidate_count))]
		/// Cancel open request to leave candidates
		/// - only callable by validator account
		/// - result upon successful call is the candidate is active in the candidate pool
		/// - origin should be the controller account
		pub fn cancel_leave_candidates(
			origin: OriginFor<T>,
			candidate_count: u32,
		) -> DispatchResultWithPostInfo {
			let controller = ensure_signed(origin)?;
			let mut state = <CandidateInfo<T>>::get(&controller).ok_or(Error::<T>::CandidateDNE)?;
			ensure!(state.is_leaving(), Error::<T>::CandidateNotLeaving);
			state.go_online();
			let mut candidates = <CandidatePool<T>>::get();
			ensure!(
				candidates.len() as u32 <= candidate_count,
				Error::<T>::TooLowCandidateCountWeightHintCancelLeaveCandidates,
			);
			candidates
				.try_push(Bond { owner: controller.clone(), amount: state.voting_power })
				.map_err(|_| Error::<T>::TooManyCandidates)?;
			<CandidatePool<T>>::put(candidates);
			<CandidateInfo<T>>::insert(&controller, state);
			Self::sort_candidates_by_voting_power();
			Self::deposit_event(Event::CancelledCandidateExit { candidate: controller });
			Ok(().into())
		}

		#[pallet::call_index(16)]
		#[pallet::weight(<T as Config>::WeightInfo::set_controller())]
		/// (Re-)set the bonded controller account. The origin must be the bonded stash account. The
		/// actual change will apply on the next round update.
		/// - origin should be the stash account
		pub fn set_controller(
			origin: OriginFor<T>,
			new: T::AccountId,
		) -> DispatchResultWithPostInfo {
			let stash = ensure_signed(origin)?;
			let old = Self::bonded_stash(&stash).ok_or(Error::<T>::StashDNE)?;
			ensure!(new != old, Error::<T>::NoWritingSameValue);
			ensure!(!Self::is_candidate(&new, TierType::All), Error::<T>::AlreadyPaired);
			ensure!(
				!Self::is_controller_set_requested(old.clone()),
				Error::<T>::AlreadyControllerSetRequested
			);
			Self::add_to_controller_sets(stash, old.clone(), new.clone());
			Self::deposit_event(Event::ControllerSet { old: old.clone(), new: new.clone() });
			Ok(().into())
		}

		#[pallet::call_index(17)]
		#[pallet::weight(<T as Config>::WeightInfo::cancel_controller_set())]
		/// Cancel the request for (re-)setting the bonded controller account.
		/// - origin should be the controller account.
		pub fn cancel_controller_set(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let controller = ensure_signed(origin)?;
			ensure!(Self::is_candidate(&controller, TierType::All), Error::<T>::CandidateDNE);
			ensure!(
				Self::is_controller_set_requested(controller.clone()),
				Error::<T>::ControllerSetDNE
			);
			Self::remove_controller_set(&controller);
			Self::deposit_event(Event::ControllerSetCancelled { candidate: controller });
			Ok(().into())
		}

		#[pallet::call_index(18)]
		#[pallet::weight(<T as Config>::WeightInfo::set_candidate_reward_dst())]
		/// Set the validator candidate reward destination
		/// - origin should be the controller account
		pub fn set_candidate_reward_dst(
			origin: OriginFor<T>,
			new_reward_dst: RewardDestination,
		) -> DispatchResultWithPostInfo {
			let controller = ensure_signed(origin)?;
			let mut state = <CandidateInfo<T>>::get(&controller).ok_or(Error::<T>::CandidateDNE)?;
			let old_reward_dst = state.reward_dst;
			ensure!(old_reward_dst != new_reward_dst, Error::<T>::NoWritingSameValue);
			state.set_reward_dst(new_reward_dst);
			<CandidateInfo<T>>::insert(&controller, state);
			Self::deposit_event(Event::ValidatorRewardDstSet {
				candidate: controller,
				old: old_reward_dst,
				new: new_reward_dst,
			});
			Ok(().into())
		}

		#[pallet::call_index(19)]
		#[pallet::weight(<T as Config>::WeightInfo::set_nominator_reward_dst())]
		/// Set the nominator reward destination
		pub fn set_nominator_reward_dst(
			origin: OriginFor<T>,
			new_reward_dst: RewardDestination,
		) -> DispatchResultWithPostInfo {
			let nominator = ensure_signed(origin)?;
			let mut state = <NominatorState<T>>::get(&nominator).ok_or(Error::<T>::NominatorDNE)?;
			let old_reward_dst = state.reward_dst;
			ensure!(old_reward_dst != new_reward_dst, Error::<T>::NoWritingSameValue);
			state.set_reward_dst(new_reward_dst);
			<NominatorState<T>>::insert(&nominator, state);
			Self::deposit_event(Event::NominatorRewardDstSet {
				nominator,
				old: old_reward_dst,
				new: new_reward_dst,
			});
			Ok(().into())
		}

		#[pallet::call_index(20)]
		#[pallet::weight(<T as Config>::WeightInfo::go_offline())]
		/// Temporarily leave the set of validator candidates without unbonding
		/// - removed from candidate pool
		/// - removed from selected candidates if contained
		/// - removed from cached selected candidates if contained
		/// - removed from selected relayers if contained
		/// - removed from cached selected relayers if contained
		/// - state changed to `Idle`
		/// - it will be completely removed from session validators after one session
		/// - origin should be the controller account
		pub fn go_offline(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let controller = ensure_signed(origin)?;
			let mut state = <CandidateInfo<T>>::get(&controller).ok_or(Error::<T>::CandidateDNE)?;
			ensure!(state.is_active(), Error::<T>::AlreadyOffline);
			ensure!(Self::remove_from_candidate_pool(&controller), Error::<T>::AlreadyOffline,);
			let mut selected_candidates = SelectedCandidates::<T>::get();
			selected_candidates.retain(|v| *v != controller);
			// refresh selected candidates
			let round = <Round<T>>::get();
			let mut cached_selected_candidates = <CachedSelectedCandidates<T>>::get();
			cached_selected_candidates.retain(|r| r.0 != round.current_round_index);
			cached_selected_candidates
				.push((round.current_round_index, selected_candidates.clone().into_inner()));
			<CachedSelectedCandidates<T>>::put(cached_selected_candidates);
			// refresh majority
			let majority: u32 = Self::compute_majority();
			<Majority<T>>::put(majority);
			let mut cached_majority = <CachedMajority<T>>::get();
			cached_majority.retain(|r| r.0 != round.current_round_index);
			cached_majority.push((round.current_round_index, majority));
			<CachedMajority<T>>::put(cached_majority);
			if state.tier == TierType::Full {
				// kickout relayer
				T::RelayManager::kickout_relayer(&controller);
				// refresh selected full candidates
				let mut selected_full_candidates = <SelectedFullCandidates<T>>::get();
				selected_full_candidates.retain(|c| *c != controller);
				<SelectedFullCandidates<T>>::put(selected_full_candidates);
			} else {
				// refresh selected basic candidates
				let mut selected_basic_candidates = <SelectedBasicCandidates<T>>::get();
				selected_basic_candidates.retain(|c| *c != controller);
				<SelectedBasicCandidates<T>>::put(selected_basic_candidates);
			}
			state.go_offline();
			<CandidateInfo<T>>::insert(&controller, state);
			SelectedCandidates::<T>::put(selected_candidates);
			Self::deposit_event(Event::CandidateWentOffline { candidate: controller });
			Ok(().into())
		}

		#[pallet::call_index(21)]
		#[pallet::weight(<T as Config>::WeightInfo::go_online())]
		/// Rejoin the set of validator candidates if previously been kicked out or went offline
		/// - state changed to `Active`
		/// - origin should be the controller account
		pub fn go_online(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let controller = ensure_signed(origin)?;
			let mut state = <CandidateInfo<T>>::get(&controller).ok_or(Error::<T>::CandidateDNE)?;

			// Check for safety
			ensure!(!state.is_active(), Error::<T>::AlreadyActive);
			ensure!(!state.is_leaving(), Error::<T>::CannotGoOnlineIfLeaving);
			state.go_online();
			let mut candidates = <CandidatePool<T>>::get();
			candidates
				.try_push(Bond { owner: controller.clone(), amount: state.voting_power })
				.map_err(|_| Error::<T>::TooManyCandidates)?;
			<CandidatePool<T>>::put(candidates);
			<CandidateInfo<T>>::insert(&controller, state);
			Self::deposit_event(Event::CandidateBackOnline { candidate: controller });
			Ok(().into())
		}

		#[pallet::call_index(22)]
		#[pallet::weight(<T as Config>::WeightInfo::candidate_bond_more())]
		/// Increase validator candidate self bond by `more`
		/// - origin should be the stash account
		pub fn candidate_bond_more(
			origin: OriginFor<T>,
			more: BalanceOf<T>,
		) -> DispatchResultWithPostInfo {
			let stash = ensure_signed(origin)?;
			let controller = Self::bonded_stash(&stash).ok_or(Error::<T>::StashDNE)?;
			let mut state = <CandidateInfo<T>>::get(&controller).ok_or(Error::<T>::CandidateDNE)?;
			// check that caller can reserve the amount before any changes to storage
			ensure!(T::Currency::can_reserve(&stash, more), Error::<T>::InsufficientBalance);
			state.bond_more::<T>(stash.clone(), controller.clone(), more)?;
			<CandidateInfo<T>>::insert(&controller, state.clone());
			Self::update_active(&controller, state.voting_power);
			Self::sort_candidates_by_voting_power();
			Ok(().into())
		}

		#[pallet::call_index(23)]
		#[pallet::weight(<T as Config>::WeightInfo::schedule_candidate_bond_less())]
		/// Request by validator candidate to decrease self bond by `less`
		/// - origin should be the controller account
		pub fn schedule_candidate_bond_less(
			origin: OriginFor<T>,
			less: BalanceOf<T>,
		) -> DispatchResultWithPostInfo {
			let controller = ensure_signed(origin)?;
			let mut state = <CandidateInfo<T>>::get(&controller).ok_or(Error::<T>::CandidateDNE)?;
			ensure!(!state.is_leaving(), Error::<T>::CandidateAlreadyLeaving);
			let when = state.schedule_bond_less::<T>(less)?;
			<CandidateInfo<T>>::insert(&controller, state);
			Self::deposit_event(Event::CandidateBondLessRequested {
				candidate: controller,
				amount_to_decrease: less,
				execute_round: when,
			});
			Ok(().into())
		}

		#[pallet::call_index(24)]
		#[pallet::weight(<T as Config>::WeightInfo::execute_candidate_bond_less())]
		/// Execute pending request to adjust the validator candidate self bond
		/// - origin should be the stash account
		pub fn execute_candidate_bond_less(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let stash = ensure_signed(origin)?;
			let controller = Self::bonded_stash(&stash).ok_or(Error::<T>::StashDNE)?;
			let mut state = <CandidateInfo<T>>::get(&controller).ok_or(Error::<T>::CandidateDNE)?;
			state.execute_bond_less::<T>(stash.clone(), controller.clone())?;
			<CandidateInfo<T>>::insert(&controller, state);
			Self::sort_candidates_by_voting_power();
			Ok(().into())
		}

		#[pallet::call_index(25)]
		#[pallet::weight(<T as Config>::WeightInfo::cancel_candidate_bond_less())]
		/// Cancel pending request to adjust the validator candidate self bond
		/// - origin should be the controller account
		pub fn cancel_candidate_bond_less(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let controller = ensure_signed(origin)?;
			let mut state = <CandidateInfo<T>>::get(&controller).ok_or(Error::<T>::CandidateDNE)?;
			state.cancel_bond_less::<T>(controller.clone())?;
			<CandidateInfo<T>>::insert(&controller, state);
			Ok(().into())
		}

		#[pallet::call_index(26)]
		#[pallet::weight(
			<T as Config>::WeightInfo::nominate(
				*candidate_nomination_count,
				*nomination_count
			)
		)]
		/// If caller is not a nominator and not a validator, then join the set of nominators
		/// If caller is a nominator, then makes nomination to change their nomination state
		pub fn nominate(
			origin: OriginFor<T>,
			candidate: T::AccountId,
			amount: BalanceOf<T>,
			// will_be_in_top: bool // weight hint
			// look into returning weight in DispatchResult
			candidate_nomination_count: u32,
			nomination_count: u32,
		) -> DispatchResultWithPostInfo {
			let nominator = ensure_signed(origin)?;
			// check that caller can reserve the amount before any changes to storage
			ensure!(T::Currency::can_reserve(&nominator, amount), Error::<T>::InsufficientBalance);
			let nominator_state = if let Some(mut state) = <NominatorState<T>>::get(&nominator) {
				ensure!(state.is_active(), Error::<T>::CannotNominateIfLeaving);
				// nomination after first
				ensure!(amount >= T::MinNomination::get(), Error::<T>::NominationBelowMin);
				ensure!(
					nomination_count >= state.nominations.0.len() as u32,
					Error::<T>::TooLowNominationCountToNominate
				);
				ensure!(
					(state.nominations.0.len() as u32) < T::MaxNominationsPerNominator::get(),
					Error::<T>::ExceedMaxNominationsPerNominator
				);
				ensure!(
					state.add_nomination(Bond { owner: candidate.clone(), amount }),
					Error::<T>::AlreadyNominatedCandidate
				);
				state
			} else {
				// first nomination
				ensure!(amount >= T::MinNominatorStk::get(), Error::<T>::NominatorBondBelowMin);
				ensure!(
					!Self::is_candidate(&nominator, TierType::All),
					Error::<T>::CandidateExists
				);
				Nominator::new(nominator.clone(), candidate.clone(), amount)
			};
			let mut state = <CandidateInfo<T>>::get(&candidate).ok_or(Error::<T>::CandidateDNE)?;
			ensure!(
				candidate_nomination_count >= state.nomination_count,
				Error::<T>::TooLowCandidateNominationCountToNominate
			);
			let (nominator_position, less_total_staked) =
				state.add_nomination::<T>(&candidate, Bond { owner: nominator.clone(), amount })?;
			T::Currency::reserve(&nominator, amount)
				.expect("verified can reserve at top of this extrinsic body");
			// only is_some if kicked the lowest bottom as a consequence of this new nomination
			let net_total_increase =
				if let Some(less) = less_total_staked { amount - less } else { amount };
			let new_total_locked = <Total<T>>::get() + net_total_increase;
			<Total<T>>::put(new_total_locked);
			<CandidateInfo<T>>::insert(&candidate, state);
			<NominatorState<T>>::insert(&nominator, nominator_state);
			Self::sort_candidates_by_voting_power();
			Self::deposit_event(Event::Nomination {
				nominator,
				locked_amount: amount,
				candidate,
				nominator_position,
			});
			Ok(().into())
		}

		#[pallet::call_index(27)]
		#[pallet::weight(<T as Config>::WeightInfo::schedule_leave_nominators())]
		/// Request to leave the set of nominators. If successful, the caller is scheduled
		/// to be allowed to exit. Success forbids future nominator actions until the request is
		/// invoked or cancelled.
		pub fn schedule_leave_nominators(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let acc = ensure_signed(origin)?;
			let mut state = <NominatorState<T>>::get(&acc).ok_or(Error::<T>::NominatorDNE)?;
			ensure!(!state.is_leaving(), Error::<T>::NominatorAlreadyLeaving);
			ensure!(state.requests().is_empty(), Error::<T>::PendingNominationRequestAlreadyExists);
			let (now, when) = state.schedule_leave::<T>();
			<NominatorState<T>>::insert(&acc, state);
			Self::deposit_event(Event::NominatorExitScheduled {
				round: now,
				nominator: acc,
				scheduled_exit: when,
			});
			Ok(().into())
		}

		#[pallet::call_index(28)]
		#[pallet::weight(<T as Config>::WeightInfo::execute_leave_nominators(*nomination_count))]
		/// Execute the right to exit the set of nominators and revoke all ongoing nominations.
		pub fn execute_leave_nominators(
			origin: OriginFor<T>,
			nomination_count: u32,
		) -> DispatchResultWithPostInfo {
			let nominator = ensure_signed(origin)?;
			let state = <NominatorState<T>>::get(&nominator).ok_or(Error::<T>::NominatorDNE)?;
			state.can_execute_leave::<T>(nomination_count)?;
			for bond in state.nominations.0 {
				if let Err(error) = Self::nominator_leaves_candidate(
					bond.owner.clone(),
					nominator.clone(),
					bond.amount,
				) {
					log::warn!(
						"STORAGE CORRUPTED \nNominator leaving validator failed with error: {:?}",
						error
					);
				}
			}
			<NominatorState<T>>::remove(&nominator);
			Self::sort_candidates_by_voting_power();
			Self::deposit_event(Event::NominatorLeft { nominator, unstaked_amount: state.total });
			Ok(().into())
		}

		#[pallet::call_index(29)]
		#[pallet::weight(<T as Config>::WeightInfo::cancel_leave_nominators())]
		/// Cancel a pending request to exit the set of nominators. Success clears the pending exit
		/// request (thereby resetting the delay upon another `leave_nominators` call).
		pub fn cancel_leave_nominators(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let nominator = ensure_signed(origin)?;
			// ensure nominator state exists
			let mut state = <NominatorState<T>>::get(&nominator).ok_or(Error::<T>::NominatorDNE)?;
			// ensure state is leaving
			ensure!(state.is_leaving(), Error::<T>::NominatorDNE);
			// cancel exit request
			state.cancel_leave();
			<NominatorState<T>>::insert(&nominator, state);
			Self::deposit_event(Event::NominatorExitCancelled { nominator });
			Ok(().into())
		}

		#[pallet::call_index(30)]
		#[pallet::weight(<T as Config>::WeightInfo::schedule_revoke_nomination())]
		/// Request to revoke an existing nomination. If successful, the nomination is scheduled
		/// to be allowed to be revoked via the `execute_nomination_request` extrinsic.
		pub fn schedule_revoke_nomination(
			origin: OriginFor<T>,
			validator: T::AccountId,
		) -> DispatchResultWithPostInfo {
			let nominator = ensure_signed(origin)?;
			let mut state = <NominatorState<T>>::get(&nominator).ok_or(Error::<T>::NominatorDNE)?;
			ensure!(!state.is_leaving(), Error::<T>::NominatorAlreadyLeaving);
			let (now, when) = state.schedule_revoke::<T>(validator.clone())?;
			<NominatorState<T>>::insert(&nominator, state);
			Self::deposit_event(Event::NominationRevocationScheduled {
				round: now,
				nominator,
				candidate: validator,
				scheduled_exit: when,
			});
			Ok(().into())
		}

		#[pallet::call_index(31)]
		#[pallet::weight(<T as Config>::WeightInfo::nominator_bond_more())]
		/// Bond more for nominators wrt a specific validator candidate.
		pub fn nominator_bond_more(
			origin: OriginFor<T>,
			candidate: T::AccountId,
			more: BalanceOf<T>,
		) -> DispatchResultWithPostInfo {
			let nominator = ensure_signed(origin)?;
			let mut state = <NominatorState<T>>::get(&nominator).ok_or(Error::<T>::NominatorDNE)?;
			state.increase_nomination::<T>(candidate.clone(), more)?;
			Self::sort_candidates_by_voting_power();
			Ok(().into())
		}

		#[pallet::call_index(32)]
		#[pallet::weight(<T as Config>::WeightInfo::schedule_nominator_bond_less())]
		/// Request bond less for nominators wrt a specific validator candidate.
		pub fn schedule_nominator_bond_less(
			origin: OriginFor<T>,
			candidate: T::AccountId,
			less: BalanceOf<T>,
		) -> DispatchResultWithPostInfo {
			let caller = ensure_signed(origin)?;
			let mut state = <NominatorState<T>>::get(&caller).ok_or(Error::<T>::NominatorDNE)?;
			ensure!(!state.is_leaving(), Error::<T>::NominatorAlreadyLeaving);
			let when = state.schedule_decrease_nomination::<T>(candidate.clone(), less)?;
			<NominatorState<T>>::insert(&caller, state);
			Self::deposit_event(Event::NominationDecreaseScheduled {
				nominator: caller,
				candidate,
				amount_to_decrease: less,
				execute_round: when,
			});
			Ok(().into())
		}

		#[pallet::call_index(33)]
		#[pallet::weight(<T as Config>::WeightInfo::execute_nominator_bond_less())]
		/// Execute pending request to change an existing nomination
		pub fn execute_nomination_request(
			origin: OriginFor<T>,
			candidate: T::AccountId,
		) -> DispatchResultWithPostInfo {
			let nominator = ensure_signed(origin)?;
			let mut state = <NominatorState<T>>::get(&nominator).ok_or(Error::<T>::NominatorDNE)?;
			state.execute_pending_request::<T>(candidate)?;
			Self::sort_candidates_by_voting_power();
			Ok(().into())
		}

		#[pallet::call_index(34)]
		#[pallet::weight(<T as Config>::WeightInfo::cancel_nominator_bond_less())]
		/// Cancel request to change an existing nomination.
		pub fn cancel_nomination_request(
			origin: OriginFor<T>,
			candidate: T::AccountId,
		) -> DispatchResultWithPostInfo {
			let nominator = ensure_signed(origin)?;
			let mut state = <NominatorState<T>>::get(&nominator).ok_or(Error::<T>::NominatorDNE)?;
			let request = state.cancel_pending_request::<T>(candidate)?;
			<NominatorState<T>>::insert(&nominator, state);
			Self::deposit_event(Event::CancelledNominationRequest {
				nominator,
				cancelled_request: request,
			});
			Ok(().into())
		}
	}
}
