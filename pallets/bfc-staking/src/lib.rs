//! # Bfc Staking
//! Minimal staking pallet that implements validator selection by total backed stake.
//! The main difference between this pallet and `frame/pallet-staking` is that this pallet
//! uses direct nomination. Nominators choose exactly who they nominate and with what stake.
//! This is different from `frame/pallet-staking` where nominators approval vote and run Phragmen.
//!
//! ### Rules
//! There is a new round every `<Round<T>>::get().length` blocks.
//!
//! At the start of every round,
//! * issuance is calculated for validators (and their nominators) for block authoring
//! `T::RewardPaymentDelay` rounds ago
//! * a new set of validators is chosen from the candidates
//!
//! Immediately following a round change, payments are made once-per-block until all payments have
//! been made. In each such block, one validator is chosen for a rewards payment and is paid along
//! with each of its top `T::MaxTopNominationsPerCandidate` nominators.
//!
//! To join the set of candidates, call `join_candidates` with `bond >= MinCandidateStk`.
//! To leave the set of candidates, call `schedule_leave_candidates`. If the call succeeds,
//! the validator is removed from the pool of candidates so they cannot be selected for future
//! validator sets, but they are not unbonded until their exit request is executed. Any signed
//! account may trigger the exit `T::LeaveCandidatesDelay` rounds after the round in which the
//! original request was made.
//!
//! To join the set of nominators, call `nominate` and pass in an account that is
//! already a validator candidate and `bond >= MinNominatorStk`. Each nominator can nominate up to
//! `T::MaxNominationsPerNominator` validator candidates by calling `nominate`.
//!
//! To revoke a nomination, call `revoke_nomination` with the validator candidate's account.
//! To leave the set of nominators and revoke all nominations, call `leave_nominators`.

#![cfg_attr(not(feature = "std"), no_std)]

pub mod inflation;
pub mod migrations;
mod pallet;
mod set;
pub mod weights;

pub use inflation::{InflationInfo, Range};
pub use pallet::pallet::*;
use weights::WeightInfo;

use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

use bp_staking::{RoundIndex, TierType};
use frame_support::{
	pallet_prelude::*,
	traits::{tokens::Balance, Currency, Get, ReservableCurrency},
};
use sp_runtime::{
	traits::{Convert, MaybeDisplay, One, Saturating, Zero},
	FixedPointOperand, Perbill, RuntimeDebug,
};
use sp_staking::SessionIndex;
use sp_std::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	fmt::Debug,
	prelude::*,
};

pub type BalanceOf<T> =
	<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

pub type BlockNumberOf<T> = <T as frame_system::Config>::BlockNumber;

/// The type that indicates the point of a reward
pub type RewardPoint = u32;

pub(crate) const LOG_TARGET: &'static str = "runtime::staking";

// syntactic sugar for logging.
#[macro_export]
macro_rules! log {
	($level:tt, $patter:expr $(, $values:expr)* $(,)?) => {
		log::$level!(
			target: crate::LOG_TARGET,
			concat!("[{:?}] 💸 ", $patter), <frame_system::Pallet<T>>::block_number() $(, $values)*
		)
	};
}

/// Used for release versioning upto v3_0_0.
///
/// Obsolete from v4. Keeping around to make encoding/decoding of old migration code easier.

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
/// A value placed in storage that represents the current version of the Staking storage. This value
/// is used by the `on_runtime_upgrade` logic to determine whether we run storage migration logic.
enum Releases {
	V1_0_0,
	V2_0_0,
	V3_0_0,
}

impl Default for Releases {
	fn default() -> Self {
		Releases::V3_0_0
	}
}

#[derive(
	PartialEq, Eq, PartialOrd, Ord, Clone, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen,
)]
/// The candidates or the nominators bonded amount to the network
pub struct Bond<AccountId, Balance> {
	/// The controller account used to reserve their staked balance
	/// - currently nominators does not use split accounts
	/// - so their account that acts as a nominator will be the controller account for now
	pub owner: AccountId,
	/// The total reserved balance as staked
	/// - the reserved balance is originated from the associated stash account
	pub amount: Balance,
}

impl<A: Decode, B: Default> Default for Bond<A, B> {
	fn default() -> Bond<A, B> {
		Bond {
			owner: A::decode(&mut sp_runtime::traits::TrailingZeroInput::zeroes())
				.expect("infinite length input; no invalid inputs for type; qed"),
			amount: B::default(),
		}
	}
}

#[derive(
	Eq,
	PartialEq,
	Ord,
	PartialOrd,
	Copy,
	Clone,
	Encode,
	Decode,
	RuntimeDebug,
	TypeInfo,
	MaxEncodedLen,
)]
/// The activity status of the validator
pub enum ValidatorStatus {
	/// Committed to be online and producing valid blocks (not equivocating)
	Active,
	/// Temporarily inactive and excused for inactivity
	Idle,
	/// Kicked out until candidates rejoin
	KickedOut,
	/// Bonded until the inner round
	Leaving(RoundIndex),
}

impl Default for ValidatorStatus {
	fn default() -> ValidatorStatus {
		ValidatorStatus::Active
	}
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
/// Snapshot of the validator state at the start of the round for which they are selected
pub struct ValidatorSnapshot<AccountId, Balance> {
	/// The self-bond of the active validator
	pub bond: Balance,
	/// The top nominations of the active validator
	pub nominations: Vec<Bond<AccountId, Balance>>,
	/// The voting power of the active validator
	pub total: Balance,
}

impl<A, B: Default> Default for ValidatorSnapshot<A, B> {
	fn default() -> ValidatorSnapshot<A, B> {
		ValidatorSnapshot { bond: B::default(), nominations: Vec::new(), total: B::default() }
	}
}

pub struct ValidatorSnapshotOf<T>(PhantomData<T>);
impl<T: Config> Convert<T::AccountId, Option<ValidatorSnapshot<T::AccountId, BalanceOf<T>>>>
	for ValidatorSnapshotOf<T>
{
	fn convert(validator: T::AccountId) -> Option<ValidatorSnapshot<T::AccountId, BalanceOf<T>>> {
		let round = Pallet::<T>::round();
		Some(Pallet::<T>::at_stake(round.current_round_index, &validator))
	}
}

#[derive(Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
/// Total staked information of the current chain state
pub struct TotalSnapshot<Balance> {
	/// The total self-bond of all validator candidates
	pub total_self_bond: Balance,
	/// The active self-bond of all active (selected) validators
	pub active_self_bond: Balance,
	/// The total (top + bottom) nominations of all validator candidates
	pub total_nominations: Balance,
	/// The total top nominations of all validator candidates
	pub total_top_nominations: Balance,
	/// The total bottom nominations of all validator candidates
	pub total_bottom_nominations: Balance,
	/// The active (top + bottom) nominations of active (selected) validators
	pub active_nominations: Balance,
	/// The active top nominations of active (selected) validators
	pub active_top_nominations: Balance,
	/// The active bottom nominations of active (selected) validators
	pub active_bottom_nominations: Balance,
	/// The count of nominators (top + bottom) of all validator candidates
	pub total_nominators: u32,
	/// The count of top nominators of all validator candidates
	pub total_top_nominators: u32,
	/// The count of bottom nominators of all validator candidates
	pub total_bottom_nominators: u32,
	/// The count of active nominators (top + bottom) of active (selected) validators
	pub active_nominators: u32,
	/// The count of active top nominators of active (selected) validators
	pub active_top_nominators: u32,
	/// The count of active bottom nominators of active (selected) validators
	pub active_bottom_nominators: u32,
	/// The total staked amount (self-bond + top/bottom nominations) of all validator candidates
	pub total_stake: Balance,
	/// The active staked amount (self-bond + top/bottom nominations) of active (selected)
	/// validators
	pub active_stake: Balance,
	/// The total voting power (self-bond + top nominations) of all validator candidates
	pub total_voting_power: Balance,
	/// The active voting power (self-bond + top nominations) of active (selected) validators
	pub active_voting_power: Balance,
}

impl<B: Default> Default for TotalSnapshot<B> {
	fn default() -> TotalSnapshot<B> {
		TotalSnapshot {
			total_self_bond: B::default(),
			active_self_bond: B::default(),
			total_nominations: B::default(),
			total_top_nominations: B::default(),
			total_bottom_nominations: B::default(),
			active_nominations: B::default(),
			active_top_nominations: B::default(),
			active_bottom_nominations: B::default(),
			total_nominators: 0u32,
			total_top_nominators: 0u32,
			total_bottom_nominators: 0u32,
			active_nominators: 0u32,
			active_top_nominators: 0u32,
			active_bottom_nominators: 0u32,
			total_stake: B::default(),
			active_stake: B::default(),
			total_voting_power: B::default(),
			active_voting_power: B::default(),
		}
	}
}

impl<
		Balance: Copy
			+ Zero
			+ PartialOrd
			+ Saturating
			+ sp_std::ops::AddAssign
			+ sp_std::ops::SubAssign
			+ sp_std::ops::Sub<Output = Balance>
			+ sp_std::fmt::Debug,
	> TotalSnapshot<Balance>
{
	pub fn increment_total_self_bond(&mut self, bond: Balance) {
		self.total_self_bond = self.total_self_bond.saturating_add(bond);
	}

	pub fn increment_active_self_bond(&mut self, bond: Balance) {
		self.active_self_bond = self.active_self_bond.saturating_add(bond);
	}

	pub fn increment_total_nominations(&mut self, nomination: Balance) {
		self.total_nominations = self.total_nominations.saturating_add(nomination);
	}

	pub fn increment_total_top_nominations(&mut self, nomination: Balance) {
		self.total_top_nominations = self.total_top_nominations.saturating_add(nomination);
	}

	pub fn increment_total_bottom_nominations(&mut self, nomination: Balance) {
		self.total_bottom_nominations = self.total_bottom_nominations.saturating_add(nomination);
	}

	pub fn increment_active_nominations(&mut self, nomination: Balance) {
		self.active_nominations = self.active_nominations.saturating_add(nomination);
	}

	pub fn increment_active_top_nominations(&mut self, nomination: Balance) {
		self.active_top_nominations = self.active_top_nominations.saturating_add(nomination);
	}

	pub fn increment_active_bottom_nominations(&mut self, nomination: Balance) {
		self.active_bottom_nominations = self.active_bottom_nominations.saturating_add(nomination);
	}

	pub fn increment_total_nominators(&mut self, nominators: u32) {
		self.total_nominators = self.total_nominators.saturating_add(nominators);
	}

	pub fn increment_total_top_nominators(&mut self, nominators: u32) {
		self.total_top_nominators = self.total_top_nominators.saturating_add(nominators);
	}

	pub fn increment_total_bottom_nominators(&mut self, nominators: u32) {
		self.total_bottom_nominators = self.total_bottom_nominators.saturating_add(nominators);
	}

	pub fn increment_active_nominators(&mut self, nominators: u32) {
		self.active_nominators = self.active_nominators.saturating_add(nominators);
	}

	pub fn increment_active_top_nominators(&mut self, nominators: u32) {
		self.active_top_nominators = self.active_top_nominators.saturating_add(nominators);
	}

	pub fn increment_active_bottom_nominators(&mut self, nominators: u32) {
		self.active_bottom_nominators = self.active_bottom_nominators.saturating_add(nominators);
	}

	pub fn increment_total_stake(&mut self, stake: Balance) {
		self.total_stake = self.total_stake.saturating_add(stake);
	}

	pub fn increment_active_stake(&mut self, stake: Balance) {
		self.active_stake = self.active_stake.saturating_add(stake);
	}

	pub fn increment_total_voting_power(&mut self, voting_power: Balance) {
		self.total_voting_power = self.total_voting_power.saturating_add(voting_power);
	}

	pub fn increment_active_voting_power(&mut self, voting_power: Balance) {
		self.active_voting_power = self.active_voting_power.saturating_add(voting_power);
	}
}

/// Reward destination options.
#[derive(
	Eq,
	PartialEq,
	Ord,
	PartialOrd,
	Copy,
	Clone,
	Encode,
	Decode,
	RuntimeDebug,
	TypeInfo,
	MaxEncodedLen,
)]
pub enum RewardDestination {
	/// Pay into the bonded account, increasing the amount at stake accordingly.
	Staked,
	/// Pay into the bonded account, not increasing the amount at stake.
	Account,
}

