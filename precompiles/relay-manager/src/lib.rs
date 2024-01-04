#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::dispatch::{Dispatchable, GetDispatchInfo, PostDispatchInfo};
use frame_support::pallet_prelude::ConstU32;
use frame_support::BoundedBTreeSet;

use pallet_evm::AddressMapping;
use pallet_relay_manager::Call as RelayManagerCall;

use precompile_utils::prelude::*;

use bp_staking::{RoundIndex, MAX_AUTHORITIES};
use sp_core::{H160, H256, U256};
use sp_std::{collections::btree_set::BTreeSet, marker::PhantomData, vec, vec::Vec};

mod types;
use types::{EvmRelayerStateOf, EvmRelayerStatesOf, RelayManagerOf, RelayerState, RelayerStates};

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

	fn set_and_validate<F>(relayers: &Vec<Address>, exact: bool, validate: F) -> EvmResult<bool>
	where
		F: FnMut(&Runtime::AccountId) -> bool,
	{
		let unique_relayers: BTreeSet<Runtime::AccountId> = relayers
			.iter()
			.map(|address| Runtime::AddressMapping::into_account_id(address.0))
			.collect();
		if unique_relayers.len() != relayers.len() {
			return Err(RevertReason::custom("Duplicate relayer address received").into());
		}

		if exact {
			let selected_relayers = RelayManagerOf::<Runtime>::selected_relayers();
			if selected_relayers.len() != unique_relayers.len() {
				return Ok(false);
			}
		}

		Ok(unique_relayers.iter().all(validate))
	}

	#[precompile::public("isRelayers(address[])")]
	#[precompile::public("is_relayers(address[])")]
	#[precompile::view]
	fn is_relayers(handle: &mut impl PrecompileHandle, relayers: Vec<Address>) -> EvmResult<bool> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		Self::set_and_validate(&relayers, false, |relayer| {
			RelayManagerOf::<Runtime>::is_relayer(relayer)
		})
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

		Self::set_and_validate(&relayers, false, |relayer| {
			RelayManagerOf::<Runtime>::is_selected_relayer(relayer, is_initial)
		})
	}

	#[precompile::public("isCompleteSelectedRelayers(address[],bool)")]
	#[precompile::public("is_complete_selected_relayers(address[],bool)")]
	#[precompile::view]
	fn is_complete_selected_relayers(
		handle: &mut impl PrecompileHandle,
		relayers: Vec<Address>,
		is_initial: bool,
	) -> EvmResult<bool> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		Self::set_and_validate(&relayers, true, |relayer| {
			RelayManagerOf::<Runtime>::is_selected_relayer(relayer, is_initial)
		})
	}

	fn get_previous_selected_relayers(
		round_index: &RoundIndex,
		is_initial: bool,
	) -> EvmResult<BoundedBTreeSet<Runtime::AccountId, ConstU32<MAX_AUTHORITIES>>> {
		let previous_selected_relayers = if is_initial {
			RelayManagerOf::<Runtime>::cached_initial_selected_relayers()
		} else {
			RelayManagerOf::<Runtime>::cached_selected_relayers()
		};

		if let Some(previous_selected_relayers) = previous_selected_relayers.get(round_index) {
			return Ok(previous_selected_relayers.clone());
		} else {
			Err(RevertReason::read_out_of_bounds("round_index").into())
		}
	}

	#[precompile::public("isPreviousSelectedRelayer(uint256,address,bool)")]
	#[precompile::public("is_previous_selected_relayer(uint256,address,bool)")]
	#[precompile::view]
	fn is_previous_selected_relayer(
		handle: &mut impl PrecompileHandle,
		round_index: RoundIndex,
		relayer: Address,
		is_initial: bool,
	) -> EvmResult<bool> {
		let relayer = Runtime::AddressMapping::into_account_id(relayer.0);

		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		Ok(Self::get_previous_selected_relayers(&round_index, is_initial)?.contains(&relayer))
	}

	#[precompile::public("isPreviousSelectedRelayers(uint256,address[],bool)")]
	#[precompile::public("is_previous_selected_relayers(uint256,address[],bool)")]
	#[precompile::view]
	fn is_previous_selected_relayers(
		handle: &mut impl PrecompileHandle,
		round_index: RoundIndex,
		relayers: Vec<Address>,
		is_initial: bool,
	) -> EvmResult<bool> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let unique_relayers = relayers
			.iter()
			.map(|address| Runtime::AddressMapping::into_account_id(address.0))
			.collect::<BTreeSet<Runtime::AccountId>>();
		if unique_relayers.len() != relayers.len() {
			return Err(RevertReason::custom("Duplicate candidate address received").into());
		}

		let previous_selected_relayers =
			Self::get_previous_selected_relayers(&round_index, is_initial)?;
		if previous_selected_relayers.is_empty() {
			return Ok(false);
		}

		Ok(relayers.iter().all(|relayer| {
			previous_selected_relayers
				.contains(&Runtime::AddressMapping::into_account_id(relayer.0))
		}))
	}

	#[precompile::public("isHeartbeatPulsed(address)")]
	#[precompile::public("is_heartbeat_pulsed(address)")]
	#[precompile::view]
	fn is_heartbeat_pulsed(
		handle: &mut impl PrecompileHandle,
		relayer: Address,
	) -> EvmResult<bool> {
		let relayer = Runtime::AddressMapping::into_account_id(relayer.0);

		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let is_heartbeat_pulsed = RelayManagerOf::<Runtime>::is_heartbeat_pulsed(&relayer);

		Ok(is_heartbeat_pulsed)
	}

	// Storage getters

	#[precompile::public("selectedRelayers(bool)")]
	#[precompile::public("selected_relayers(bool)")]
	#[precompile::view]
	fn selected_relayers(
		handle: &mut impl PrecompileHandle,
		is_initial: bool,
	) -> EvmResult<Vec<Address>> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let selected_relayers = match is_initial {
			true => RelayManagerOf::<Runtime>::initial_selected_relayers(),
			false => RelayManagerOf::<Runtime>::selected_relayers(),
		};

		let result = selected_relayers
			.into_iter()
			.map(|address| Address(address.into()))
			.collect::<Vec<Address>>();

		Ok(result)
	}

	#[precompile::public("previousSelectedRelayers(uint256,bool)")]
	#[precompile::public("previous_selected_relayers(uint256,bool)")]
	#[precompile::view]
	fn previous_selected_relayers(
		handle: &mut impl PrecompileHandle,
		round_index: RoundIndex,
		is_initial: bool,
	) -> EvmResult<Vec<Address>> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let previous_selected_relayers =
			Self::get_previous_selected_relayers(&round_index, is_initial)?;

		Ok(previous_selected_relayers
			.into_iter()
			.map(|account_id| Address(account_id.into()))
			.collect())
	}

	#[precompile::public("relayerPool()")]
	#[precompile::public("relayer_pool()")]
	#[precompile::view]
	fn relayer_pool(handle: &mut impl PrecompileHandle) -> EvmResult<(Vec<Address>, Vec<Address>)> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let relayer_pool = RelayManagerOf::<Runtime>::relayer_pool();

		let mut relayers: Vec<Address> = vec![];
		let mut controllers: Vec<Address> = vec![];

		for r in relayer_pool {
			relayers.push(Address(r.relayer.into()));
			controllers.push(Address(r.controller.into()));
		}

		Ok((relayers, controllers))
	}

	#[precompile::public("majority(bool)")]
	#[precompile::view]
	fn majority(handle: &mut impl PrecompileHandle, is_initial: bool) -> EvmResult<U256> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let majority = match is_initial {
			true => RelayManagerOf::<Runtime>::initial_majority(),
			false => RelayManagerOf::<Runtime>::majority(),
		};

		Ok(majority.into())
	}

	#[precompile::public("previousMajority(uint256,bool)")]
	#[precompile::public("previous_majority(uint256,bool)")]
	#[precompile::view]
	fn previous_majority(
		handle: &mut impl PrecompileHandle,
		round_index: RoundIndex,
		is_initial: bool,
	) -> EvmResult<U256> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let cached_majority = match is_initial {
			true => RelayManagerOf::<Runtime>::cached_initial_majority(),
			false => RelayManagerOf::<Runtime>::cached_majority(),
		};

		let mut result = 0u32;
		let cached_len = cached_majority.len();
		if cached_len > 0 {
			let head_majority = &cached_majority[0];
			let tail_majority = &cached_majority[cached_len - 1];

			if round_index < head_majority.0 || round_index > tail_majority.0 {
				return Err(RevertReason::read_out_of_bounds("round_index").into());
			}
			for majority in cached_majority {
				if round_index == majority.0 {
					result = majority.1;
					break;
				}
			}
		}

		Ok(result.into())
	}

	#[precompile::public("latestRound()")]
	#[precompile::public("latest_round()")]
	#[precompile::view]
	fn latest_round(handle: &mut impl PrecompileHandle) -> EvmResult<RoundIndex> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let round = RelayManagerOf::<Runtime>::round();

		Ok(round)
	}

	#[precompile::public("relayerState(address)")]
	#[precompile::public("relayer_state(address)")]
	#[precompile::view]
	fn relayer_state(
		handle: &mut impl PrecompileHandle,
		relayer: Address,
	) -> EvmResult<EvmRelayerStateOf> {
		let relayer = Runtime::AddressMapping::into_account_id(relayer.0);

		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let mut relayer_state = RelayerStates::<Runtime>::default();

		if let Some(state) = RelayManagerOf::<Runtime>::relayer_state(&relayer) {
			let mut new = RelayerState::<Runtime>::default();
			new.set_state(relayer, state);
			relayer_state.insert_state(new);
		} else {
			relayer_state.insert_empty();
		}

		Ok(relayer_state.into())
	}

	#[precompile::public("relayerStates()")]
	#[precompile::public("relayer_states()")]
	#[precompile::view]
	fn relayer_states(handle: &mut impl PrecompileHandle) -> EvmResult<EvmRelayerStatesOf> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let mut relayer_states = RelayerStates::<Runtime>::default();

		for relayer in pallet_relay_manager::RelayerState::<Runtime>::iter() {
			let owner: Runtime::AccountId = relayer.0;
			let state = relayer.1;
			let mut new = RelayerState::<Runtime>::default();
			new.set_state(owner, state);
			relayer_states.insert_state(new);
		}

		Ok(relayer_states.into())
	}

	// Dispatchable methods

	#[precompile::public("heartbeat()")]
	fn heartbeat(handle: &mut impl PrecompileHandle) -> EvmResult {
		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = RelayManagerCall::<Runtime>::heartbeat {};

		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}

	#[precompile::public("heartbeatV2(uint256,bytes32)")]
	#[precompile::public("heartbeat_v2(uint256,bytes32)")]
	fn heartbeat_v2(
		handle: &mut impl PrecompileHandle,
		impl_version: u32,
		spec_version: H256,
	) -> EvmResult {
		let origin = Runtime::AddressMapping::into_account_id(handle.context().caller);

		let call = RelayManagerCall::<Runtime>::heartbeat_v2 {
			impl_version,
			spec_version: spec_version.into(),
		};

		RuntimeHelper::<Runtime>::try_dispatch(handle, Some(origin).into(), call)?;

		Ok(())
	}
}
