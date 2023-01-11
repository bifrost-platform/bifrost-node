#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(test, feature(assert_matches))]

use frame_support::dispatch::{Dispatchable, GetDispatchInfo, PostDispatchInfo};

use pallet_evm::AddressMapping;
use pallet_relay_manager::{Call as RelayManagerCall, RelayerMetadata, RelayerStatus};

use precompile_utils::prelude::*;

use sp_core::{H160, H256};
use sp_std::{marker::PhantomData, vec, vec::Vec};

mod types;
use types::RelayManagerOf;

/// A precompile to wrap the functionality from pallet_relay_manager
pub struct RelayManagerPrecompile<Runtime>(PhantomData<Runtime>);

#[precompile_utils::precompile]
impl<Runtime> RelayManagerPrecompile<Runtime>
where
	Runtime: pallet_relay_manager::Config + pallet_evm::Config + frame_system::Config,
	Runtime::Hash: From<H256> + Into<H256>,
	Runtime::AccountId: Into<H160>,
	Runtime::RuntimeCall: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	<Runtime::RuntimeCall as Dispatchable>::RuntimeOrigin: From<Option<Runtime::AccountId>>,
	Runtime::RuntimeCall: From<RelayManagerCall<Runtime>>,
{
	// Role verifiers

	#[precompile::public("isRelayer(address)")]
	#[precompile::public("is_relayer(address)")]
	#[precompile::view]
	fn is_relayer(handle: &mut impl PrecompileHandle, relayer: Address) -> EvmResult<bool> {
		let relayer = Runtime::AddressMapping::into_account_id(relayer.0);

		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let is_relayer = RelayManagerOf::<Runtime>::is_relayer(&relayer);

		Ok(is_relayer)
	}

	#[precompile::public("isSelectedRelayer(address,bool)")]
	#[precompile::public("is_selected_relayer(address,bool)")]
	#[precompile::view]
	fn is_selected_relayer(
		handle: &mut impl PrecompileHandle,
		relayer: Address,
		is_initial: bool,
	) -> EvmResult<bool> {
		let relayer = Runtime::AddressMapping::into_account_id(relayer.0);

		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let is_selected_relayer =
			RelayManagerOf::<Runtime>::is_selected_relayer(&relayer, is_initial);

		Ok(is_selected_relayer)
	}

	#[precompile::public("isRelayers(address[])")]
	#[precompile::public("is_relayers(address[])")]
	#[precompile::view]
	fn is_relayers(handle: &mut impl PrecompileHandle, relayers: Vec<Address>) -> EvmResult<bool> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let mut unique_relayers = relayers
			.clone()
			.into_iter()
			.map(|address| Runtime::AddressMapping::into_account_id(address.0))
			.collect::<Vec<Runtime::AccountId>>();

		let previous_len = unique_relayers.len();
		unique_relayers.sort();
		unique_relayers.dedup();
		let current_len = unique_relayers.len();
		if current_len < previous_len {
			return Err(RevertReason::custom("Duplicate candidate address received").into())
		}

		let mut is_relayers = true;
		for relayer in unique_relayers {
			if !RelayManagerOf::<Runtime>::is_relayer(&relayer) {
				is_relayers = false;
				break
			}
		}

		Ok(is_relayers)
	}

	#[precompile::public("isSelectedRelayers(address[],bool)")]
	#[precompile::public("is_selected_relayers(address[],bool)")]
	#[precompile::view]
	fn is_selected_relayers(
		handle: &mut impl PrecompileHandle,
		relayers: Vec<Address>,
		is_initial: bool,
	) -> EvmResult<bool> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let mut unique_relayers = relayers
			.clone()
			.into_iter()
			.map(|address| Runtime::AddressMapping::into_account_id(address.0))
			.collect::<Vec<Runtime::AccountId>>();

		let previous_len = unique_relayers.len();
		unique_relayers.sort();
		unique_relayers.dedup();
		let current_len = unique_relayers.len();
		if current_len < previous_len {
			return Err(RevertReason::custom("Duplicate candidate address received").into())
		}

		let mut is_relayers = true;
		for relayer in unique_relayers {
			if !RelayManagerOf::<Runtime>::is_selected_relayer(&relayer, is_initial) {
				is_relayers = false;
				break
			}
		}

		Ok(is_relayers)
	}

	#[precompile::public("isCompleteSelectedRelayers(address[]")]
	fn is_complete_selected_relayers(
		handle: &mut impl PrecompileHandle,
		relayers: Vec<Address>,
		is_initial: bool,
	) -> EvmResult<bool> {
	}
}