impl Default for RewardDestination {
	fn default() -> Self {
		RewardDestination::Staked
	}
}

#[derive(Default, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
/// Info needed to make delayed controller sets after round end
pub struct DelayedControllerSet<AccountId> {
	/// The bonded stash account
	pub stash: AccountId,
	/// The original bonded controller account
	pub old: AccountId,
	/// The new controller account
	pub new: AccountId,
}

impl<AccountId: PartialEq + Clone> DelayedControllerSet<AccountId> {
	pub fn new(stash: AccountId, old: AccountId, new: AccountId) -> Self {
		DelayedControllerSet { stash, old, new }
	}
}

#[derive(Default, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
/// Info needed to made delayed commission sets after round end
pub struct DelayedCommissionSet<AccountId> {
	/// The bonded controller account
	pub who: AccountId,
	/// The original commission rate
	pub old: Perbill,
	/// The new commission rate
	pub new: Perbill,
}

impl<AccountId: PartialEq + Clone> DelayedCommissionSet<AccountId> {
	pub fn new(who: AccountId, old: Perbill, new: Perbill) -> Self {
		DelayedCommissionSet { who, old, new }
	}
}

#[derive(Default, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
/// Info needed to make delayed payments to stakers after round end
pub struct DelayedPayout<Balance> {
	/// Total round reward (result of compute_issuance() at round end)
	pub round_issuance: Balance,
	/// The total inflation paid this round to stakers
	pub total_staking_reward: Balance,
	/// The default validator commission rate
	pub validator_commission: Perbill,
}

#[derive(
	Eq,
	PartialEq,
	Ord,
	PartialOrd,
	Clone,
	Copy,
	Encode,
	Decode,
	RuntimeDebug,
	TypeInfo,
	MaxEncodedLen,
)]
/// Request scheduled to change the candidate self-bond
pub struct CandidateBondLessRequest<Balance> {
	/// The requested less amount
	pub amount: Balance,
	/// The executable round index
	pub when_executable: RoundIndex,
}

#[derive(Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
/// Type for top and bottom nomination storage item
pub struct Nominations<AccountId, Balance> {
	/// Single nomination bond of top or bottom
	pub nominations: Vec<Bond<AccountId, Balance>>,
	/// The total nomination of top or bottom
	pub total: Balance,
}

impl<A, B: Default> Default for Nominations<A, B> {
	fn default() -> Nominations<A, B> {
		Nominations { nominations: Vec::new(), total: B::default() }
	}
}

impl<
		AccountId: PartialEq + Clone,
		Balance: Copy
			+ Ord
			+ sp_std::ops::AddAssign
			+ sp_std::ops::SubAssign
			+ sp_std::ops::Sub<Output = Balance>
			+ Zero
			+ Saturating,
	> Nominations<AccountId, Balance>
{
	/// Retrieve the nominator accounts as a vector
	pub fn nominators(&self) -> Vec<AccountId> {
		self.nominations.iter().map(|n| n.owner.clone()).collect()
	}

	pub fn count(&self) -> u32 {
		self.nominations.len() as u32
	}

	pub fn sort_greatest_to_least(&mut self) {
		self.nominations.sort_by(|a, b| b.amount.cmp(&a.amount));
	}

	/// Insert sorted greatest to least and increase .total accordingly
	/// Insertion respects first come first serve so new nominations are pushed after existing
	/// nominations if the amount is the same
	pub fn insert_sorted_greatest_to_least(&mut self, nomination: Bond<AccountId, Balance>) {
		self.total = self.total.saturating_add(nomination.amount);

		let insertion_index =
			match self.nominations.binary_search_by(|x| nomination.amount.cmp(&x.amount)) {
				// Find the next index where amount is not equal to the current nomination amount.
				Ok(i) => self.nominations[i..]
					.iter()
					.position(|x| x.amount != nomination.amount)
					.map_or(self.nominations.len(), |offset| i + offset),
				Err(i) => i,
			};

		self.nominations.insert(insertion_index, nomination);
	}

	/// Return the capacity status for top nominations
	pub fn top_capacity<T: Config>(&self) -> CapacityStatus {
		match &self.nominations {
			x if x.len() as u32 >= T::MaxTopNominationsPerCandidate::get() => CapacityStatus::Full,
			x if x.is_empty() => CapacityStatus::Empty,
			_ => CapacityStatus::Partial,
		}
	}

	/// Return the capacity status for bottom nominations
	pub fn bottom_capacity<T: Config>(&self) -> CapacityStatus {
		match &self.nominations {
			x if x.len() as u32 >= T::MaxBottomNominationsPerCandidate::get() => {
				CapacityStatus::Full
			},
			x if x.is_empty() => CapacityStatus::Empty,
			_ => CapacityStatus::Partial,
		}
	}

	/// Return last nomination amount without popping the nomination
	pub fn lowest_nomination_amount(&self) -> Balance {
		self.nominations.last().map(|x| x.amount).unwrap_or(Balance::zero())
	}

	/// Return highest nomination amount
	pub fn highest_nomination_amount(&self) -> Balance {
		self.nominations.first().map(|x| x.amount).unwrap_or(Balance::zero())
	}

	/// Slash nominator's bonding and total amount with given slashing value.
	pub fn slash_nomination_amount<T: Config>(
		&mut self,
		nominator: &AccountId,
		value: Balance,
	) -> bool {
		for bond in self.nominations.iter_mut() {
			if bond.owner == *nominator {
				bond.amount = bond.amount.saturating_sub(value);
				self.total = self.total.saturating_sub(value);
				return true;
			}
		}
		false
	}
}

#[derive(
	Eq, PartialEq, Ord, PartialOrd, Clone, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen,
)]
/// Capacity status for top or bottom nominations
pub enum CapacityStatus {
	/// Reached capacity
	Full,
	/// Empty aka contains no nominations
	Empty,
	/// Partially full (nonempty and not full)
	Partial,
}

#[derive(
	Eq, PartialEq, Ord, PartialOrd, Clone, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen,
)]
/// Productivity status for active validators
pub enum ProductivityStatus {
	/// Successfully produced a block
	Active,
	/// Failed to produce a block
	Idle,
	/// Inactive for this round
	Ready,
}

