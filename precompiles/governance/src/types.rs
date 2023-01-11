use frame_support::traits::Currency;

use pallet_democracy::{Conviction, PropIndex};

use precompile_utils::prelude::Address;

use sp_core::{H160, H256};

pub type BalanceOf<Runtime> = <<Runtime as pallet_democracy::Config>::Currency as Currency<
	<Runtime as frame_system::Config>::AccountId,
>>::Balance;

pub type BlockNumberOf<Runtime> = <Runtime as frame_system::Config>::BlockNumber;

pub type HashOf<Runtime> = <Runtime as frame_system::Config>::Hash;

pub type DemocracyOf<Runtime> = pallet_democracy::Pallet<Runtime>;

pub type EvmPublicProposalsOf = (Vec<PropIndex>, Vec<H256>, Vec<Address>);

/// EVM struct for referenda voting information
pub struct ReferendaVotes<Runtime: pallet_democracy::Config> {
	/// The index of this referenda
	pub ref_index: u32,
	/// The voter addresses of this referenda
	pub voters: Vec<Address>,
	/// The raw votes submitted for each voters (conviction not applied)
	pub raw_votes: Vec<BalanceOf<Runtime>>,
	/// The voting side of each voters (true: aye, false: nay)
	pub voting_sides: Vec<bool>,
	/// The conviction of each voters (0~6)
	pub convictions: Vec<u32>,
}

impl<Runtime> ReferendaVotes<Runtime>
where
	Runtime: pallet_democracy::Config,
	Runtime::AccountId: Into<H160>,
{
	pub fn default(ref_index: u32) -> Self {
		ReferendaVotes {
			ref_index,
			voters: vec![],
			raw_votes: vec![],
			voting_sides: vec![],
			convictions: vec![],
		}
	}

	pub fn insert_vote(
		&mut self,
		voter: Runtime::AccountId,
		raw_vote: BalanceOf<Runtime>,
		voting_side: bool,
		conviction: Conviction,
	) {
		self.voters.push(Address(voter.into()));
		self.raw_votes.push(raw_vote);
		self.voting_sides.push(voting_side);
		let raw_conviction = match conviction {
			Conviction::None => 0u32,
			Conviction::Locked1x => 1u32,
			Conviction::Locked2x => 2u32,
			Conviction::Locked3x => 3u32,
			Conviction::Locked4x => 4u32,
			Conviction::Locked5x => 5u32,
			Conviction::Locked6x => 6u32,
		};
		self.convictions.push(raw_conviction);
	}
}

/// EVM struct for account voting information
pub struct AccountVotes<Runtime: pallet_democracy::Config> {
	/// The index of voted referendas (removable)
	pub ref_index: Vec<u32>,
	/// The raw votes submitted for each referenda (conviction not applied)
	pub raw_votes: Vec<BalanceOf<Runtime>>,
	/// The voting side of each referenda (true: aye, false: nay)
	pub voting_sides: Vec<bool>,
	/// The conviction multiplier of each votes (0~6)
	pub convictions: Vec<u32>,
	/// The delegated amount of votes received for this account (conviction applied)
	pub delegated_votes: BalanceOf<Runtime>,
	/// The delegated raw amount of votes received for this account (conviction not applied)
	pub delegated_raw_votes: BalanceOf<Runtime>,
	/// The block number that expires the locked balance
	pub lock_expired_at: BlockNumberOf<Runtime>,
	/// The balance locked to the network
	pub lock_balance: BalanceOf<Runtime>,
}

impl<Runtime> AccountVotes<Runtime>
where
	Runtime: pallet_democracy::Config,
	Runtime::AccountId: Into<H160>,
{
	pub fn default() -> Self {
		let zero = 0u32;
		AccountVotes {
			ref_index: vec![],
			raw_votes: vec![],
			voting_sides: vec![],
			convictions: vec![],
			delegated_votes: zero.into(),
			delegated_raw_votes: zero.into(),
			lock_expired_at: zero.into(),
			lock_balance: zero.into(),
		}
	}

	pub fn insert_vote(
		&mut self,
		ref_index: u32,
		raw_vote: BalanceOf<Runtime>,
		voting_side: bool,
		conviction: Conviction,
	) {
		self.ref_index.push(ref_index);
		self.raw_votes.push(raw_vote);
		self.voting_sides.push(voting_side);
		let raw_conviction = match conviction {
			Conviction::None => 0u32,
			Conviction::Locked1x => 1u32,
			Conviction::Locked2x => 2u32,
			Conviction::Locked3x => 3u32,
			Conviction::Locked4x => 4u32,
			Conviction::Locked5x => 5u32,
			Conviction::Locked6x => 6u32,
		};
		self.convictions.push(raw_conviction);
	}

	pub fn set_delegations(
		&mut self,
		delegated_votes: BalanceOf<Runtime>,
		delegated_raw_votes: BalanceOf<Runtime>,
	) {
		self.delegated_votes = delegated_votes;
		self.delegated_raw_votes = delegated_raw_votes;
	}

	pub fn set_expiration(
		&mut self,
		lock_expired_at: BlockNumberOf<Runtime>,
		lock_balance: BalanceOf<Runtime>,
	) {
		self.lock_expired_at = lock_expired_at;
		self.lock_balance = lock_balance;
	}
}
