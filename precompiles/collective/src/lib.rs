#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]

use frame_support::dispatch::{GetDispatchInfo, PostDispatchInfo};

use pallet_collective::Call as CollectiveCall;
use pallet_evm::AddressMapping;

use precompile_utils::prelude::*;

use sp_core::{Decode, H160, H256};
use sp_runtime::traits::Dispatchable;
use sp_std::{marker::PhantomData, vec::Vec};

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
	<Runtime as pallet_evm::Config>::AddressMapping: AddressMapping<Runtime::AccountId>,
{
	#[precompile::public("isMember(address)")]
	#[precompile::public("is_member(address)")]
	#[precompile::view]
	fn is_member(handle: &mut impl PrecompileHandle, who: Address) -> EvmResult<bool> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let who = Runtime::AddressMapping::into_account_id(who.0);
		Ok(CollectiveOf::<Runtime, Instance>::is_member(&who))
	}

	#[precompile::public("members()")]
	#[precompile::view]
	fn members(handle: &mut impl PrecompileHandle) -> EvmResult<Vec<Address>> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let members = pallet_collective::Members::<Runtime, Instance>::get()
			.into_iter()
			.map(|member| Address(member.into()))
			.collect::<Vec<Address>>();

		Ok(members)
	}
}
