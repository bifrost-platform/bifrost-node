#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(test, feature(assert_matches))]

use frame_support::{
	dispatch::{Dispatchable, GetDispatchInfo, PostDispatchInfo},
	traits::Currency,
};

use fp_evm::{Context, ExitError, ExitSucceed, PrecompileFailure, PrecompileOutput};
use pallet_democracy::{
	AccountVote, Call as DemocracyCall, Conviction, ReferendumInfo, Vote, VoteThreshold, Voting,
};
use pallet_evm::{AddressMapping, Precompile};
use precompile_utils::{
	Address, Bytes, EvmData, EvmDataReader, EvmDataWriter, EvmResult, FunctionModifier, Gasometer,
	RuntimeHelper,
};

use sp_core::{H160, H256, U256};
use sp_std::{
	convert::{TryFrom, TryInto},
	fmt::Debug,
	marker::PhantomData,
	vec,
	vec::Vec,
};

type BalanceOf<Runtime> = <<Runtime as pallet_democracy::Config>::Currency as Currency<
	<Runtime as frame_system::Config>::AccountId,
>>::Balance;

type BlockNumberOf<Runtime> = <Runtime as frame_system::Config>::BlockNumber;

type HashOf<Runtime> = <Runtime as frame_system::Config>::Hash;

type DemocracyOf<Runtime> = pallet_democracy::Pallet<Runtime>;

#[precompile_utils::generate_function_selector]
#[derive(Debug, PartialEq)]
enum Action {
	// Storage getters
	PublicPropCount = "public_prop_count()",
	PublicProps = "public_props()",
	DepositOf = "deposit_of(uint256)",
	VotingOf = "voting_of(uint256)",
	AccountVotes = "account_votes(address)",
	LowestUnbaked = "lowest_unbaked()",
	OngoingReferendumInfo = "ongoing_referendum_info(uint256)",
	FinishedReferendumInfo = "finished_referendum_info(uint256)",
	// Dispatchable methods
	Propose = "propose(bytes32,uint256)",
	Second = "second(uint256,uint256)",
	Vote = "vote(uint256,bool,uint256,uint256)",
	RemoveVote = "remove_vote(uint256)",
	Delegate = "delegate(address,uint256,uint256)",
	Undelegate = "undelegate()",
	Unlock = "unlock(address)",
	NotePreimage = "note_preimage(bytes)",
	NoteImminentPreimage = "note_imminent_preimage(bytes)",
	// Dispatchable methods for council members
	ExternalPropose = "external_propose(bytes32)",
	ExternalProposeMajority = "external_propose_majority(bytes32)",
	ExternalProposeDefault = "external_propose_default(bytes32)",
	EmergencyCancel = "emergency_cancel(uint256)",
}

/// EVM struct for referenda voting information
struct ReferendaVotes<Runtime: pallet_democracy::Config> {
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
	fn default(ref_index: u32) -> Self {
		ReferendaVotes {
			ref_index,
			voters: vec![],
			raw_votes: vec![],
			voting_sides: vec![],
			convictions: vec![],
		}
	}