#[derive(
	Eq, PartialEq, Ord, PartialOrd, Clone, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen,
)]
/// All candidate info except the top and bottom nominations
pub struct CandidateMetadata<AccountId, Balance, BlockNumber> {
	/// This candidate's stash account (public key)
	pub stash: AccountId,
	/// This candidate's current self-bond
	pub bond: Balance,
	/// This candidate's initial self-bond
	pub initial_bond: Balance,
	/// Total number of nominations (top + bottom) to this candidate
	pub nomination_count: u32,
	/// Self-bond + top nominations
	pub voting_power: Balance,
	/// The smallest top nomination amount
	pub lowest_top_nomination_amount: Balance,
	/// The highest bottom nomination amount
	pub highest_bottom_nomination_amount: Balance,
	/// The smallest bottom nomination amount
	pub lowest_bottom_nomination_amount: Balance,
	/// Capacity status for top nominations
	pub top_capacity: CapacityStatus,
	/// Capacity status for bottom nominations
	pub bottom_capacity: CapacityStatus,
	/// Maximum 1 pending request to decrease candidate self bond at any given time
	pub request: Option<CandidateBondLessRequest<Balance>>,
	/// Current status of the validator
	pub status: ValidatorStatus,
	/// Selection state of the candidate in the current round
	pub is_selected: bool,
	/// The validator commission ratio
	pub commission: Perbill,
	/// The last block number this candidate produced
	pub last_block: BlockNumber,
	/// The total blocks this candidate produced in the current round
	pub blocks_produced: u32,
	/// The block productivity for this candidate in the current round
	pub productivity: Perbill,
	/// The block productivity status for this candidate in the current round
	pub productivity_status: ProductivityStatus,
	/// The destination for round rewards
	pub reward_dst: RewardDestination,
	/// The amount of awarded tokens to this candidate
	pub awarded_tokens: Balance,
	/// The tier type of this candidate
	pub tier: TierType,
}

