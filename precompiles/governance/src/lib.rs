#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(test, feature(assert_matches))]

use frame_support::{
	dispatch::{Dispatchable, GetDispatchInfo, PostDispatchInfo},
	traits::Currency,
};

use pallet_democracy::{
	AccountVote, Call as DemocracyCall, Conviction, PropIndex, ReferendumInfo, Vote, VoteThreshold,
	Voting,
};
use pallet_evm::{AddressMapping, Precompile};

use precompile_utils::prelude::*;

use fp_evm::{Context, ExitError, ExitSucceed, PrecompileFailure, PrecompileOutput};
use sp_core::{H160, H256, U256};
use sp_std::{
	convert::{TryFrom, TryInto},
	fmt::Debug,
	marker::PhantomData,
	vec,
	vec::Vec,
};

mod types;
use types::{BalanceOf, BlockNumberOf, DemocracyOf, EvmPublicProposalsOf, HashOf};

/// A precompile to wrap the functionality from governance related pallets.
pub struct GovernancePrecompile<Runtime>(PhantomData<Runtime>);

#[precompile_utils::precompile]
impl<Runtime> GovernancePrecompile<Runtime>
where
	Runtime: pallet_democracy::Config + pallet_evm::Config + frame_system::Config,
	Runtime::RuntimeCall: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	<Runtime::RuntimeCall as Dispatchable>::RuntimeOrigin: From<Option<Runtime::AccountId>>,
	BalanceOf<Runtime>: TryFrom<U256> + TryInto<u128> + EvmData,
	BlockNumberOf<Runtime>: EvmData,
	HashOf<Runtime>: Into<H256> + From<H256> + EvmData,
	Runtime::RuntimeCall: From<DemocracyCall<Runtime>>,
	Runtime::Hash: From<H256> + Into<H256>,
	Runtime::AccountId: Into<H160>,
{
	// Storage getters

	#[precompile::public("publicPropCount()")]
	#[precompile::public("public_prop_count()")]
	#[precompile::view]
	fn public_prop_count(handle: &mut impl PrecompileHandle) -> EvmResult<u32> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let prop_count = DemocracyOf::<Runtime>::public_prop_count();

		Ok(prop_count)
	}
}