	fn insert_vote(
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
struct AccountVotes<Runtime: pallet_democracy::Config> {
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
	fn default() -> Self {
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

	fn insert_vote(
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

	fn set_delegations(
		&mut self,
		delegated_votes: BalanceOf<Runtime>,
		delegated_raw_votes: BalanceOf<Runtime>,
	) {
		self.delegated_votes = delegated_votes;
		self.delegated_raw_votes = delegated_raw_votes;
	}

	fn set_expiration(
		&mut self,
		lock_expired_at: BlockNumberOf<Runtime>,
		lock_balance: BalanceOf<Runtime>,
	) {
		self.lock_expired_at = lock_expired_at;
		self.lock_balance = lock_balance;
	}
}

/// A precompile to wrap the functionality from governance related pallets.
pub struct GovernancePrecompile<Runtime>(PhantomData<Runtime>);

impl<Runtime> Precompile for GovernancePrecompile<Runtime>
where
	Runtime: pallet_democracy::Config + pallet_evm::Config + frame_system::Config,
	BalanceOf<Runtime>: TryFrom<U256> + TryInto<u128> + Debug + EvmData,
	BlockNumberOf<Runtime>: EvmData,
	HashOf<Runtime>: EvmData,
	Runtime::Call: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	<Runtime::Call as Dispatchable>::Origin: From<Option<Runtime::AccountId>>,
	Runtime::Call: From<DemocracyCall<Runtime>>,
	Runtime::Hash: From<H256> + Into<H256>,
	Runtime::AccountId: Into<H160>,
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
				Action::Propose |
				Action::Second |
				Action::Vote |
				Action::RemoveVote |
				Action::Delegate |
				Action::Undelegate |
				Action::Unlock |
				Action::NotePreimage |
				Action::NoteImminentPreimage => FunctionModifier::NonPayable,
				_ => FunctionModifier::View,
			},
		)?;

		let (origin, call) = match selector {
			// Storage getters
			Action::PublicPropCount => return Self::public_prop_count(gasometer),
			Action::PublicProps => return Self::public_props(gasometer),
			Action::DepositOf => return Self::deposit_of(input, gasometer),
			Action::VotingOf => return Self::voting_of(input, gasometer),
			Action::AccountVotes => return Self::account_votes(input, gasometer),
			Action::LowestUnbaked => return Self::lowest_unbaked(gasometer),
			Action::OngoingReferendumInfo => return Self::ongoing_referendum_info(input, gasometer),
			Action::FinishedReferendumInfo =>
				return Self::finished_referendum_info(input, gasometer),

			// Dispatchable methods
			Action::Propose => Self::propose(input, gasometer, context)?,
			Action::Second => Self::second(input, gasometer, context)?,
			Action::Vote => Self::vote(input, gasometer, context)?,
			Action::RemoveVote => Self::remove_vote(input, gasometer, context)?,
			Action::Delegate => Self::delegate(input, gasometer, context)?,
			Action::Undelegate => Self::undelegate(context)?,
			Action::Unlock => Self::unlock(input, gasometer, context)?,
			Action::NotePreimage => Self::note_preimage(input, gasometer, context)?,
			Action::NoteImminentPreimage =>
				Self::note_imminent_preimage(input, gasometer, context)?,

			// Dispatchable methods for council members
			Action::ExternalPropose => Self::external_propose(input, gasometer, context)?,
			Action::ExternalProposeMajority =>
				Self::external_propose_majority(input, gasometer, context)?,
			Action::ExternalProposeDefault =>
				Self::external_propose_default(input, gasometer, context)?,
			Action::EmergencyCancel => Self::emergency_cancel(input, gasometer, context)?,
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

impl<Runtime> GovernancePrecompile<Runtime>
where
	Runtime: pallet_democracy::Config + pallet_evm::Config + frame_system::Config,
	BalanceOf<Runtime>: TryFrom<U256> + TryInto<u128> + Debug + EvmData,
	BlockNumberOf<Runtime>: EvmData,
	HashOf<Runtime>: EvmData,
	Runtime::Call: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	<Runtime::Call as Dispatchable>::Origin: From<Option<Runtime::AccountId>>,
	Runtime::Call: From<DemocracyCall<Runtime>>,
	Runtime::Hash: From<H256>,
	Runtime::AccountId: Into<H160>,
{
	// Storage getters

	fn public_prop_count(gasometer: &mut Gasometer) -> EvmResult<PrecompileOutput> {
		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let prop_count = DemocracyOf::<Runtime>::public_prop_count();

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(prop_count).build(),
			logs: Default::default(),
		})
	}

	fn public_props(gasometer: &mut Gasometer) -> EvmResult<PrecompileOutput> {
		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let mut prop_index: Vec<u32> = vec![];
		let mut prop_hash: Vec<HashOf<Runtime>> = vec![];
		let mut proposer: Vec<Address> = vec![];

		let public_props = DemocracyOf::<Runtime>::public_props();
		for prop in public_props {
			prop_index.push(prop.0.into());
			prop_hash.push(prop.1.into());
			proposer.push(Address(prop.2.into()));
		}

		let output =
			EvmDataWriter::new().write(prop_index).write(prop_hash).write(proposer).build();

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output,
			logs: Default::default(),
		})
	}

	fn deposit_of(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 1)?;
		let prop_index: u32 = input.read(gasometer)?;

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let zero = 0u32;
		let mut total_deposit: BalanceOf<Runtime> = zero.into();
		let mut initial_deposit: BalanceOf<Runtime> = zero.into();
		let mut depositors: Vec<Address> = vec![];

		if let Some(deposit_of) = DemocracyOf::<Runtime>::deposit_of(prop_index) {
			initial_deposit = deposit_of.1.into();
			for depositor in deposit_of.0 {
				depositors.push(Address(depositor.into()));
				total_deposit += initial_deposit;
			}
		}

		let output = EvmDataWriter::new()
			.write(total_deposit)
			.write(initial_deposit)
			.write(depositors)
			.build();

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output,
			logs: Default::default(),
		})
	}

	fn voting_of(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 1)?;
		let ref_index: u32 = input.read(gasometer)?;

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let mut referenda_votes = ReferendaVotes::<Runtime>::default(ref_index);

		for voting_of in pallet_democracy::VotingOf::<Runtime>::iter() {
			let voter: Runtime::AccountId = voting_of.0;
			let state = voting_of.1;

			match state {
				Voting::Direct { votes, .. } =>
					for direct_vote in votes {
						if direct_vote.0 == ref_index {
							let account_vote = direct_vote.1;
							match account_vote {
								AccountVote::Standard { vote, balance } => {
									referenda_votes.insert_vote(
										voter,
										balance,
										vote.aye,
										vote.conviction,
									);
								},
								AccountVote::Split { .. } => (),
							};
							break
						}
					},
				Voting::Delegating { .. } => (),
			};
		}

		let output = EvmDataWriter::new()
			.write(referenda_votes.ref_index)
			.write(referenda_votes.voters)
			.write(referenda_votes.raw_votes)
			.write(referenda_votes.voting_sides)
			.write(referenda_votes.convictions)
			.build();

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output,
			logs: Default::default(),
		})
	}

	fn account_votes(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 1)?;
		let account = input.read::<Address>(gasometer)?.0;
		let account = Runtime::AddressMapping::into_account_id(account);

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let mut account_votes = AccountVotes::<Runtime>::default();

		match pallet_democracy::VotingOf::<Runtime>::get(account) {
			Voting::Direct { votes, delegations, prior } => {
				account_votes.set_delegations(delegations.votes, delegations.capital);
				account_votes.set_expiration(prior.expired_at(), prior.locked());
				for direct_vote in votes {
					let account_vote = direct_vote.1;
					match account_vote {
						AccountVote::Standard { vote, balance } => {
							account_votes.insert_vote(
								direct_vote.0,
								balance,
								vote.aye,
								vote.conviction,
							);
						},
						AccountVote::Split { .. } => (),
					};
				}
			},
			Voting::Delegating { .. } => (),
		};

		let output = EvmDataWriter::new()
			.write(account_votes.ref_index)
			.write(account_votes.raw_votes)
			.write(account_votes.voting_sides)
			.write(account_votes.convictions)
			.write(account_votes.delegated_votes)
			.write(account_votes.delegated_raw_votes)
			.write(account_votes.lock_expired_at)
			.write(account_votes.lock_balance)
			.build();

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output,
			logs: Default::default(),
		})
	}

	fn lowest_unbaked(gasometer: &mut Gasometer) -> EvmResult<PrecompileOutput> {
		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let lowest_unbaked = DemocracyOf::<Runtime>::lowest_unbaked();

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(lowest_unbaked).build(),
			logs: Default::default(),
		})
	}

	fn ongoing_referendum_info(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 1)?;
		let ref_index: u32 = input.read(gasometer)?;

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let ref_status = match DemocracyOf::<Runtime>::referendum_info(ref_index) {
			Some(ReferendumInfo::Ongoing(ref_status)) => ref_status,
			Some(ReferendumInfo::Finished { .. }) =>
				return Err(PrecompileFailure::Error {
					exit_status: ExitError::Other("Referendum is finished".into()),
				}),
			None =>
				return Err(PrecompileFailure::Error {
					exit_status: ExitError::Other(
						"failed to get ongoing (or finished for that matter) referendum".into(),
					),
				}),
		};

		let threshold: u8 = match ref_status.threshold {
			VoteThreshold::SuperMajorityApprove => 0,
			VoteThreshold::SuperMajorityAgainst => 1,
			VoteThreshold::SimpleMajority => 2,
		};

		let end: BlockNumberOf<Runtime> = ref_status.end.into();
		let prop_hash: HashOf<Runtime> = ref_status.proposal_hash.into();
		let delay: BlockNumberOf<Runtime> = ref_status.delay.into();
		let ayes: BalanceOf<Runtime> = ref_status.tally.ayes.into();
		let nays: BalanceOf<Runtime> = ref_status.tally.nays.into();
		let turnout: BalanceOf<Runtime> = ref_status.tally.turnout.into();

		let output = EvmDataWriter::new()
			.write(end)
			.write(prop_hash)
			.write(threshold)
			.write(delay)
			.write(ayes)
			.write(nays)
			.write(turnout)
			.build();

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output,
			logs: Default::default(),
		})
	}

	fn finished_referendum_info(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 1)?;
		let ref_index: u32 = input.read(gasometer)?;

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let ref_status = match DemocracyOf::<Runtime>::referendum_info(ref_index) {
			Some(ReferendumInfo::Finished { approved, end }) => (approved, end),
			Some(ReferendumInfo::Ongoing(..)) =>
				return Err(PrecompileFailure::Error {
					exit_status: ExitError::Other("Referendum is ongoing".into()),
				}),
			None =>
				return Err(PrecompileFailure::Error {
					exit_status: ExitError::Other(
						"failed to get ongoing (or finished for that matter) referendum".into(),
					),
				}),
		};

		let approved: bool = ref_status.0.into();
		let end: BlockNumberOf<Runtime> = ref_status.1.into();

		let output = EvmDataWriter::new().write(approved).write(end).build();

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output,
			logs: Default::default(),
		})
	}

	// Dispatchable methods

	fn propose(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, DemocracyCall<Runtime>)> {
		input.expect_arguments(gasometer, 2)?;

		let proposal_hash = input.read::<H256>(gasometer)?.into();
		let amount = input.read::<BalanceOf<Runtime>>(gasometer)?;

		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call = DemocracyCall::<Runtime>::propose { proposal_hash, value: amount };

		Ok((Some(origin).into(), call))
	}

	fn second(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, DemocracyCall<Runtime>)> {
		input.expect_arguments(gasometer, 2)?;

		let proposal = input.read(gasometer)?;
		let seconds_upper_bound = input.read(gasometer)?;

		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call = DemocracyCall::<Runtime>::second { proposal, seconds_upper_bound };

		Ok((Some(origin).into(), call))
	}

	fn vote(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, DemocracyCall<Runtime>)> {
		input.expect_arguments(gasometer, 4)?;

		let ref_index = input.read(gasometer)?;
		let aye = input.read(gasometer)?;
		let balance = input.read(gasometer)?;
		let conviction = input
			.read::<u8>(gasometer)?
			.try_into()
			.map_err(|_| gasometer.revert("Conviction must be an integer in the range 0-6"))?;
		let vote = AccountVote::Standard { vote: Vote { aye, conviction }, balance };

		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call = DemocracyCall::<Runtime>::vote { ref_index, vote };

		Ok((Some(origin).into(), call))
	}

	fn remove_vote(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, DemocracyCall<Runtime>)> {
		input.expect_arguments(gasometer, 1)?;

		let referendum_index = input.read(gasometer)?;

		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call = DemocracyCall::<Runtime>::remove_vote { index: referendum_index };

		Ok((Some(origin).into(), call))
	}

	fn delegate(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, DemocracyCall<Runtime>)> {
		input.expect_arguments(gasometer, 3)?;

		let to: H160 = input.read::<Address>(gasometer)?.into();
		let to = Runtime::AddressMapping::into_account_id(to);
		let conviction = input
			.read::<u8>(gasometer)?
			.try_into()
			.map_err(|_| gasometer.revert("Conviction must be an integer in the range 0-6"))?;
		let balance = input.read(gasometer)?;

		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call = DemocracyCall::<Runtime>::delegate { to, conviction, balance };

		Ok((Some(origin).into(), call))
	}

	fn undelegate(
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, DemocracyCall<Runtime>)> {
		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call = DemocracyCall::<Runtime>::undelegate {};

		Ok((Some(origin).into(), call))
	}

	fn unlock(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, DemocracyCall<Runtime>)> {
		input.expect_arguments(gasometer, 1)?;

		let target: H160 = input.read::<Address>(gasometer)?.into();
		let target = Runtime::AddressMapping::into_account_id(target);

		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call = DemocracyCall::<Runtime>::unlock { target };

		Ok((Some(origin).into(), call))
	}

	fn note_preimage(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, DemocracyCall<Runtime>)> {
		let encoded_proposal: Vec<u8> = input.read::<Bytes>(gasometer)?.into();

		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call = DemocracyCall::<Runtime>::note_preimage { encoded_proposal };

		Ok((Some(origin).into(), call))
	}

	fn note_imminent_preimage(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, DemocracyCall<Runtime>)> {
		let encoded_proposal: Vec<u8> = input.read::<Bytes>(gasometer)?.into();

		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call = DemocracyCall::<Runtime>::note_imminent_preimage { encoded_proposal };

		Ok((Some(origin).into(), call))
	}

	// Dispatchable methods for council members

	fn external_propose(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, DemocracyCall<Runtime>)> {
		input.expect_arguments(gasometer, 1)?;

		let proposal_hash = input.read::<H256>(gasometer)?.into();

		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call = DemocracyCall::<Runtime>::external_propose { proposal_hash };

		Ok((Some(origin).into(), call))
	}

	fn external_propose_majority(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, DemocracyCall<Runtime>)> {
		input.expect_arguments(gasometer, 1)?;

		let proposal_hash = input.read::<H256>(gasometer)?.into();

		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call = DemocracyCall::<Runtime>::external_propose_majority { proposal_hash };

		Ok((Some(origin).into(), call))
	}

	fn external_propose_default(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, DemocracyCall<Runtime>)> {
		input.expect_arguments(gasometer, 1)?;

		let proposal_hash = input.read::<H256>(gasometer)?.into();

		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call = DemocracyCall::<Runtime>::external_propose_default { proposal_hash };

		Ok((Some(origin).into(), call))
	}

	fn emergency_cancel(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, DemocracyCall<Runtime>)> {
		input.expect_arguments(gasometer, 1)?;

		let ref_index = input.read(gasometer)?;

		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call = DemocracyCall::<Runtime>::emergency_cancel { ref_index };

		Ok((Some(origin).into(), call))
	}
}