impl<
		AccountId: PartialEq + Clone,
		Balance: Copy
			+ Zero
			+ PartialOrd
			+ Saturating
			+ sp_std::ops::AddAssign
			+ sp_std::ops::SubAssign
			+ sp_std::ops::Sub<Output = Balance>
			+ sp_std::fmt::Debug,
		BlockNumber: Copy
			+ Zero
			+ PartialOrd
			+ sp_std::ops::AddAssign
			+ sp_std::ops::SubAssign
			+ sp_std::ops::Sub<Output = BlockNumber>
			+ sp_std::fmt::Debug,
	> CandidateMetadata<AccountId, Balance, BlockNumber>
{
	pub fn new<T: Config>(stash: AccountId, bond: Balance, tier: TierType) -> Self
	where
		BlockNumberOf<T>: From<BlockNumber>,
	{
		let commission = match tier {
			TierType::Full => <DefaultFullValidatorCommission<T>>::get(),
			_ => <DefaultBasicValidatorCommission<T>>::get(),
		};
		CandidateMetadata {
			stash,
			bond,
			initial_bond: bond,
			nomination_count: 0u32,
			voting_power: bond,
			lowest_top_nomination_amount: Zero::zero(),
			highest_bottom_nomination_amount: Zero::zero(),
			lowest_bottom_nomination_amount: Zero::zero(),
			top_capacity: CapacityStatus::Empty,
			bottom_capacity: CapacityStatus::Empty,
			request: None,
			status: ValidatorStatus::Active,
			is_selected: false,
			commission,
			last_block: Zero::zero(),
			blocks_produced: 0u32,
			productivity: Perbill::from_percent(100),
			productivity_status: ProductivityStatus::Idle,
			reward_dst: RewardDestination::default(),
			awarded_tokens: Zero::zero(),
			tier,
		}
	}

	pub fn is_active(&self) -> bool {
		matches!(self.status, ValidatorStatus::Active)
	}

	pub fn is_kicked_out(&self) -> bool {
		matches!(self.status, ValidatorStatus::KickedOut)
	}

	pub fn is_leaving(&self) -> bool {
		matches!(self.status, ValidatorStatus::Leaving(_))
	}

	pub fn schedule_leave<T: Config>(&mut self) -> Result<(RoundIndex, RoundIndex), DispatchError> {
		ensure!(!self.is_leaving(), Error::<T>::CandidateAlreadyLeaving);
		ensure!(self.request.is_none(), Error::<T>::PendingCandidateRequestAlreadyExists);
		let now = <Round<T>>::get().current_round_index;
		let when = now + T::LeaveCandidatesDelay::get();
		self.status = ValidatorStatus::Leaving(when);
		Ok((now, when))
	}

	pub fn can_leave<T: Config>(&self) -> DispatchResult {
		if let ValidatorStatus::Leaving(when) = self.status {
			ensure!(
				<Round<T>>::get().current_round_index >= when,
				Error::<T>::CandidateCannotLeaveYet
			);
			Ok(())
		} else {
			Err(Error::<T>::CandidateNotLeaving.into())
		}
	}

	pub fn go_offline(&mut self) {
		self.status = ValidatorStatus::Idle;
		self.is_selected = false;
	}

	pub fn go_online(&mut self) {
		self.status = ValidatorStatus::Active;
	}

	pub fn kick_out(&mut self) {
		self.status = ValidatorStatus::KickedOut;
		self.is_selected = false;
	}

	pub fn slash_bond(&mut self, bond: Balance) {
		self.bond = self.bond.saturating_sub(bond);
	}

	pub fn slash_voting_power(&mut self, voting_power: Balance) {
		self.voting_power = self.voting_power.saturating_sub(voting_power);
	}

	pub fn set_commission(&mut self, commission: Perbill) {
		self.commission = commission;
	}

	pub fn reset_commission<T: Config>(&mut self) {
		if self.tier == TierType::Full {
			self.commission = <DefaultFullValidatorCommission<T>>::get();
		} else {
			self.commission = <DefaultBasicValidatorCommission<T>>::get();
		}
	}

	pub fn set_reward_dst(&mut self, reward_dst: RewardDestination) {
		self.reward_dst = reward_dst;
	}

	pub fn set_is_selected(&mut self, is_selected: bool) {
		self.is_selected = is_selected;
	}

	pub fn set_last_block(&mut self, last_block: BlockNumber) {
		self.last_block = last_block;
	}

	pub fn increment_blocks_produced(&mut self) {
		self.blocks_produced += 1;
	}

	pub fn decrement_productivity<T: Config>(&mut self) {
		let base = <ProductivityPerBlock<T>>::get().deconstruct();
		let now = self.productivity.deconstruct();
		let new = {
			if base >= now {
				Perbill::zero()
			} else {
				Perbill::from_parts(now - base)
			}
		};
		self.productivity = new;
	}

	pub fn increment_awarded_tokens(&mut self, tokens: Balance) {
		self.awarded_tokens += tokens;
	}

	pub fn reset_blocks_produced(&mut self) {
		self.blocks_produced = 0u32;
	}

	pub fn reset_productivity(&mut self) {
		self.productivity = Perbill::from_percent(100);
		self.productivity_status = ProductivityStatus::Idle;
	}

	pub fn bond_more<T: Config>(
		&mut self,
		stash: T::AccountId,
		controller: T::AccountId,
		more: Balance,
	) -> DispatchResult
	where
		BalanceOf<T>: From<Balance>,
	{
		T::Currency::reserve(&stash, more.into())?;
		let new_total = <Total<T>>::get().saturating_add(more.into());
		self.bond = self.bond.saturating_add(more.into());
		self.voting_power = self.voting_power.saturating_add(more.into());
		<Total<T>>::put(new_total);
		<Pallet<T>>::deposit_event(Event::CandidateBondedMore {
			candidate: controller.clone(),
			amount: more.into(),
			new_total_bond: self.bond.into(),
		});
		Ok(())
	}

	/// Schedule executable decrease of validator candidate self bond
	/// Returns the round at which the validator can execute the pending request
	pub fn schedule_bond_less<T: Config>(
		&mut self,
		less: Balance,
	) -> Result<RoundIndex, DispatchError>
	where
		BalanceOf<T>: Into<Balance>,
	{
		// ensure no pending request
		ensure!(self.request.is_none(), Error::<T>::PendingCandidateRequestAlreadyExists);
		// ensure bond above min after decrease
		ensure!(self.bond > less, Error::<T>::CandidateBondBelowMin);
		if self.tier == TierType::Full {
			ensure!(
				self.bond - less >= T::MinFullCandidateStk::get().into(),
				Error::<T>::CandidateBondBelowMin
			);
		} else {
			ensure!(
				self.bond - less >= T::MinBasicCandidateStk::get().into(),
				Error::<T>::CandidateBondBelowMin
			);
		}
		let when_executable =
			<Round<T>>::get().current_round_index + T::CandidateBondLessDelay::get();
		self.request = Some(CandidateBondLessRequest { amount: less, when_executable });
		Ok(when_executable)
	}

	/// Execute pending request to decrease the validator self bond
	/// Returns the event to be emitted
	pub fn execute_bond_less<T: Config>(
		&mut self,
		stash: T::AccountId,
		controller: T::AccountId,
	) -> DispatchResult
	where
		BalanceOf<T>: From<Balance>,
	{
		let request = self.request.ok_or(Error::<T>::PendingCandidateRequestsDNE)?;
		ensure!(
			request.when_executable <= <Round<T>>::get().current_round_index,
			Error::<T>::PendingCandidateRequestNotDueYet
		);
		T::Currency::unreserve(&stash, request.amount.into());
		let new_total_staked = <Total<T>>::get().saturating_sub(request.amount.into());
		// Arithmetic assumptions are self.bond > less && self.bond - less > ValidatorMinBond
		// (assumptions enforced by `schedule_bond_less`; if storage corrupts, must re-verify)
		self.bond = self.bond.saturating_sub(request.amount);
		self.voting_power = self.voting_power.saturating_sub(request.amount);
		let event = Event::CandidateBondedLess {
			candidate: controller.clone().into(),
			amount: request.amount.into(),
			new_bond: self.bond.into(),
		};
		// reset s.t. no pending request
		self.request = None;
		// update candidate pool value because it must change if self bond changes
		<Total<T>>::put(new_total_staked);
		Pallet::<T>::update_active(&controller, self.voting_power.into())?;
		Pallet::<T>::deposit_event(event);
		Ok(())
	}

	/// Cancel candidate bond less request
	pub fn cancel_bond_less<T: Config>(&mut self, who: T::AccountId) -> DispatchResult
	where
		BalanceOf<T>: From<Balance>,
	{
		let request = self.request.ok_or(Error::<T>::PendingCandidateRequestsDNE)?;
		let event = Event::CancelledCandidateBondLess {
			candidate: who.clone().into(),
			amount: request.amount.into(),
			execute_round: request.when_executable,
		};
		self.request = None;
		Pallet::<T>::deposit_event(event);
		Ok(())
	}

	/// Reset top nominations metadata
	pub fn reset_top_data<T: Config>(
		&mut self,
		candidate: T::AccountId,
		top_nominations: &Nominations<T::AccountId, BalanceOf<T>>,
	) -> DispatchResult
	where
		BalanceOf<T>: Into<Balance> + From<Balance>,
	{
		self.lowest_top_nomination_amount = top_nominations.lowest_nomination_amount().into();
		self.top_capacity = top_nominations.top_capacity::<T>();
		let old_voting_power = self.voting_power;
		self.voting_power = self.bond + top_nominations.total.into();
		// CandidatePool value for candidate always changes if top nominations total changes
		// so we moved the update into this function to deduplicate code and patch a bug that
		// forgot to apply the update when increasing top nomination
		if old_voting_power != self.voting_power {
			Pallet::<T>::update_active(&candidate, self.voting_power.into())?;
		}

		Ok(())
	}

	/// Reset bottom nominations metadata
	pub fn reset_bottom_data<T: Config>(
		&mut self,
		bottom_nominations: &Nominations<T::AccountId, BalanceOf<T>>,
	) where
		BalanceOf<T>: Into<Balance>,
	{
		self.lowest_bottom_nomination_amount = bottom_nominations.lowest_nomination_amount().into();
		self.highest_bottom_nomination_amount =
			bottom_nominations.highest_nomination_amount().into();
		self.bottom_capacity = bottom_nominations.bottom_capacity::<T>();
	}

	/// Add nomination
	/// Returns whether nominator was added and an optional negative total counted remainder
	/// for if a bottom nomination was kicked
	/// MUST ensure no nomination exists for this candidate in the `NominatorState` before call
	pub fn add_nomination<T: Config>(
		&mut self,
		candidate: &T::AccountId,
		nomination: Bond<T::AccountId, BalanceOf<T>>,
	) -> Result<(NominatorAdded<Balance>, Option<Balance>), DispatchError>
	where
		BalanceOf<T>: Into<Balance> + From<Balance>,
	{
		let mut less_total_staked = None;
		let nominator_added = match self.top_capacity {
			CapacityStatus::Full => {
				// top is full, insert into top iff the lowest_top < amount
				if self.lowest_top_nomination_amount < nomination.amount.into() {
					// bumps lowest top to the bottom inside this function call
					less_total_staked = self.add_top_nomination::<T>(candidate, nomination)?;
					NominatorAdded::AddedToTop { new_total: self.voting_power }
				} else {
					// if bottom is full, only insert if greater than lowest bottom (which will
					// be bumped out)
					if matches!(self.bottom_capacity, CapacityStatus::Full) {
						ensure!(
							nomination.amount.into() > self.lowest_bottom_nomination_amount,
							Error::<T>::CannotNominateLessThanLowestBottomWhenBottomIsFull
						);
						// need to subtract from total staked
						less_total_staked = Some(self.lowest_bottom_nomination_amount);
					}
					// insert into bottom
					self.add_bottom_nomination::<T>(false, candidate, nomination)?;
					NominatorAdded::AddedToBottom
				}
			},
			// top is either empty or partially full
			_ => {
				self.add_top_nomination::<T>(candidate, nomination)?;
				NominatorAdded::AddedToTop { new_total: self.voting_power }
			},
		};
		Ok((nominator_added, less_total_staked))
	}

	/// Add nomination to top nomination
	/// Returns Option<negative_total_staked_remainder>
	/// Only call if lowest top nomination is less than nomination.amount || !top_full
	pub fn add_top_nomination<T: Config>(
		&mut self,
		candidate: &T::AccountId,
		nomination: Bond<T::AccountId, BalanceOf<T>>,
	) -> Result<Option<Balance>, DispatchError>
	where
		BalanceOf<T>: Into<Balance> + From<Balance>,
	{
		let mut less_total_staked = None;
		let mut top_nominations =
			<TopNominations<T>>::get(candidate).ok_or(<Error<T>>::TopNominationDNE)?;
		let max_top_nominations_per_candidate = T::MaxTopNominationsPerCandidate::get();
		if top_nominations.nominations.len() as u32 == max_top_nominations_per_candidate {
			// pop lowest top nomination
			let new_bottom_nomination =
				top_nominations.nominations.pop().ok_or(<Error<T>>::TopNominationDNE)?;
			top_nominations.total =
				top_nominations.total.saturating_sub(new_bottom_nomination.amount);
			if matches!(self.bottom_capacity, CapacityStatus::Full) {
				less_total_staked = Some(self.lowest_bottom_nomination_amount);
			}
			self.add_bottom_nomination::<T>(true, candidate, new_bottom_nomination)?;
		}
		// insert into top
		top_nominations.insert_sorted_greatest_to_least(nomination);
		// update candidate info
		self.reset_top_data::<T>(candidate.clone(), &top_nominations)?;
		if less_total_staked.is_none() {
			// only increment nomination count if we are not kicking a bottom nomination
			self.nomination_count += 1u32;
		}
		<TopNominations<T>>::insert(&candidate, top_nominations);
		Ok(less_total_staked)
	}

	/// Add nomination to bottom nominations
	/// Check before call that if capacity is full, inserted nomination is higher than lowest
	/// bottom nomination (and if so, need to adjust the total storage item)
	/// CALLER MUST ensure(lowest_bottom_to_be_kicked.amount < nomination.amount)
	pub fn add_bottom_nomination<T: Config>(
		&mut self,
		bumped_from_top: bool,
		candidate: &T::AccountId,
		nomination: Bond<T::AccountId, BalanceOf<T>>,
	) -> DispatchResult
	where
		BalanceOf<T>: Into<Balance> + From<Balance>,
	{
		let mut bottom_nominations =
			<BottomNominations<T>>::get(candidate).ok_or(<Error<T>>::BottomNominationDNE)?;
		// if bottom is full, kick the lowest bottom (which is expected to be lower than input
		// as per check)
		let increase_nomination_count = if bottom_nominations.nominations.len() as u32
			== T::MaxBottomNominationsPerCandidate::get()
		{
			let lowest_bottom_to_be_kicked =
				bottom_nominations.nominations.pop().ok_or(<Error<T>>::BottomNominationDNE)?;
			// EXPECT lowest_bottom_to_be_kicked.amount < nomination.amount enforced by caller
			// if lowest_bottom_to_be_kicked.amount == nomination.amount, we will still kick
			// the lowest bottom to enforce first come first served
			bottom_nominations.total =
				bottom_nominations.total.saturating_sub(lowest_bottom_to_be_kicked.amount);
			// update nominator state
			// unreserve kicked bottom
			T::Currency::unreserve(
				&lowest_bottom_to_be_kicked.owner,
				lowest_bottom_to_be_kicked.amount,
			);
			// total staked is updated via propagation of lowest bottom nomination amount prior
			// to call
			let mut nominator_state = <NominatorState<T>>::get(&lowest_bottom_to_be_kicked.owner)
				.ok_or(<Error<T>>::NominatorDNE)?;
			let leaving = nominator_state.nominations.len() == 1usize;
			nominator_state.rm_nomination(candidate);
			nominator_state.requests.remove_request(&candidate);
			Pallet::<T>::deposit_event(Event::NominationKicked {
				nominator: lowest_bottom_to_be_kicked.owner.clone(),
				candidate: candidate.clone(),
				unstaked_amount: lowest_bottom_to_be_kicked.amount,
			});
			if leaving {
				<NominatorState<T>>::remove(&lowest_bottom_to_be_kicked.owner);
				Pallet::<T>::deposit_event(Event::NominatorLeft {
					nominator: lowest_bottom_to_be_kicked.owner,
					unstaked_amount: lowest_bottom_to_be_kicked.amount,
				});
			} else {
				<NominatorState<T>>::insert(&lowest_bottom_to_be_kicked.owner, nominator_state);
			}
			false
		} else {
			!bumped_from_top
		};
		// only increase nomination count if new bottom nomination (1) doesn't come from top &&
		// (2) doesn't pop the lowest nomination from the bottom
		if increase_nomination_count {
			self.nomination_count += 1u32;
		}
		bottom_nominations.insert_sorted_greatest_to_least(nomination);
		self.reset_bottom_data::<T>(&bottom_nominations);
		<BottomNominations<T>>::insert(candidate, bottom_nominations);

		Ok(())
	}

	/// Remove nomination
	/// Removes from top if amount is above lowest top or top is not full
	/// Return Ok(if_voting_power_changed)
	pub fn rm_nomination_if_exists<T: Config>(
		&mut self,
		candidate: &T::AccountId,
		nominator: T::AccountId,
		amount: Balance,
	) -> Result<bool, DispatchError>
	where
		BalanceOf<T>: Into<Balance> + From<Balance>,
	{
		let amount_geq_lowest_top = amount >= self.lowest_top_nomination_amount;
		let top_is_not_full = !matches!(self.top_capacity, CapacityStatus::Full);
		let lowest_top_eq_highest_bottom =
			self.lowest_top_nomination_amount == self.highest_bottom_nomination_amount;
		let nomination_dne_err: DispatchError = Error::<T>::NominationDNE.into();
		if top_is_not_full || (amount_geq_lowest_top && !lowest_top_eq_highest_bottom) {
			self.rm_top_nomination::<T>(candidate, nominator)
		} else if amount_geq_lowest_top && lowest_top_eq_highest_bottom {
			let result = self.rm_top_nomination::<T>(candidate, nominator.clone());
			if result == Err(nomination_dne_err) {
				// worst case removal
				self.rm_bottom_nomination::<T>(candidate, nominator)
			} else {
				result
			}
		} else {
			self.rm_bottom_nomination::<T>(candidate, nominator)
		}
	}

	/// Remove top nomination, bumps top bottom nomination if exists
	pub fn rm_top_nomination<T: Config>(
		&mut self,
		candidate: &T::AccountId,
		nominator: T::AccountId,
	) -> Result<bool, DispatchError>
	where
		BalanceOf<T>: Into<Balance> + From<Balance>,
	{
		let old_voting_power = self.voting_power;
		// remove top nomination
		let mut top_nominations =
			<TopNominations<T>>::get(candidate).ok_or(<Error<T>>::CandidateDNE)?;
		let mut actual_amount_option: Option<BalanceOf<T>> = None;
		top_nominations.nominations = top_nominations
			.nominations
			.clone()
			.into_iter()
			.filter(|d| {
				if d.owner != nominator {
					true
				} else {
					actual_amount_option = Some(d.amount);
					false
				}
			})
			.collect();
		let actual_amount = actual_amount_option.ok_or(Error::<T>::NominationDNE)?;
		top_nominations.total = top_nominations.total.saturating_sub(actual_amount);
		// if bottom nonempty => bump top bottom to top
		if !matches!(self.bottom_capacity, CapacityStatus::Empty) {
			let mut bottom_nominations =
				<BottomNominations<T>>::get(candidate).ok_or(<Error<T>>::BottomNominationDNE)?;
			// expect already stored greatest to least by bond amount
			let highest_bottom_nomination = bottom_nominations.nominations.remove(0);
			bottom_nominations.total =
				bottom_nominations.total.saturating_sub(highest_bottom_nomination.amount);
			self.reset_bottom_data::<T>(&bottom_nominations);
			<BottomNominations<T>>::insert(candidate, bottom_nominations);
			// insert highest bottom into top nominations
			top_nominations.insert_sorted_greatest_to_least(highest_bottom_nomination);
		}
		// update candidate info
		self.reset_top_data::<T>(candidate.clone(), &top_nominations)?;
		self.nomination_count = self.nomination_count.saturating_sub(1u32);
		<TopNominations<T>>::insert(candidate, top_nominations);
		// return whether total counted changed
		Ok(old_voting_power == self.voting_power)
	}

	/// Remove bottom nomination
	/// Returns if_voting_power_changed: bool
	pub fn rm_bottom_nomination<T: Config>(
		&mut self,
		candidate: &T::AccountId,
		nominator: T::AccountId,
	) -> Result<bool, DispatchError>
	where
		BalanceOf<T>: Into<Balance>,
	{
		// remove bottom nomination
		let mut bottom_nominations =
			<BottomNominations<T>>::get(candidate).ok_or(<Error<T>>::BottomNominationDNE)?;
		let mut actual_amount_option: Option<BalanceOf<T>> = None;
		bottom_nominations.nominations = bottom_nominations
			.nominations
			.clone()
			.into_iter()
			.filter(|d| {
				if d.owner != nominator {
					true
				} else {
					actual_amount_option = Some(d.amount);
					false
				}
			})
			.collect();
		let actual_amount = actual_amount_option.ok_or(Error::<T>::NominationDNE)?;
		bottom_nominations.total = bottom_nominations.total.saturating_sub(actual_amount);
		// update candidate info
		self.reset_bottom_data::<T>(&bottom_nominations);
		self.nomination_count = self.nomination_count.saturating_sub(1u32);
		<BottomNominations<T>>::insert(candidate, bottom_nominations);
		Ok(false)
	}

	/// Increase nomination amount
	pub fn increase_nomination<T: Config>(
		&mut self,
		candidate: &T::AccountId,
		nominator: T::AccountId,
		bond: BalanceOf<T>,
		more: BalanceOf<T>,
	) -> Result<bool, DispatchError>
	where
		BalanceOf<T>: Into<Balance> + From<Balance>,
	{
		let lowest_top_eq_highest_bottom =
			self.lowest_top_nomination_amount == self.highest_bottom_nomination_amount;
		let bond_geq_lowest_top = bond.into() >= self.lowest_top_nomination_amount;
		let nomination_dne_err: DispatchError = Error::<T>::NominationDNE.into();
		if bond_geq_lowest_top && !lowest_top_eq_highest_bottom {
			// definitely in top
			self.increase_top_nomination::<T>(candidate, nominator.clone(), more)
		} else if bond_geq_lowest_top && lowest_top_eq_highest_bottom {
			// update top but if error then update bottom (because could be in bottom because
			// lowest_top_eq_highest_bottom)
			let result = self.increase_top_nomination::<T>(candidate, nominator.clone(), more);
			if result == Err(nomination_dne_err) {
				self.increase_bottom_nomination::<T>(candidate, nominator, bond, more)
			} else {
				result
			}
		} else {
			self.increase_bottom_nomination::<T>(candidate, nominator, bond, more)
		}
	}

	/// Increase top nomination
	pub fn increase_top_nomination<T: Config>(
		&mut self,
		candidate: &T::AccountId,
		nominator: T::AccountId,
		more: BalanceOf<T>,
	) -> Result<bool, DispatchError>
	where
		BalanceOf<T>: Into<Balance> + From<Balance>,
	{
		let mut top_nominations =
			<TopNominations<T>>::get(candidate).ok_or(<Error<T>>::TopNominationDNE)?;
		let mut in_top = false;
		top_nominations.nominations = top_nominations
			.nominations
			.into_iter()
			.map(|d| {
				if d.owner == nominator {
					in_top = true;
					Bond { owner: d.owner, amount: d.amount.saturating_add(more) }
				} else {
					d
				}
			})
			.collect();
		ensure!(in_top, Error::<T>::NominationDNE);
		top_nominations.total = top_nominations.total.saturating_add(more);
		top_nominations.sort_greatest_to_least();
		self.reset_top_data::<T>(candidate.clone(), &top_nominations)?;
		<TopNominations<T>>::insert(candidate, top_nominations);
		Ok(true)
	}

	/// Increase bottom nomination
	pub fn increase_bottom_nomination<T: Config>(
		&mut self,
		candidate: &T::AccountId,
		nominator: T::AccountId,
		bond: BalanceOf<T>,
		more: BalanceOf<T>,
	) -> Result<bool, DispatchError>
	where
		BalanceOf<T>: Into<Balance> + From<Balance>,
	{
		let mut bottom_nominations =
			<BottomNominations<T>>::get(candidate).ok_or(Error::<T>::CandidateDNE)?;
		let mut nomination_option: Option<Bond<T::AccountId, BalanceOf<T>>> = None;
		let in_top_after = if bond.saturating_add(more).into() > self.lowest_top_nomination_amount {
			// bump it from bottom
			bottom_nominations.nominations = bottom_nominations
				.nominations
				.clone()
				.into_iter()
				.filter(|d| {
					if d.owner != nominator {
						true
					} else {
						nomination_option = Some(Bond {
							owner: d.owner.clone(),
							amount: d.amount.saturating_add(more),
						});
						false
					}
				})
				.collect();
			let nomination = nomination_option.ok_or(Error::<T>::NominationDNE)?;
			bottom_nominations.total = bottom_nominations.total.saturating_sub(bond);
			// add it to top
			let mut top_nominations =
				<TopNominations<T>>::get(candidate).ok_or(<Error<T>>::TopNominationDNE)?;
			// if top is full, pop lowest top
			if matches!(top_nominations.top_capacity::<T>(), CapacityStatus::Full) {
				// pop lowest top nomination
				let new_bottom_nomination =
					top_nominations.nominations.pop().ok_or(<Error<T>>::TopNominationDNE)?;
				top_nominations.total =
					top_nominations.total.saturating_sub(new_bottom_nomination.amount);
				bottom_nominations.insert_sorted_greatest_to_least(new_bottom_nomination);
			}
			// insert into top
			top_nominations.insert_sorted_greatest_to_least(nomination);
			self.reset_top_data::<T>(candidate.clone(), &top_nominations)?;
			<TopNominations<T>>::insert(candidate, top_nominations);
			true
		} else {
			let mut in_bottom = false;
			// just increase the nomination
			bottom_nominations.nominations = bottom_nominations
				.nominations
				.into_iter()
				.map(|d| {
					if d.owner == nominator {
						in_bottom = true;
						Bond { owner: d.owner, amount: d.amount.saturating_add(more) }
					} else {
						d
					}
				})
				.collect();
			ensure!(in_bottom, Error::<T>::NominationDNE);
			bottom_nominations.total = bottom_nominations.total.saturating_add(more);
			bottom_nominations.sort_greatest_to_least();
			false
		};
		self.reset_bottom_data::<T>(&bottom_nominations);
		<BottomNominations<T>>::insert(candidate, bottom_nominations);
		Ok(in_top_after)
	}

	/// Decrease nomination
	pub fn decrease_nomination<T: Config>(
		&mut self,
		candidate: &T::AccountId,
		nominator: T::AccountId,
		bond: Balance,
		less: BalanceOf<T>,
	) -> Result<bool, DispatchError>
	where
		BalanceOf<T>: Into<Balance> + From<Balance>,
	{
		let lowest_top_eq_highest_bottom =
			self.lowest_top_nomination_amount == self.highest_bottom_nomination_amount;
		let bond_geq_lowest_top = bond >= self.lowest_top_nomination_amount;
		let nomination_dne_err: DispatchError = Error::<T>::NominationDNE.into();
		if bond_geq_lowest_top && !lowest_top_eq_highest_bottom {
			// definitely in top
			self.decrease_top_nomination::<T>(candidate, nominator.clone(), bond.into(), less)
		} else if bond_geq_lowest_top && lowest_top_eq_highest_bottom {
			// update top but if error then update bottom (because could be in bottom because
			// lowest_top_eq_highest_bottom)
			let result =
				self.decrease_top_nomination::<T>(candidate, nominator.clone(), bond.into(), less);
			if result == Err(nomination_dne_err) {
				self.decrease_bottom_nomination::<T>(candidate, nominator, less)
			} else {
				result
			}
		} else {
			self.decrease_bottom_nomination::<T>(candidate, nominator, less)
		}
	}

	/// Decrease top nomination
	pub fn decrease_top_nomination<T: Config>(
		&mut self,
		candidate: &T::AccountId,
		nominator: T::AccountId,
		bond: BalanceOf<T>,
		less: BalanceOf<T>,
	) -> Result<bool, DispatchError>
	where
		BalanceOf<T>: Into<Balance> + From<Balance>,
	{
		// The nomination after the `decrease-nomination` will be strictly less than the
		// highest bottom nomination
		let bond_after_less_than_highest_bottom =
			bond.saturating_sub(less).into() < self.highest_bottom_nomination_amount;
		// The top nominations is full and the bottom nominations has at least one nomination
		let full_top_and_nonempty_bottom = matches!(self.top_capacity, CapacityStatus::Full)
			&& !matches!(self.bottom_capacity, CapacityStatus::Empty);
		let mut top_nominations =
			<TopNominations<T>>::get(candidate).ok_or(Error::<T>::CandidateDNE)?;
		let in_top_after = if bond_after_less_than_highest_bottom && full_top_and_nonempty_bottom {
			let mut nomination_option: Option<Bond<T::AccountId, BalanceOf<T>>> = None;
			// take nomination from top
			top_nominations.nominations = top_nominations
				.nominations
				.clone()
				.into_iter()
				.filter(|d| {
					if d.owner != nominator {
						true
					} else {
						top_nominations.total = top_nominations.total.saturating_sub(d.amount);
						nomination_option = Some(Bond {
							owner: d.owner.clone(),
							amount: d.amount.saturating_sub(less),
						});
						false
					}
				})
				.collect();
			let nomination = nomination_option.ok_or(Error::<T>::NominationDNE)?;
			// pop highest bottom by reverse and popping
			let mut bottom_nominations =
				<BottomNominations<T>>::get(candidate).ok_or(<Error<T>>::BottomNominationDNE)?;
			let highest_bottom_nomination = bottom_nominations.nominations.remove(0);
			bottom_nominations.total =
				bottom_nominations.total.saturating_sub(highest_bottom_nomination.amount);
			// insert highest bottom into top
			top_nominations.insert_sorted_greatest_to_least(highest_bottom_nomination);
			// insert previous top into bottom
			bottom_nominations.insert_sorted_greatest_to_least(nomination);
			self.reset_bottom_data::<T>(&bottom_nominations);
			<BottomNominations<T>>::insert(candidate, bottom_nominations);
			false
		} else {
			// keep it in the top
			let mut is_in_top = false;
			top_nominations.nominations = top_nominations
				.nominations
				.into_iter()
				.map(|d| {
					if d.owner == nominator {
						is_in_top = true;
						Bond { owner: d.owner, amount: d.amount.saturating_sub(less) }
					} else {
						d
					}
				})
				.collect();
			ensure!(is_in_top, Error::<T>::NominationDNE);
			top_nominations.total = top_nominations.total.saturating_sub(less);
			top_nominations.sort_greatest_to_least();
			true
		};
		self.reset_top_data::<T>(candidate.clone(), &top_nominations)?;
		<TopNominations<T>>::insert(candidate, top_nominations);
		Ok(in_top_after)
	}

	/// Decrease bottom nomination
	pub fn decrease_bottom_nomination<T: Config>(
		&mut self,
		candidate: &T::AccountId,
		nominator: T::AccountId,
		less: BalanceOf<T>,
	) -> Result<bool, DispatchError>
	where
		BalanceOf<T>: Into<Balance>,
	{
		let mut bottom_nominations =
			<BottomNominations<T>>::get(candidate).ok_or(<Error<T>>::BottomNominationDNE)?;
		let mut in_bottom = false;
		bottom_nominations.nominations = bottom_nominations
			.nominations
			.into_iter()
			.map(|d| {
				if d.owner == nominator {
					in_bottom = true;
					Bond { owner: d.owner, amount: d.amount.saturating_sub(less) }
				} else {
					d
				}
			})
			.collect();
		ensure!(in_bottom, Error::<T>::NominationDNE);
		bottom_nominations.sort_greatest_to_least();
		self.reset_bottom_data::<T>(&bottom_nominations);
		<BottomNominations<T>>::insert(candidate, bottom_nominations);
		Ok(false)
	}
}

