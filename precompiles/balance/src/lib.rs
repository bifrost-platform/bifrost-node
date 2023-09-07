#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::dispatch::{Dispatchable, GetDispatchInfo, PostDispatchInfo};

use fp_evm::PrecompileHandle;
use precompile_utils::{substrate::RuntimeHelper, EvmResult};

use sp_core::{H160, U256};
use sp_std::marker::PhantomData;

/// A precompile to wrap the functionality from pallet_balances
pub struct BalancePrecompile<Runtime>(PhantomData<Runtime>);

#[precompile_utils::precompile]
impl<Runtime> BalancePrecompile<Runtime>
where
	Runtime: pallet_balances::Config + pallet_evm::Config + frame_system::Config,
	Runtime::RuntimeCall: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	<Runtime::RuntimeCall as Dispatchable>::RuntimeOrigin: From<Option<Runtime::AccountId>>,
	<Runtime as pallet_balances::Config>::Balance: Into<U256>,
	Runtime::AccountId: Into<H160>,
{
	// Storage getters

	#[precompile::public("totalIssuance()")]
	#[precompile::public("total_issuance()")]
	#[precompile::view]
	fn total_issuance(handle: &mut impl PrecompileHandle) -> EvmResult<U256> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let total_issuance = <pallet_balances::Pallet<Runtime>>::total_issuance();

		Ok(total_issuance.into())
	}
}
