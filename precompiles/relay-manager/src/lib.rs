#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(test, feature(assert_matches))]

use frame_support::dispatch::{Dispatchable, GetDispatchInfo, PostDispatchInfo};

use pallet_evm::AddressMapping;
use pallet_relay_manager::Call as RelayManagerCall;

use precompile_utils::prelude::*;

use bp_staking::RoundIndex;
use sp_core::{H160, H256, U256};
use sp_std::{marker::PhantomData, vec, vec::Vec};

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

	#[precompile::public("isCompleteSelectedRelayers(address[],bool)")]
	#[precompile::public("is_complete_selected_relayers(address[],bool)")]
	#[precompile::view]
	fn is_complete_selected_relayers(
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
		let selected_relayers = RelayManagerOf::<Runtime>::selected_relayers();
		if selected_relayers.len() != unique_relayers.len() {
			is_relayers = false;
		} else {
			for relayer in unique_relayers {
				if !RelayManagerOf::<Runtime>::is_selected_relayer(&relayer, is_initial) {
					is_relayers = false;
					break
				}
			}
		}

		Ok(is_relayers)
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
		let mut result: bool = false;
		let previous_selected_relayers = match is_initial {
			true => RelayManagerOf::<Runtime>::cached_initial_selected_relayers(),
			false => RelayManagerOf::<Runtime>::cached_selected_relayers(),
		};

		let cached_len = previous_selected_relayers.len();
		if cached_len > 0 {
			let head_selected = &previous_selected_relayers[0];
			let tail_selected = &previous_selected_relayers[cached_len - 1];

			// out of round index
			if round_index < head_selected.0 || round_index > tail_selected.0 {
				return Err(RevertReason::read_out_of_bounds("round_index").into())
			}
			'outer: for selected_relayers in previous_selected_relayers {
				if round_index == selected_relayers.0 {
					for selected_relayer in selected_relayers.1 {
						if relayer == selected_relayer {
							result = true;
							break 'outer
						}
					}
					break
				}
			}
		}

		Ok(result)
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

		let mut result: bool = false;
		if unique_relayers.len() > 0 {
			let previous_selected_relayers = match is_initial {
				true => RelayManagerOf::<Runtime>::cached_initial_selected_relayers(),
				false => RelayManagerOf::<Runtime>::cached_selected_relayers(),
			};

			let cached_len = previous_selected_relayers.len();
			if cached_len > 0 {
				let head_selected = &previous_selected_relayers[0];
				let tail_selected = &previous_selected_relayers[cached_len - 1];

				if round_index < head_selected.0 || round_index > tail_selected.0 {
					return Err(RevertReason::read_out_of_bounds("round_index").into())
				}
				'outer: for selected_relayers in previous_selected_relayers {
					if round_index == selected_relayers.0 {
						let mutated_relayers: Vec<Address> = selected_relayers
							.1
							.into_iter()
							.map(|address| Address(address.into()))
							.collect();
						for relayer in relayers {
							if !mutated_relayers.contains(&relayer) {
								break 'outer
							}
						}
						result = true;
						break
					}
				}
			}
		}

		Ok(result)
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
		let previous_selected_relayers = match is_initial {
			true => RelayManagerOf::<Runtime>::cached_initial_selected_relayers(),
			false => RelayManagerOf::<Runtime>::cached_selected_relayers(),
		};

		let mut result: Vec<Address> = vec![];
		let cached_len = previous_selected_relayers.len();
		if cached_len > 0 {
			let head_selected = &previous_selected_relayers[0];
			let tail_selected = &previous_selected_relayers[cached_len - 1];

			// out of round index
			if round_index < head_selected.0 || round_index > tail_selected.0 {
				return Err(RevertReason::read_out_of_bounds("round_index").into())
			}
			for relayers in previous_selected_relayers {
				if round_index == relayers.0 {
					result =
						relayers.1.into_iter().map(|address| Address(address.into())).collect();
					break
				}
			}
		}

		Ok(result)
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
				return Err(RevertReason::read_out_of_bounds("round_index").into())
			}
			for majority in cached_majority {
				if round_index == majority.0 {
					result = majority.1;
					break
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
}
