#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]

use frame_support::dispatch::{GetDispatchInfo, PostDispatchInfo};
use pallet_evm::AddressMapping;
use pallet_pools::Call as PoolsCall;
use precompile_utils::prelude::*;
use sp_core::{H160, U256};
use sp_runtime::traits::Dispatchable;
use sp_std::marker::PhantomData;

/// A precompile that dispatches borrow and repay requests to pallet-pools.
///
/// Called by the CCCP receiver contract when a borrow or repay message
/// arrives on Bifrost from an external EVM (Spoke) chain.
pub struct PoolsPrecompile<Runtime>(PhantomData<Runtime>);

#[precompile_utils::precompile]
impl<Runtime> PoolsPrecompile<Runtime>
where
	Runtime: pallet_pools::Config + pallet_evm::Config + frame_system::Config,
	Runtime::RuntimeCall: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	<Runtime::RuntimeCall as Dispatchable>::RuntimeOrigin: From<Option<Runtime::AccountId>>,
	Runtime::RuntimeCall: From<PoolsCall<Runtime>>,
	<Runtime as pallet_evm::Config>::AddressMapping: AddressMapping<Runtime::AccountId>,
{
	/// Draw funds from a tranche treasury.
	///
	/// Called by the CCCP receiver contract when a `requestBorrow` message
	/// arrives from the Spoke chain. Increments the tranche's `borrowed` counter
	/// and emits a `Borrowed` event. Fails if treasury liquidity is insufficient.
	///
	/// @param pool_id       the pool ID
	/// @param chain_id      EVM chain ID of the chain where the vault is deployed
	/// @param vault_address ERC-7540 vault contract address on that chain
	/// @param amount        USDC amount to borrow
	#[precompile::public("borrow(uint64,uint64,address,uint256)")]
	fn borrow(
		handle: &mut impl PrecompileHandle,
		pool_id: u64,
		chain_id: u64,
		vault_address: Address,
		amount: U256,
	) -> EvmResult {
		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let vault_address: H160 = vault_address.0;

		let call = PoolsCall::<Runtime>::borrow { pool_id, chain_id, vault_address, amount };

		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call, 0)?;
		Ok(())
	}

	/// Return funds to a tranche treasury.
	///
	/// Called by the CCCP receiver contract when a `repay` message arrives
	/// from the Spoke chain. Decrements the tranche's `borrowed` counter and
	/// emits a `Repaid` event.
	///
	/// @param pool_id       the pool ID
	/// @param chain_id      EVM chain ID of the chain where the vault is deployed
	/// @param vault_address ERC-7540 vault contract address on that chain
	/// @param amount        USDC amount being repaid
	#[precompile::public("repay(uint64,uint64,address,uint256)")]
	fn repay(
		handle: &mut impl PrecompileHandle,
		pool_id: u64,
		chain_id: u64,
		vault_address: Address,
		amount: U256,
	) -> EvmResult {
		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let vault_address: H160 = vault_address.0;

		let call = PoolsCall::<Runtime>::repay { pool_id, chain_id, vault_address, amount };

		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call, 0)?;
		Ok(())
	}
}
