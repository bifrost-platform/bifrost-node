#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{
	dispatch::{GetDispatchInfo, PostDispatchInfo},
	weights::Weight,
};

use pallet_collective::Call as CollectiveCall;
use pallet_evm::AddressMapping;

use precompile_utils::prelude::*;

use sp_core::{Decode, H160, H256};
use sp_runtime::traits::Dispatchable;
use sp_std::{boxed::Box, convert::TryInto, marker::PhantomData, vec::Vec};

type CollectiveOf<Runtime, Instance> = pallet_collective::Pallet<Runtime, Instance>;

/// A precompile to wrap the functionality from collective related pallets.
pub struct CollectivePrecompile<Runtime, Instance: 'static>(PhantomData<(Runtime, Instance)>);

#[precompile_utils::precompile]
impl<Runtime, Instance> CollectivePrecompile<Runtime, Instance>
where
	Instance: 'static,
	Runtime: pallet_collective::Config<Instance> + pallet_evm::Config + frame_system::Config,
	Runtime::RuntimeCall: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo + Decode,
	<Runtime::RuntimeCall as Dispatchable>::RuntimeOrigin: From<Option<Runtime::AccountId>>,
	Runtime::RuntimeCall: From<CollectiveCall<Runtime, Instance>>,
	<Runtime as pallet_collective::Config<Instance>>::Proposal:
		From<CollectiveCall<Runtime, Instance>>,
	Runtime::Hash: From<H256> + Into<H256>,
	Runtime::AccountId: Into<H160>,
{
	#[precompile::public("members()")]
	#[precompile::view]
	fn members(handle: &mut impl PrecompileHandle) -> EvmResult<Vec<Address>> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let members = CollectiveOf::<Runtime, Instance>::members()
			.into_iter()
			.map(|member| Address(member.into()))
			.collect::<Vec<Address>>();

		Ok(members)
	}

	#[precompile::public("propose(uint256,bytes)")]
	#[precompile::view]
	fn propose(handle: &mut impl PrecompileHandle, threshold: u32, proposal: Vec<u8>) -> EvmResult {
		let proposal_length: u32 = match proposal.len().try_into() {
			Ok(proposal_length) => proposal_length,
			Err(_) => return Err(RevertReason::value_is_too_large("proposal").into()),
		};
		if let Ok(proposal) = CollectiveCall::<Runtime, Instance>::decode(&mut &*proposal) {
			let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
			let call = CollectiveCall::<Runtime, Instance>::propose {
				threshold,
				proposal: Box::new(proposal.into()),
				length_bound: proposal_length,
			};

			RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

			Ok(())
		} else {
			return Err(RevertReason::custom("Failed to decode proposal").into());
		}
	}

	#[precompile::public("execute(bytes)")]
	#[precompile::view]
	fn execute(handle: &mut impl PrecompileHandle, proposal: Vec<u8>) -> EvmResult {
		let proposal_length: u32 = match proposal.len().try_into() {
			Ok(proposal_length) => proposal_length,
			Err(_) => return Err(RevertReason::value_is_too_large("proposal").into()),
		};
		if let Ok(proposal) = CollectiveCall::<Runtime, Instance>::decode(&mut &*proposal) {
			let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
			let call = CollectiveCall::<Runtime, Instance>::execute {
				proposal: Box::new(proposal.into()),
				length_bound: proposal_length,
			};

			RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

			Ok(())
		} else {
			return Err(RevertReason::custom("Failed to decode proposal").into());
		}
	}

	#[precompile::public("vote(bytes32,uint256,bool)")]
	#[precompile::view]
	fn vote(
		handle: &mut impl PrecompileHandle,
		proposal_hash: H256,
		proposal_index: u32,
		approve: bool,
	) -> EvmResult {
		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = CollectiveCall::<Runtime, Instance>::vote {
			proposal: proposal_hash.into(),
			index: proposal_index,
			approve,
		};

		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}

	#[precompile::public("close(bytes32,uint256,uint256,uint256)")]
	#[precompile::view]
	fn close(
		handle: &mut impl PrecompileHandle,
		proposal_hash: H256,
		proposal_index: u32,
		proposal_weight_bound: u64,
		length_bound: u32,
	) -> EvmResult {
		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = CollectiveCall::<Runtime, Instance>::close {
			proposal_hash: proposal_hash.into(),
			index: proposal_index,
			proposal_weight_bound: Weight::from_parts(proposal_weight_bound, 0),
			length_bound,
		};

		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}
}