/// Convey relevant information describing if a nominator was added to the top or bottom
/// Nominations added to the top yield a new total
#[derive(Clone, Copy, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum NominatorAdded<B> {
	AddedToTop { new_total: B },
	AddedToBottom,
}

#[derive(Clone, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum NominatorStatus {
	/// Active with no scheduled exit
	Active,
	/// Schedule exit to revoke all ongoing nominations
	Leaving(RoundIndex),
}

#[derive(Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
/// Nominator state
pub struct Nominator<AccountId, Balance> {
	/// Nominator account
	pub id: AccountId,
	/// Current state of all nominations
	pub nominations: BTreeMap<AccountId, Balance>,
	/// Initial state of all nominations
	pub initial_nominations: BTreeMap<AccountId, Balance>,
	/// Total balance locked for this nominator
	pub total: Balance,
	/// Requests to change nominations, relevant if active
	pub requests: PendingNominationRequests<AccountId, Balance>,
	/// Status for this nominator
	pub status: NominatorStatus,
	/// The destination for round rewards
	pub reward_dst: RewardDestination,
	/// The total amount of awarded tokens to this nominator
	pub awarded_tokens: Balance,
	/// The amount of awarded tokens to this nominator per candidate
	pub awarded_tokens_per_candidate: BTreeMap<AccountId, Balance>,
}

