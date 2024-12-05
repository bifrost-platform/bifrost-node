#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{
	dispatch::{GetDispatchInfo, PostDispatchInfo},
	traits::{Bounded, QueryPreimage},
};
use frame_system::pallet_prelude::BlockNumberFor;

use pallet_democracy::{
	AccountVote, Call as DemocracyCall, Conviction, ReferendumInfo, Vote, VoteThreshold, Voting,
};
use pallet_evm::AddressMapping;
use pallet_preimage::Call as PreimageCall;

use precompile_utils::prelude::*;

use sp_core::{H160, H256, U256};
use sp_runtime::traits::{Dispatchable, Hash, StaticLookup};
use sp_std::{
	convert::{TryFrom, TryInto},
	marker::PhantomData,
	vec,
	vec::Vec,
};

mod types;
use types::{
	AccountVotes, BalanceOf, EvmAccountVotes, EvmVotingOf, GetEncodedProposalSizeLimit, HashOf,
	ReferendaVotes,
};

/// A precompile to wrap the functionality from governance related pallets.
pub struct GovernancePrecompile<Runtime>(PhantomData<Runtime>);

#[precompile_utils::precompile]
impl<Runtime> GovernancePrecompile<Runtime>
where
	Runtime: pallet_democracy::Config
		+ pallet_evm::Config
		+ frame_system::Config<Hash = H256>
		+ pallet_preimage::Config,
	Runtime::RuntimeCall: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	<Runtime::RuntimeCall as Dispatchable>::RuntimeOrigin: From<Option<Runtime::AccountId>>,
	Runtime::RuntimeCall: From<DemocracyCall<Runtime>>,
	Runtime::RuntimeCall: From<PreimageCall<Runtime>>,
	BalanceOf<Runtime>: TryFrom<U256> + Into<U256>,
	HashOf<Runtime>: Into<H256> + From<H256>,
	Runtime::Hash: From<H256> + Into<H256>,
	Runtime::AccountId: Into<H160>,
	BlockNumberFor<Runtime>: Into<U256>,
{
	// Storage getters

	#[precompile::public("publicPropCount()")]
	#[precompile::public("public_prop_count()")]
	#[precompile::view]
	fn public_prop_count(handle: &mut impl PrecompileHandle) -> EvmResult<u32> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let prop_count = pallet_democracy::PublicPropCount::<Runtime>::get();

		Ok(prop_count)
	}

	#[precompile::public("depositOf(uint256)")]
	#[precompile::public("deposit_of(uint256)")]
	#[precompile::view]
	fn deposit_of(
		handle: &mut impl PrecompileHandle,
		prop_index: u32,
	) -> EvmResult<(U256, U256, Vec<Address>)> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let zero = 0u32;
		let mut total_deposit: U256 = zero.into();
		let mut initial_deposit: U256 = zero.into();
		let mut depositors: Vec<Address> = vec![];

		if let Some(deposit_of) = pallet_democracy::DepositOf::<Runtime>::get(prop_index) {
			initial_deposit = deposit_of.1.into();
			for depositor in deposit_of.0 {
				depositors.push(Address(depositor.into()));
				total_deposit += initial_deposit;
			}
		}

		Ok((total_deposit, initial_deposit, depositors))
	}

	#[precompile::public("votingOf(uint256)")]
	#[precompile::public("voting_of(uint256)")]
	#[precompile::view]
	fn voting_of(handle: &mut impl PrecompileHandle, ref_index: u32) -> EvmResult<EvmVotingOf> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let mut referenda_votes = ReferendaVotes::<Runtime>::default(ref_index);

		let _ref_status = match pallet_democracy::ReferendumInfoOf::<Runtime>::get(ref_index) {
			Some(ReferendumInfo::Ongoing(ref_status)) => ref_status,
			Some(ReferendumInfo::Finished { .. }) => Err(revert("Referendum is finished"))?,
			None => Err(revert("Unknown referendum"))?,
		};

		for voting_of in pallet_democracy::VotingOf::<Runtime>::iter() {
			let voter: Runtime::AccountId = voting_of.0;
			let state = voting_of.1;

			match state {
				Voting::Direct { votes, .. } => {
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
							break;
						}
					}
				},
				Voting::Delegating { .. } => (),
			};
		}

		Ok((
			referenda_votes.ref_index.into(),
			referenda_votes.voters,
			referenda_votes
				.raw_votes
				.clone()
				.into_iter()
				.map(|v| v.into())
				.collect::<Vec<U256>>(),
			referenda_votes.voting_sides,
			referenda_votes.convictions,
		))
	}

	#[precompile::public("accountVotes(address)")]
	#[precompile::public("account_votes(address)")]
	#[precompile::view]
	fn account_votes(
		handle: &mut impl PrecompileHandle,
		account: Address,
	) -> EvmResult<EvmAccountVotes> {
		let account = Runtime::AddressMapping::into_account_id(account.0);

		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
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

		Ok(account_votes.into())
	}

	#[precompile::public("lowestUnbaked()")]
	#[precompile::public("lowest_unbaked()")]
	#[precompile::view]
	fn lowest_unbaked(handle: &mut impl PrecompileHandle) -> EvmResult<U256> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let lowest_unbaked = pallet_democracy::LowestUnbaked::<Runtime>::get();

		Ok(lowest_unbaked.into())
	}

	#[precompile::public("ongoingReferendumInfo(uint256)")]
	#[precompile::public("ongoing_referendum_info(uint256)")]
	#[precompile::view]
	fn ongoing_referendum_info(
		handle: &mut impl PrecompileHandle,
		ref_index: u32,
	) -> EvmResult<(U256, H256, u8, U256, U256, U256, U256)> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let ref_status = match pallet_democracy::ReferendumInfoOf::<Runtime>::get(ref_index) {
			Some(ReferendumInfo::Ongoing(ref_status)) => ref_status,
			Some(ReferendumInfo::Finished { .. }) => Err(revert("Referendum is finished"))?,
			None => Err(revert("Unknown referendum"))?,
		};

		let threshold_u8: u8 = match ref_status.threshold {
			VoteThreshold::SuperMajorityApprove => 0,
			VoteThreshold::SuperMajorityAgainst => 1,
			VoteThreshold::SimpleMajority => 2,
		};

		Ok((
			ref_status.end.into(),
			ref_status.proposal.hash().into(),
			threshold_u8.into(),
			ref_status.delay.into(),
			ref_status.tally.ayes.into(),
			ref_status.tally.nays.into(),
			ref_status.tally.turnout.into(),
		))
	}

	#[precompile::public("finishedReferendumInfo(uint256)")]
	#[precompile::public("finished_referendum_info(uint256)")]
	#[precompile::view]
	fn finished_referendum_info(
		handle: &mut impl PrecompileHandle,
		ref_index: u32,
	) -> EvmResult<(bool, U256)> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let (approved, end) = match pallet_democracy::ReferendumInfoOf::<Runtime>::get(ref_index) {
			Some(ReferendumInfo::Ongoing(_)) => Err(revert("Referendum is ongoing"))?,
			Some(ReferendumInfo::Finished { approved, end }) => (approved, end),
			None => Err(revert("Unknown referendum"))?,
		};

		Ok((approved, end.into()))
	}

	// Dispatchable methods

	#[precompile::public("propose(bytes32,uint256)")]
	fn propose(handle: &mut impl PrecompileHandle, proposal_hash: H256, value: U256) -> EvmResult {
		handle.record_log_costs_manual(2, 32)?;

		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let _prop_count = pallet_democracy::PublicPropCount::<Runtime>::get();

		let value = Self::u256_to_amount(value).in_field("value")?;

		// This forces it to have the proposal in pre-images.
		// TODO: REVISIT
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let len = <Runtime as pallet_democracy::Config>::Preimages::len(&proposal_hash).ok_or({
			RevertReason::custom("Failure in preimage fetch").in_field("proposal_hash")
		})?;

		let bounded = Bounded::Lookup::<
			pallet_democracy::CallOf<Runtime>,
			<Runtime as frame_system::Config>::Hashing,
		> {
			hash: proposal_hash,
			len,
		};

		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = DemocracyCall::<Runtime>::propose { proposal: bounded, value };

		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}

	#[precompile::public("second(uint256,uint256)")]
	fn second(
		handle: &mut impl PrecompileHandle,
		prop_index: u32,
		_seconds_upper_bound: u32,
	) -> EvmResult {
		handle.record_log_costs_manual(2, 32)?;

		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = DemocracyCall::<Runtime>::second { proposal: prop_index };

		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}

	#[precompile::public("vote(uint256,bool,uint256,uint256)")]
	fn vote(
		handle: &mut impl PrecompileHandle,
		ref_index: u32,
		aye: bool,
		vote_amount: U256,
		conviction: u8,
	) -> EvmResult {
		handle.record_log_costs_manual(2, 32 * 4)?;
		let vote_amount_balance = Self::u256_to_amount(vote_amount).in_field("voteAmount")?;

		let conviction_enum: Conviction = conviction.clone().try_into().map_err(|_| {
			RevertReason::custom("Must be an integer between 0 and 6 included")
				.in_field("conviction")
		})?;

		let vote = AccountVote::Standard {
			vote: Vote { aye, conviction: conviction_enum },
			balance: vote_amount_balance,
		};

		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = DemocracyCall::<Runtime>::vote { ref_index, vote };

		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}

	#[precompile::public("removeVote(uint256)")]
	#[precompile::public("remove_vote(uint256)")]
	fn remove_vote(handle: &mut impl PrecompileHandle, ref_index: u32) -> EvmResult {
		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = DemocracyCall::<Runtime>::remove_vote { index: ref_index };

		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}

	#[precompile::public("delegate(address,uint256,uint256)")]
	fn delegate(
		handle: &mut impl PrecompileHandle,
		representative: Address,
		conviction: u8,
		amount: U256,
	) -> EvmResult {
		handle.record_log_costs_manual(2, 32)?;
		let amount = Self::u256_to_amount(amount).in_field("amount")?;

		let conviction: Conviction = conviction.try_into().map_err(|_| {
			RevertReason::custom("Must be an integer between 0 and 6 included")
				.in_field("conviction")
		})?;

		let to = Runtime::AddressMapping::into_account_id(representative.into());
		let to: <Runtime::Lookup as StaticLookup>::Source = Runtime::Lookup::unlookup(to.clone());
		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = DemocracyCall::<Runtime>::delegate { to, conviction, balance: amount };

		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}

	#[precompile::public("undelegate()")]
	fn un_delegate(handle: &mut impl PrecompileHandle) -> EvmResult {
		handle.record_log_costs_manual(2, 0)?;
		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = DemocracyCall::<Runtime>::undelegate {};

		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}

	#[precompile::public("unlock(address)")]
	fn unlock(handle: &mut impl PrecompileHandle, target: Address) -> EvmResult {
		let target: H160 = target.into();
		let target = Runtime::AddressMapping::into_account_id(target);
		let target: <Runtime::Lookup as StaticLookup>::Source =
			Runtime::Lookup::unlookup(target.clone());

		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = DemocracyCall::<Runtime>::unlock { target };

		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}

	#[precompile::public("notePreimage(bytes)")]
	#[precompile::public("note_preimage(bytes)")]
	fn note_preimage(
		handle: &mut impl PrecompileHandle,
		encoded_proposal: BoundedBytes<GetEncodedProposalSizeLimit>,
	) -> EvmResult {
		let encoded_proposal: Vec<u8> = encoded_proposal.into();

		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = PreimageCall::<Runtime>::note_preimage { bytes: encoded_proposal.into() };
		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}

	#[precompile::public("noteImminentPreimage(bytes)")]
	#[precompile::public("note_imminent_preimage(bytes)")]
	fn note_imminent_preimage(
		handle: &mut impl PrecompileHandle,
		encoded_proposal: BoundedBytes<GetEncodedProposalSizeLimit>,
	) -> EvmResult {
		let encoded_proposal: Vec<u8> = encoded_proposal.into();

		// To mimic imminent preimage behavior, we need to check whether the preimage
		// has been requested
		// is_requested implies db read
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let proposal_hash = <Runtime as frame_system::Config>::Hashing::hash(&encoded_proposal);
		if !<<Runtime as pallet_democracy::Config>::Preimages as QueryPreimage>::is_requested(
			&proposal_hash.into(),
		) {
			return Err(revert("not imminent preimage (preimage not requested)"));
		};

		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = PreimageCall::<Runtime>::note_preimage { bytes: encoded_proposal.into() };
		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}

	fn u256_to_amount(value: U256) -> MayRevert<BalanceOf<Runtime>> {
		value
			.try_into()
			.map_err(|_| RevertReason::value_is_too_large("balance type").into())
	}
}
