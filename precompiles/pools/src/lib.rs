#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]

use frame_support::dispatch::{GetDispatchInfo, PostDispatchInfo};
use pallet_pools::{Call as PoolsCall, PoolInspect};
use precompile_utils::prelude::*;
use sp_core::{H160, U256};
use sp_runtime::traits::Dispatchable;
use sp_std::marker::PhantomData;

pub(crate) const SELECTOR_LOG_BORROWED: [u8; 32] =
	keccak256!("Borrowed(uint64,uint64,address,uint256)");
pub(crate) const SELECTOR_LOG_REPAID: [u8; 32] =
	keccak256!("Repaid(uint64,uint64,address,uint256)");

/// A precompile that dispatches borrow and repay requests to pallet-pools.
///
/// Only callable by the Gateway contract whose address is stored in
/// `pallet_pools::GatewayAddress` storage. Calls are dispatched with the
/// `pallet_pools::Origin::Gateway` origin so the pallet rejects any direct
/// extrinsic submissions.
pub struct PoolsPrecompile<Runtime>(PhantomData<Runtime>);

#[precompile_utils::precompile]
impl<Runtime> PoolsPrecompile<Runtime>
where
	Runtime: pallet_pools::Config + pallet_evm::Config + frame_system::Config,
	Runtime::RuntimeCall: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	<Runtime::RuntimeCall as Dispatchable>::RuntimeOrigin: From<pallet_pools::Origin>,
	Runtime::RuntimeCall: From<PoolsCall<Runtime>>,
	pallet_pools::Pallet<Runtime>: PoolInspect<Runtime::AccountId>,
{
	/// Draw funds from a tranche treasury.
	///
	/// Only the Gateway contract may call this function.
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
		if handle.context().caller != pallet_pools::Pallet::<Runtime>::gateway_address() {
			return Err(revert("caller is not the gateway"));
		}

		let vault_address: H160 = vault_address.0;
		let call = PoolsCall::<Runtime>::borrow { pool_id, chain_id, vault_address, amount };

		RuntimeHelper::<Runtime>::try_dispatch(
			handle,
			pallet_pools::Origin::Gateway.into(),
			call,
			0,
		)?;

		let event = log1(
			handle.context().address,
			SELECTOR_LOG_BORROWED,
			solidity::encode_event_data((
				U256::from(pool_id),
				U256::from(chain_id),
				Address(vault_address),
				amount,
			)),
		);
		handle.record_log_costs(&[&event])?;
		event.record(handle)?;

		Ok(())
	}

	/// Return funds to a tranche treasury.
	///
	/// Only the Gateway contract may call this function.
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
		if handle.context().caller != pallet_pools::Pallet::<Runtime>::gateway_address() {
			return Err(revert("caller is not the gateway"));
		}

		let vault_address: H160 = vault_address.0;
		let call = PoolsCall::<Runtime>::repay { pool_id, chain_id, vault_address, amount };

		RuntimeHelper::<Runtime>::try_dispatch(
			handle,
			pallet_pools::Origin::Gateway.into(),
			call,
			0,
		)?;

		let event = log1(
			handle.context().address,
			SELECTOR_LOG_REPAID,
			solidity::encode_event_data((
				U256::from(pool_id),
				U256::from(chain_id),
				Address(vault_address),
				amount,
			)),
		);
		handle.record_log_costs(&[&event])?;
		event.record(handle)?;

		Ok(())
	}
}