impl<
		AccountId: Ord + Clone,
		Balance: Copy
			+ sp_std::fmt::Debug
			+ Saturating
			+ sp_runtime::traits::AtLeast32BitUnsigned
			+ Ord
			+ Zero
			+ One
			+ Default,
	> Nominator<AccountId, Balance>
{
	pub fn new(id: AccountId, validator: AccountId, amount: Balance) -> Self {
		let nominations: BTreeMap<AccountId, Balance> =
			BTreeMap::from([(validator.clone(), amount)]);
		let initial_nominations: BTreeMap<AccountId, Balance> =
			BTreeMap::from([(validator.clone(), amount)]);
		let awarded_tokens_per_candidate: BTreeMap<AccountId, Balance> =
			BTreeMap::from([(validator.clone(), Zero::zero())]);
		Nominator {
			id,
			nominations,
			initial_nominations,
			total: amount,
			requests: PendingNominationRequests::new(),
			status: NominatorStatus::Active,
			reward_dst: RewardDestination::default(),
			awarded_tokens: Zero::zero(),
			awarded_tokens_per_candidate,
		}
	}

	pub fn requests(&self) -> BTreeMap<AccountId, NominationRequest<AccountId, Balance>> {
		self.requests.requests.clone()
	}

	pub fn is_active(&self) -> bool {
		matches!(self.status, NominatorStatus::Active)
	}

	pub fn is_revoking(&self, candidate: &AccountId) -> bool {
		if let Some(request) = self.requests().get(candidate) {
			if request.action == NominationChange::Revoke {
				return true;
			}
		}
		false
	}

	pub fn is_leaving(&self) -> bool {
		matches!(self.status, NominatorStatus::Leaving(_))
	}

	pub fn replace_nominations(&mut self, old: &AccountId, new: &AccountId) {
		if let Some(amount) = self.nominations.remove(old) {
			self.nominations.insert(new.clone(), amount);
		}

		if let Some(amount) = self.initial_nominations.remove(old) {
			self.initial_nominations.insert(new.clone(), amount);
		}
	}

	pub fn replace_requests(&mut self, old: &AccountId, new: &AccountId) {
		if let Some(request) = self.requests.requests.get(old) {
			let request_clone = request.clone();
			self.requests.requests.remove(old);
			self.requests.requests.insert(
				new.clone(),
				NominationRequest {
					validator: new.clone(),
					amount: request_clone.amount,
					when_executable: request_clone.when_executable,
					action: request_clone.action,
				},
			);
		}
	}

	pub fn increment_awarded_tokens(&mut self, validator: &AccountId, tokens: Balance) {
		if let Some(x) = self.awarded_tokens_per_candidate.get_mut(validator) {
			*x += tokens;
		}
		self.awarded_tokens += tokens;
	}

	/// Can only leave if the current round is less than or equal to scheduled execution round
	/// - returns None if not in leaving state
	pub fn can_execute_leave<T: Config>(&self, nomination_weight_hint: u32) -> DispatchResult {
		ensure!(
			nomination_weight_hint >= (self.nominations.len() as u32),
			Error::<T>::TooLowNominationCountToLeaveNominators
		);
		if let NominatorStatus::Leaving(when) = self.status {
			ensure!(
				<Round<T>>::get().current_round_index >= when,
				Error::<T>::NominatorCannotLeaveYet
			);
			Ok(())
		} else {
			Err(Error::<T>::NominatorNotLeaving.into())
		}
	}

	/// Set status to leaving
	pub(crate) fn set_leaving(&mut self, when: RoundIndex) {
		self.status = NominatorStatus::Leaving(when);
	}

	pub fn set_reward_dst(&mut self, reward_dst: RewardDestination) {
		self.reward_dst = reward_dst;
	}

	/// Schedule status to exit
	pub fn schedule_leave<T: Config>(&mut self) -> (RoundIndex, RoundIndex) {
		let now = <Round<T>>::get().current_round_index;
		let when = now + T::LeaveNominatorsDelay::get();
		self.set_leaving(when);
		(now, when)
	}

	/// Set nominator status to active
	pub fn cancel_leave(&mut self) {
		self.status = NominatorStatus::Active
	}

	// pub fn add_nomination(&mut self, bond: Bond<AccountId, Balance>) -> bool {
	pub fn add_nomination<T: Config>(
		&mut self,
		candidate: AccountId,
		amount: Balance,
	) -> DispatchResult {
		if let Some(_) = self.nominations.insert(candidate.clone(), amount) {
			self.total += amount;
			self.initial_nominations.insert(candidate.clone(), amount);
			self.awarded_tokens_per_candidate.insert(candidate.clone(), Zero::zero());
			Ok(())
		} else {
			Err(<Error<T>>::AlreadyNominatedCandidate.into())
		}
	}

	// Return Some(remaining balance), must be more than MinNominatorStk
	// Return None if nomination not found
	pub fn rm_nomination(&mut self, validator: &AccountId) -> Option<Balance> {
		if let Some(amount) = self.nominations.remove(validator) {
			self.initial_nominations.remove(validator);
			self.awarded_tokens_per_candidate.remove(validator);

			self.total = self.total.saturating_sub(amount);
			Some(self.total)
		} else {
			None
		}
	}

	pub fn increase_nomination<T: Config>(
		&mut self,
		candidate: AccountId,
		amount: Balance,
	) -> DispatchResult
	where
		BalanceOf<T>: From<Balance>,
		T::AccountId: From<AccountId>,
		Nominator<T::AccountId, BalanceOf<T>>: From<Nominator<AccountId, Balance>>,
	{
		let nominator_id: T::AccountId = self.id.clone().into();
		let candidate_id: T::AccountId = candidate.clone().into();
		let balance_amt: BalanceOf<T> = amount.into();
		// increase nomination
		if let Some(candidate_amount) = self.nominations.get_mut(&candidate) {
			let before_amount = candidate_amount.clone();
			*candidate_amount += amount;
			self.total += amount;
			// update validator state nomination
			let mut validator_state =
				<CandidateInfo<T>>::get(&candidate_id).ok_or(Error::<T>::CandidateDNE)?;
			T::Currency::reserve(&self.id.clone().into(), balance_amt)?;
			let in_top = validator_state.increase_nomination::<T>(
				&candidate_id,
				nominator_id.clone(),
				before_amount.into(),
				balance_amt,
			)?;
			let after = validator_state.voting_power;
			Pallet::<T>::update_active(&candidate_id, after)?;
			let new_total_staked = <Total<T>>::get().saturating_add(balance_amt);
			let nom_st: Nominator<T::AccountId, BalanceOf<T>> = self.clone().into();
			<Total<T>>::put(new_total_staked);
			<CandidateInfo<T>>::insert(&candidate_id, validator_state);
			<NominatorState<T>>::insert(&nominator_id, nom_st);
			Pallet::<T>::deposit_event(Event::NominationIncreased {
				nominator: nominator_id,
				candidate: candidate_id,
				amount: balance_amt,
				in_top,
			});
			return Ok(());
		}
		Err(Error::<T>::NominationDNE.into())
	}

	/// Schedule decrease nomination
	pub fn schedule_decrease_nomination<T: Config>(
		&mut self,
		validator: AccountId,
		less: Balance,
	) -> Result<RoundIndex, DispatchError>
	where
		BalanceOf<T>: Into<Balance> + From<Balance>,
	{
		// get nomination amount
		return if let Some(amount) = self.nominations.get(&validator) {
			ensure!(*amount > less, Error::<T>::NominatorBondBelowMin);
			let expected_amt: BalanceOf<T> = (*amount - less).into();
			ensure!(expected_amt >= T::MinNomination::get(), Error::<T>::NominationBelowMin);
			// Net Total is total after pending orders are executed
			let net_total = self.total - self.requests.less_total;
			// Net Total is always >= MinNominatorStk
			let max_subtracted_amount = net_total - T::MinNominatorStk::get().into();
			ensure!(less <= max_subtracted_amount, Error::<T>::NominatorBondBelowMin);
			let when = <Round<T>>::get().current_round_index + T::NominationBondLessDelay::get();
			self.requests.bond_less::<T>(validator, less, when)?;
			Ok(when)
		} else {
			Err(Error::<T>::NominationDNE.into())
		};
	}

	/// Schedule revocation for the given validator
	pub fn schedule_revoke<T: Config>(
		&mut self,
		validator: AccountId,
	) -> Result<(RoundIndex, RoundIndex), DispatchError>
	where
		BalanceOf<T>: Into<Balance>,
	{
		// get nomination amount
		return if let Some(amount) = self.nominations.get(&validator) {
			let now = <Round<T>>::get().current_round_index;
			let when = now + T::RevokeNominationDelay::get();
			// add revocation to pending requests
			self.requests.revoke::<T>(validator, *amount, when)?;
			Ok((now, when))
		} else {
			Err(Error::<T>::NominationDNE.into())
		};
	}

	/// Execute pending nomination change request
	pub fn execute_pending_request<T: Config>(&mut self, candidate: AccountId) -> DispatchResult
	where
		BalanceOf<T>: From<Balance> + Into<Balance>,
		T::AccountId: From<AccountId>,
		Nominator<T::AccountId, BalanceOf<T>>: From<Nominator<AccountId, Balance>>,
	{
		let now = <Round<T>>::get().current_round_index;
		let NominationRequest { amount, action, when_executable, .. } = self
			.requests
			.requests
			.remove(&candidate)
			.ok_or(Error::<T>::PendingNominationRequestDNE)?;
		ensure!(when_executable <= now, Error::<T>::PendingNominationRequestNotDueYet);
		let (balance_amt, candidate_id, nominator_id): (BalanceOf<T>, T::AccountId, T::AccountId) =
			(amount.into(), candidate.clone().into(), self.id.clone().into());
		match action {
			NominationChange::Revoke => {
				// revoking last nomination => leaving set of nominators
				let leaving = if self.nominations.len() == 1usize {
					true
				} else {
					ensure!(
						self.total - T::MinNominatorStk::get().into() >= amount,
						Error::<T>::NominatorBondBelowMin
					);
					false
				};
				// remove from pending requests
				self.requests.less_total = self.requests.less_total.saturating_sub(amount);
				self.requests.revocations_count =
					self.requests.revocations_count.saturating_sub(1u32);
				// remove nomination from nominator state
				self.rm_nomination(&candidate);
				// remove nomination from validator state nominations
				Pallet::<T>::nominator_leaves_candidate(
					candidate_id.clone(),
					nominator_id.clone(),
					balance_amt,
				)?;
				Pallet::<T>::deposit_event(Event::NominationRevoked {
					nominator: nominator_id.clone(),
					candidate: candidate_id,
					unstaked_amount: balance_amt,
				});
				if leaving {
					<NominatorState<T>>::remove(&nominator_id);
					Pallet::<T>::deposit_event(Event::NominatorLeft {
						nominator: nominator_id,
						unstaked_amount: balance_amt,
					});
				} else {
					let nom_st: Nominator<T::AccountId, BalanceOf<T>> = self.clone().into();
					<NominatorState<T>>::insert(&nominator_id, nom_st);
				}
				Ok(())
			},
			NominationChange::Decrease => {
				// remove from pending requests
				self.requests.less_total = self.requests.less_total.saturating_sub(amount);
				// decrease nomination
				return if let Some(candidate_amount) = self.nominations.get_mut(&candidate) {
					return if *candidate_amount > amount {
						let amount_before = candidate_amount.clone();
						*candidate_amount = candidate_amount.saturating_sub(amount);
						self.total = self.total.saturating_sub(amount);
						let new_total: BalanceOf<T> = self.total.into();
						ensure!(
							new_total >= T::MinNomination::get(),
							Error::<T>::NominationBelowMin
						);
						ensure!(
							new_total >= T::MinNominatorStk::get(),
							Error::<T>::NominatorBondBelowMin
						);
						let mut validator = <CandidateInfo<T>>::get(&candidate_id)
							.ok_or(Error::<T>::CandidateDNE)?;
						T::Currency::unreserve(&nominator_id, balance_amt);
						// need to go into decrease_nomination
						let in_top = validator.decrease_nomination::<T>(
							&candidate_id,
							nominator_id.clone(),
							amount_before.into(),
							balance_amt,
						)?;
						<CandidateInfo<T>>::insert(&candidate_id, validator);
						let new_total_staked = <Total<T>>::get().saturating_sub(balance_amt);
						<Total<T>>::put(new_total_staked);
						let nom_st: Nominator<T::AccountId, BalanceOf<T>> = self.clone().into();
						<NominatorState<T>>::insert(&nominator_id, nom_st);
						Pallet::<T>::deposit_event(Event::NominationDecreased {
							nominator: nominator_id,
							candidate: candidate_id,
							amount: balance_amt,
							in_top,
						});
						Ok(())
					} else {
						// must rm entire nomination if x.amount <= less or cancel request
						Err(Error::<T>::NominationBelowMin.into())
					};
				} else {
					Err(Error::<T>::NominationDNE.into())
				};
			},
		}
	}

	/// Cancel pending nomination change request
	pub fn cancel_pending_request<T: Config>(
		&mut self,
		candidate: AccountId,
	) -> Result<NominationRequest<AccountId, Balance>, DispatchError> {
		let order = self
			.requests
			.requests
			.remove(&candidate)
			.ok_or(Error::<T>::PendingNominationRequestDNE)?;
		match order.action {
			NominationChange::Revoke => {
				self.requests.revocations_count =
					self.requests.revocations_count.saturating_sub(1u32);
				self.requests.less_total = self.requests.less_total.saturating_sub(order.amount);
			},
			NominationChange::Decrease => {
				self.requests.less_total = self.requests.less_total.saturating_sub(order.amount);
			},
		}
		Ok(order)
	}
}

