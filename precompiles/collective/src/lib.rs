#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(test, feature(assert_matches))]

use frame_support::dispatch::{Dispatchable, GetDispatchInfo, PostDispatchInfo};

use fp_evm::{Context, ExitError, ExitSucceed, PrecompileFailure, PrecompileOutput};
use pallet_collective::Call as CollectiveCall;
use pallet_evm::{AddressMapping, Precompile};
use precompile_utils::{
	Address, Bytes, EvmDataReader, EvmDataWriter, EvmResult, FunctionModifier, Gasometer,
	RuntimeHelper,
};

use sp_core::{Decode, H160, H256};
use sp_std::{boxed::Box, convert::TryInto, fmt::Debug, marker::PhantomData, vec, vec::Vec};

type CollectiveOf<Runtime, Instance> = pallet_collective::Pallet<Runtime, Instance>;

#[precompile_utils::generate_function_selector]
#[derive(Debug, PartialEq)]
enum Action {
	// Storage getters
	Members = "members()",
	// Dispatchable methods
	Propose = "propose(uint256,bytes)",
	Vote = "vote(bytes32,uint256,bool)",
	Close = "close(bytes32,uint256,uint256,uint256)",
	Execute = "execute(bytes)",
}

/// A precompile to wrap the functionality from collective related pallets.
pub struct CollectivePrecompile<Runtime, Instance: 'static>(PhantomData<(Runtime, Instance)>);

impl<Runtime, Instance> Precompile for CollectivePrecompile<Runtime, Instance>
where
	Instance: 'static,
	Runtime: pallet_collective::Config<Instance> + pallet_evm::Config + frame_system::Config,
	Runtime::Call: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo + Decode,
	<Runtime::Call as Dispatchable>::Origin: From<Option<Runtime::AccountId>>,
	Runtime::Call: From<CollectiveCall<Runtime, Instance>>,
	<Runtime as pallet_collective::Config<Instance>>::Proposal:
		From<CollectiveCall<Runtime, Instance>>,
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
				Action::Propose | Action::Vote | Action::Close | Action::Execute =>
					FunctionModifier::NonPayable,
				_ => FunctionModifier::View,
			},
		)?;

		let (origin, call) = match selector {
			// Storage getters
			Action::Members => return Self::members(gasometer),
			// Dispatchable methods
			Action::Propose => Self::propose(input, gasometer, context)?,
			Action::Vote => Self::vote(input, gasometer, context)?,
			Action::Close => Self::close(input, gasometer, context)?,
			Action::Execute => Self::execute(input, gasometer, context)?,
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

impl<Runtime, Instance> CollectivePrecompile<Runtime, Instance>
where
	Instance: 'static,
	Runtime: pallet_collective::Config<Instance> + pallet_evm::Config + frame_system::Config,
	Runtime::Call: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo + Decode,
	<Runtime::Call as Dispatchable>::Origin: From<Option<Runtime::AccountId>>,
	Runtime::Call: From<CollectiveCall<Runtime, Instance>>,
	<Runtime as pallet_collective::Config<Instance>>::Proposal:
		From<CollectiveCall<Runtime, Instance>>,
	Runtime::Hash: From<H256> + Into<H256>,
	Runtime::AccountId: Into<H160>,
{
	fn members(gasometer: &mut Gasometer) -> EvmResult<PrecompileOutput> {
		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let members = CollectiveOf::<Runtime, Instance>::members()
			.into_iter()
			.map(|member| Address(member.into()))
			.collect::<Vec<Address>>();

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(members).build(),
			logs: vec![],
		})
	}

	fn propose(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, CollectiveCall<Runtime, Instance>)> {
		input.expect_arguments(gasometer, 2)?;

		let threshold = input.read(gasometer)?;
		let proposal: Vec<u8> = input.read::<Bytes>(gasometer)?.into();
		let proposal_length: u32 = match proposal.len().try_into() {
			Ok(proposal_length) => proposal_length,
			Err(_) =>
				return Err(PrecompileFailure::Error {
					exit_status: ExitError::Other("Proposal is too large".into()),
				}),
		};
		if let Ok(proposal) = CollectiveCall::<Runtime, Instance>::decode(&mut &*proposal) {
			let origin = Runtime::AddressMapping::into_account_id(context.caller);
			let call = CollectiveCall::<Runtime, Instance>::propose {
				threshold,
				proposal: Box::new(proposal.into()),
				length_bound: proposal_length,
			};

			Ok((Some(origin).into(), call))
		} else {
			return Err(PrecompileFailure::Error {
				exit_status: ExitError::Other("Failed to decode proposal".into()),
			})
		}
	}

	fn execute(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, CollectiveCall<Runtime, Instance>)> {
		input.expect_arguments(gasometer, 1)?;

		let proposal: Vec<u8> = input.read::<Bytes>(gasometer)?.into();
		let proposal_length: u32 = match proposal.len().try_into() {
			Ok(proposal_length) => proposal_length,
			Err(_) =>
				return Err(PrecompileFailure::Error {
					exit_status: ExitError::Other("Proposal is too large".into()),
				}),
		};
		if let Ok(proposal) = CollectiveCall::<Runtime, Instance>::decode(&mut &*proposal) {
			let origin = Runtime::AddressMapping::into_account_id(context.caller);
			let call = CollectiveCall::<Runtime, Instance>::execute {
				proposal: Box::new(proposal.into()),
				length_bound: proposal_length,
			};

			Ok((Some(origin).into(), call))
		} else {
			return Err(PrecompileFailure::Error {
				exit_status: ExitError::Other("Failed to decode proposal".into()),
			})
		}
	}

	fn vote(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, CollectiveCall<Runtime, Instance>)> {
		input.expect_arguments(gasometer, 3)?;

		let proposal_hash = input.read::<H256>(gasometer)?.into();
		let proposal_index: u32 = input.read(gasometer)?;
		let approve: bool = input.read(gasometer)?;

		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call = CollectiveCall::<Runtime, Instance>::vote {
			proposal: proposal_hash,
			index: proposal_index,
			approve,
		};

		Ok((Some(origin).into(), call))
	}

	fn close(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, CollectiveCall<Runtime, Instance>)> {
		input.expect_arguments(gasometer, 4)?;

		let proposal_hash = input.read::<H256>(gasometer)?.into();
		let proposal_index: u32 = input.read(gasometer)?;
		let proposal_weight_bound: u64 = input.read(gasometer)?;
		let length_bound: u32 = input.read(gasometer)?;

		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call = CollectiveCall::<Runtime, Instance>::close {
			proposal_hash,
			index: proposal_index,
			proposal_weight_bound,
			length_bound,
		};

		Ok((Some(origin).into(), call))
	}
}
