//! Read-only precompile over `pallet-permissioned-authority`, deployed at the address of the
//! `bfc-staking` precompile (0x0400) on chains that replace staking with it. It keeps the exact
//! `round_info()` / `latest_round()` ABI the CCCP relayer reads from its `authority_address`.

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]

use frame_system::pallet_prelude::BlockNumberFor;
use pallet_evm::AddressMapping;
use precompile_utils::prelude::*;
use sp_core::{H160, U256};
use sp_std::{marker::PhantomData, vec::Vec};

/// Same tuple layout as the `bfc-staking` precompile's `round_info()`: current round index,
/// first session index, current session index, first round block, first session block,
/// current block, round length, session length.
pub type EvmRoundInfoOf = (u32, u32, u32, U256, U256, U256, u32, u32);

/// A precompile to expose the round/authority state of pallet_permissioned_authority.
pub struct PermissionedAuthorityPrecompile<Runtime>(PhantomData<Runtime>);

#[precompile_utils::precompile]
impl<Runtime> PermissionedAuthorityPrecompile<Runtime>
where
	Runtime: pallet_permissioned_authority::Config + pallet_evm::Config,
	Runtime::AccountId: Into<H160>,
	BlockNumberFor<Runtime>: Into<U256>,
	<Runtime as pallet_evm::Config>::AddressMapping: AddressMapping<Runtime::AccountId>,
{
	/// Returns the information of the current round
	/// @return: The current rounds index, first session index, current session index,
	///          first round block, first session block, current block, round length, session length
	#[precompile::public("roundInfo()")]
	#[precompile::public("round_info()")]
	#[precompile::view]
	fn round_info(handle: &mut impl PrecompileHandle) -> EvmResult<EvmRoundInfoOf> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let round_info = pallet_permissioned_authority::Round::<Runtime>::get();

		Ok((
			round_info.current_round_index,
			round_info.first_session_index,
			round_info.current_session_index,
			round_info.first_round_block.into(),
			round_info.first_session_block.into(),
			round_info.current_block.into(),
			round_info.round_length,
			round_info.session_length,
		))
	}

	/// Returns the latest round index
	/// @return: The latest round index
	#[precompile::public("latestRound()")]
	#[precompile::public("latest_round()")]
	#[precompile::view]
	fn latest_round(handle: &mut impl PrecompileHandle) -> EvmResult<u32> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		Ok(pallet_permissioned_authority::Round::<Runtime>::get().current_round_index)
	}

	/// Returns the validators of the current round
	/// @return: The active authority (validator) addresses
	#[precompile::public("activeAuthorities()")]
	#[precompile::public("active_authorities()")]
	#[precompile::view]
	fn active_authorities(handle: &mut impl PrecompileHandle) -> EvmResult<Vec<Address>> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		Ok(pallet_permissioned_authority::ActiveAuthorities::<Runtime>::get()
			.into_iter()
			.map(|a| Address(a.into()))
			.collect())
	}

	/// Returns the requested validators, applied at the next session rotation
	/// @return: The requested authority (validator) addresses
	#[precompile::public("authorities()")]
	#[precompile::view]
	fn authorities(handle: &mut impl PrecompileHandle) -> EvmResult<Vec<Address>> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		Ok(pallet_permissioned_authority::Authorities::<Runtime>::get()
			.into_iter()
			.map(|a| Address(a.into()))
			.collect())
	}

	/// Verifies if the given address is a validator of the current round
	/// @param: `who` the address for which to verify
	#[precompile::public("isActiveAuthority(address)")]
	#[precompile::public("is_active_authority(address)")]
	#[precompile::view]
	fn is_active_authority(handle: &mut impl PrecompileHandle, who: Address) -> EvmResult<bool> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let who = Runtime::AddressMapping::into_account_id(who.0);
		Ok(pallet_permissioned_authority::ActiveAuthorities::<Runtime>::get().contains(&who))
	}
}