#[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo)]
/// Changes requested by the nominator
/// - limit of 1 ongoing change per nomination
pub enum NominationChange {
	/// Requests to unbond the entire nomination
	Revoke,
	/// Requests to unbond a certain amount of nomination
	Decrease,
}

#[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo)]
/// The nomination unbonding request of a specific nominator
pub struct NominationRequest<AccountId, Balance> {
	/// The validator who owns this nomination
	pub validator: AccountId,
	/// The unbonding amount
	pub amount: Balance,
	/// The round index when this request is executable
	pub when_executable: RoundIndex,
	/// The requested unbonding action
	pub action: NominationChange,
}

#[derive(Clone, Encode, PartialEq, Decode, RuntimeDebug, TypeInfo)]
/// Pending requests to mutate nominations for each nominator
pub struct PendingNominationRequests<AccountId, Balance> {
	/// Number of pending revocations (necessary for determining whether revoke is exit)
	pub revocations_count: u32,
	/// Map from validator -> Request (enforces at most 1 pending request per nomination)
	pub requests: BTreeMap<AccountId, NominationRequest<AccountId, Balance>>,
	/// Sum of pending revocation amounts + bond less amounts
	pub less_total: Balance,
}

impl<A: Ord, B: Zero> Default for PendingNominationRequests<A, B> {
	fn default() -> PendingNominationRequests<A, B> {
		PendingNominationRequests {
			revocations_count: 0u32,
			requests: BTreeMap::new(),
			less_total: B::zero(),
		}
	}
}

impl<
		A: Parameter + Member + MaybeSerializeDeserialize + Debug + MaybeDisplay + Ord + MaxEncodedLen,
		B: Balance + MaybeSerializeDeserialize + Debug + MaxEncodedLen + FixedPointOperand,
	> PendingNominationRequests<A, B>
{
	pub fn remove_request(&mut self, address: &A) {
		if let Some(request) = self.requests.remove(address) {
			self.less_total = self.less_total.saturating_sub(request.amount);
			if matches!(request.action, NominationChange::Revoke) {
				self.revocations_count = self.revocations_count.saturating_sub(1u32);
			}
		}
	}
}

impl<
		A: Ord + Clone,
		B: Zero
			+ Ord
			+ Copy
			+ Clone
			+ sp_std::ops::AddAssign
			+ sp_std::ops::Add<Output = B>
			+ sp_std::ops::SubAssign
			+ sp_std::ops::Sub<Output = B>,
	> PendingNominationRequests<A, B>
{
	/// New default (empty) pending requests
	pub fn new() -> PendingNominationRequests<A, B> {
		PendingNominationRequests::default()
	}

	/// Add bond less order to pending requests, only succeeds if returns true
	/// - limit is the maximum amount allowed that can be subtracted from the nomination
	/// before it would be below the minimum nomination amount
	pub fn bond_less<T: Config>(
		&mut self,
		validator: A,
		amount: B,
		when_executable: RoundIndex,
	) -> DispatchResult {
		ensure!(
			self.requests.get(&validator).is_none(),
			Error::<T>::PendingNominationRequestAlreadyExists
		);
		self.requests.insert(
			validator.clone(),
			NominationRequest {
				validator,
				amount,
				when_executable,
				action: NominationChange::Decrease,
			},
		);
		self.less_total += amount;
		Ok(())
	}

	/// Add revoke order to pending requests
	/// - limit is the maximum amount allowed that can be subtracted from the nomination
	/// before it would be below the minimum nomination amount
	pub fn revoke<T: Config>(
		&mut self,
		validator: A,
		amount: B,
		when_executable: RoundIndex,
	) -> DispatchResult {
		ensure!(
			self.requests.get(&validator).is_none(),
			Error::<T>::PendingNominationRequestAlreadyExists
		);
		self.requests.insert(
			validator.clone(),
			NominationRequest {
				validator,
				amount,
				when_executable,
				action: NominationChange::Revoke,
			},
		);
		self.revocations_count += 1u32;
		self.less_total += amount;
		Ok(())
	}
}

#[derive(Copy, Clone, PartialEq, Eq, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
/// The current round index and transition information
pub struct RoundInfo<BlockNumber> {
	/// Current round index
	pub current_round_index: RoundIndex,
	/// Current round first session index
	pub first_session_index: SessionIndex,
	/// Current round current session index
	pub current_session_index: SessionIndex,
	/// The first block of the current round
	pub first_round_block: BlockNumber,
	/// The first block of the current session
	pub first_session_block: BlockNumber,
	/// The current block of the current round
	pub current_block: BlockNumber,
	/// The length of the current round in number of blocks
	pub round_length: u32,
	/// The length of the current session in number of blocks
	pub session_length: u32,
}
impl<
		B: Copy
			+ sp_std::ops::Add<Output = B>
			+ sp_std::ops::Sub<Output = B>
			+ From<u32>
			+ PartialOrd
			+ sp_std::fmt::Debug,
	> RoundInfo<B>
{
	pub fn new(
		current_round_index: RoundIndex,
		first_session_index: SessionIndex,
		current_session_index: SessionIndex,
		first_round_block: B,
		first_session_block: B,
		current_block: B,
		round_length: u32,
		session_length: u32,
	) -> RoundInfo<B> {
		RoundInfo {
			current_round_index,
			first_session_index,
			current_session_index,
			first_round_block,
			first_session_block,
			current_block,
			round_length,
			session_length,
		}
	}

	/// Check if the round should be updated
	pub fn should_update(&self, now: B) -> bool {
		now - self.first_round_block >= self.round_length.into()
	}

	/// New round
	pub fn update_round<T: Config>(&mut self, now: B) {
		self.current_round_index += 1;
		self.first_session_index = Session::<T>::get();
		self.current_session_index = Session::<T>::get();
		self.first_round_block = now;
		self.first_session_block = now;
		self.current_block = now;
	}

	/// New session
	pub fn update_session<T: Config>(&mut self, now: B, new_session: SessionIndex) {
		self.current_session_index = new_session;
		self.first_session_block = now;
	}

	/// New block
	pub fn update_block(&mut self, now: B) {
		self.current_block = now;
	}
}
impl<
		B: Copy
			+ sp_std::ops::Add<Output = B>
			+ sp_std::ops::Sub<Output = B>
			+ From<u32>
			+ PartialOrd
			+ sp_std::fmt::Debug,
	> Default for RoundInfo<B>
{
	fn default() -> RoundInfo<B> {
		RoundInfo::new(1u32, 1u32, 1u32, 0u32.into(), 0u32.into(), 0u32.into(), 20u32, 200u32)
	}
}
