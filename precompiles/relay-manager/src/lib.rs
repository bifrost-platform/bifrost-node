#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(test, feature(assert_matches))]

use fp_evm::{Context, ExitError, ExitSucceed, PrecompileFailure, PrecompileOutput};
use frame_support::dispatch::{Dispatchable, GetDispatchInfo, PostDispatchInfo};
use pallet_evm::{AddressMapping, Precompile};
use pallet_relay_manager::{Call as RelayManagerCall, RelayerMetadata, RelayerStatus};
use precompile_utils::{
	Address, EvmDataReader, EvmDataWriter, EvmResult, FunctionModifier, Gasometer, RuntimeHelper,
};

use sp_core::{H160, H256};
use sp_std::{fmt::Debug, marker::PhantomData, vec, vec::Vec};

type RelayManagerOf<Runtime> = pallet_relay_manager::Pallet<Runtime>;

#[precompile_utils::generate_function_selector]
#[derive(Debug, PartialEq)]
enum Action {
	// Role verifiers
	IsRelayer = "is_relayer(address)",
	IsSelectedRelayer = "is_selected_relayer(address,bool)",
	IsRelayers = "is_relayers(address[])",
	IsSelectedRelayers = "is_selected_relayers(address[],bool)",
	IsCompleteSelectedRelayers = "is_complete_selected_relayers(address[],bool)",
	IsPreviousSelectedRelayer = "is_previous_selected_relayer(uint256,address,bool)",
	IsPreviousSelectedRelayers = "is_previous_selected_relayers(uint256,address[],bool)",
	IsHeartbeatPulsed = "is_heartbeat_pulsed(address)",
	// Storage getters
	SelectedRelayers = "selected_relayers(bool)",
	PreviousSelectedRelayers = "previous_selected_relayers(uint256,bool)",
	RelayerPool = "relayer_pool()",
	Majority = "majority(bool)",
	PreviousMajority = "previous_majority(uint256,bool)",
	LatestRound = "latest_round()",
	RelayerState = "relayer_state(address)",
	RelayerStates = "relayer_states()",
	// Dispatchable methods
	Heartbeat = "heartbeat()",
}

/// EVM struct for relayer state
struct RelayerState<Runtime: pallet_relay_manager::Config> {
	/// This relayer's account
	pub relayer: Address,
	/// This relayer's controller account
	pub controller: Address,
	/// Current status of this relayer
	pub status: u32,
	/// Zero-sized type used to mark things that "act like" they own a T
	phantom: PhantomData<Runtime>,
}

impl<Runtime> RelayerState<Runtime>
where
	Runtime: pallet_relay_manager::Config,
	Runtime::AccountId: Into<H160>,
{
	fn default() -> Self {
		RelayerState {
			relayer: Address(Default::default()),
			controller: Address(Default::default()),
			status: 0u32,
			phantom: PhantomData,
		}
	}

	fn set_state(
		&mut self,
		relayer: Runtime::AccountId,
		state: RelayerMetadata<Runtime::AccountId>,
	) {
		self.relayer = Address(relayer.into());
		self.controller = Address(state.controller.into());
		self.status = match state.status {
			RelayerStatus::KickedOut => 2u32.into(),
			RelayerStatus::Active => 1u32.into(),
			RelayerStatus::Idle => 0u32.into(),
		};
	}
}

/// EVM struct for relayer states
struct RelayerStates<Runtime: pallet_relay_manager::Config> {
	/// This relayer's account
	pub relayer: Vec<Address>,
	/// This relayer's controller account
	pub controller: Vec<Address>,
	/// Current status of this relayer
	pub status: Vec<u32>,
	/// Zero-sized type used to mark things that "act like" they own a T
	phantom: PhantomData<Runtime>,
}

impl<Runtime> RelayerStates<Runtime>
where
	Runtime: pallet_relay_manager::Config,
	Runtime::AccountId: Into<H160>,
{
	fn default() -> Self {
		RelayerStates { relayer: vec![], controller: vec![], status: vec![], phantom: PhantomData }
	}

	fn insert_empty(&mut self) {
		self.relayer.push(Address(Default::default()));
		self.controller.push(Address(Default::default()));
		self.status.push(0u32);
	}

	fn insert_state(&mut self, state: RelayerState<Runtime>) {
		self.relayer.push(Address(state.relayer.into()));
		self.controller.push(Address(state.controller.into()));
		self.status.push(state.status);
	}
}

/// A precompile to wrap the functionality from pallet_relay_manager
pub struct RelayManagerPrecompile<Runtime>(PhantomData<Runtime>);

impl<Runtime> Precompile for RelayManagerPrecompile<Runtime>
where
	Runtime: pallet_relay_manager::Config + pallet_evm::Config + frame_system::Config,
	Runtime::Hash: From<H256> + Into<H256>,
	Runtime::AccountId: Into<H160>,
	Runtime::Call: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	<Runtime::Call as Dispatchable>::Origin: From<Option<Runtime::AccountId>>,
	Runtime::Call: From<RelayManagerCall<Runtime>>,
{
	fn execute(
		input: &[u8],
		target_gas: Option<u64>,
		context: &Context,
		is_static: bool,
	) -> EvmResult<PrecompileOutput> {
		let mut gasometer = Gasometer::new(target_gas);
		let gasometer = &mut gasometer;

		let (mut input, selector) = EvmDataReader::new_with_selector(gasometer, input)?;
		let input = &mut input;

		gasometer.check_function_modifier(
			context,
			is_static,
			match selector {
				Action::IsRelayer |
				Action::IsSelectedRelayer |
				Action::IsRelayers |
				Action::IsSelectedRelayers |
				Action::IsCompleteSelectedRelayers |
				Action::IsPreviousSelectedRelayer |
				Action::IsPreviousSelectedRelayers |
				Action::IsHeartbeatPulsed |
				Action::SelectedRelayers |
				Action::PreviousSelectedRelayers |
				Action::RelayerPool |
				Action::Majority |
				Action::PreviousMajority |
				Action::LatestRound |
				Action::RelayerState |
				Action::RelayerStates => FunctionModifier::View,
				_ => FunctionModifier::NonPayable,
			},
		)?;

		let (origin, call) = match selector {
			// Role verifiers
			Action::IsRelayer => return Self::is_relayer(input, gasometer),
			Action::IsSelectedRelayer => return Self::is_selected_relayer(input, gasometer),
			Action::IsRelayers => return Self::is_relayers(input, gasometer),
			Action::IsSelectedRelayers => return Self::is_selected_relayers(input, gasometer),
			Action::IsCompleteSelectedRelayers =>
				return Self::is_complete_selected_relayers(input, gasometer),
			Action::IsPreviousSelectedRelayer =>
				return Self::is_previous_selected_relayer(input, gasometer),
			Action::IsPreviousSelectedRelayers =>
				return Self::is_previous_selected_relayers(input, gasometer),
			Action::IsHeartbeatPulsed => return Self::is_heartbeat_pulsed(input, gasometer),
			// Storage getters
			Action::SelectedRelayers => return Self::selected_relayers(input, gasometer),
			Action::PreviousSelectedRelayers =>
				return Self::previous_selected_relayers(input, gasometer),
			Action::RelayerPool => return Self::relayer_pool(gasometer),
			Action::Majority => return Self::majority(input, gasometer),
			Action::PreviousMajority => return Self::previous_majority(input, gasometer),
			Action::LatestRound => return Self::latest_round(gasometer),
			Action::RelayerState => return Self::relayer_state(input, gasometer),
			Action::RelayerStates => return Self::relayer_states(gasometer),
			// Dispatchable methods
			Action::Heartbeat => Self::heartbeat(context)?,
		};

		// Dispatch call (if enough gas).
		RuntimeHelper::<Runtime>::try_dispatch(origin, call, gasometer)?;

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: vec![],
			logs: vec![],
		})
	}
}

impl<Runtime> RelayManagerPrecompile<Runtime>
where
	Runtime: pallet_relay_manager::Config + pallet_evm::Config + frame_system::Config,
	Runtime::Hash: From<H256> + Into<H256>,
	Runtime::AccountId: Into<H160>,
	Runtime::Call: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	<Runtime::Call as Dispatchable>::Origin: From<Option<Runtime::AccountId>>,
	Runtime::Call: From<RelayManagerCall<Runtime>>,
{
	// Role verifiers

	fn is_relayer(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 1)?;
		let relayer = input.read::<Address>(gasometer)?.0;
		let relayer = Runtime::AddressMapping::into_account_id(relayer);

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let is_relayer = RelayManagerOf::<Runtime>::is_relayer(&relayer);

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(is_relayer).build(),
			logs: Default::default(),
		})
	}

	fn is_selected_relayer(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 2)?;
		let relayer = input.read::<Address>(gasometer)?.0;
		let relayer = Runtime::AddressMapping::into_account_id(relayer);
		let is_initial = input.read::<bool>(gasometer)?;

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let is_selected_relayer =
			RelayManagerOf::<Runtime>::is_selected_relayer(&relayer, is_initial);

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(is_selected_relayer).build(),
			logs: Default::default(),
		})
	}

	fn is_relayers(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 1)?;
		let relayers = input.read::<Vec<Address>>(gasometer)?;

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

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
			return Err(PrecompileFailure::Error {
				exit_status: ExitError::Other("Duplicate relayer address received".into()),
			})
		}

		let mut is_relayers = true;
		for relayer in unique_relayers {
			if !RelayManagerOf::<Runtime>::is_relayer(&relayer) {
				is_relayers = false;
				break
			}
		}

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(is_relayers).build(),
			logs: Default::default(),
		})
	}

	fn is_selected_relayers(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 2)?;
		let relayers = input.read::<Vec<Address>>(gasometer)?;
		let is_initial = input.read::<bool>(gasometer)?;

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

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
			return Err(PrecompileFailure::Error {
				exit_status: ExitError::Other("Duplicate relayer address received".into()),
			})
		}

		let mut is_relayers = true;
		for relayer in unique_relayers {
			if !RelayManagerOf::<Runtime>::is_selected_relayer(&relayer, is_initial) {
				is_relayers = false;
				break
			}
		}

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(is_relayers).build(),
			logs: Default::default(),
		})
	}

	fn is_complete_selected_relayers(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 2)?;
		let relayers = input.read::<Vec<Address>>(gasometer)?;
		let is_initial = input.read::<bool>(gasometer)?;

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

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
			return Err(PrecompileFailure::Error {
				exit_status: ExitError::Other("Duplicate relayer address received".into()),
			})
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

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(is_relayers).build(),
			logs: Default::default(),
		})
	}

	fn is_previous_selected_relayer(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 3)?;
		let round_index = input.read::<u32>(gasometer)?;
		let relayer = input.read::<Address>(gasometer)?.0;
		let relayer = Runtime::AddressMapping::into_account_id(relayer);
		let is_initial = input.read::<bool>(gasometer)?;

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
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
				return Err(PrecompileFailure::Error {
					exit_status: ExitError::Other("Out of round index".into()),
				})
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

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(result).build(),
			logs: Default::default(),
		})
	}

	fn is_previous_selected_relayers(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 3)?;
		let round_index = input.read::<u32>(gasometer)?;
		let relayers = input.read::<Vec<Address>>(gasometer)?;
		let is_initial = input.read::<bool>(gasometer)?;

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

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
			return Err(PrecompileFailure::Error {
				exit_status: ExitError::Other("Duplicate relayer address received".into()),
			})
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
					return Err(PrecompileFailure::Error {
						exit_status: ExitError::Other("Round index out of bound".into()),
					})
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

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(result).build(),
			logs: Default::default(),
		})
	}

	fn is_heartbeat_pulsed(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 1)?;
		let relayer = input.read::<Address>(gasometer)?.0;
		let relayer = Runtime::AddressMapping::into_account_id(relayer);

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let is_heartbeat_pulsed = RelayManagerOf::<Runtime>::is_heartbeat_pulsed(&relayer);

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(is_heartbeat_pulsed).build(),
			logs: Default::default(),
		})
	}

	// Storage getters

	fn selected_relayers(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 1)?;
		let is_initial = input.read::<bool>(gasometer)?;

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let selected_relayers = match is_initial {
			true => RelayManagerOf::<Runtime>::initial_selected_relayers(),
			false => RelayManagerOf::<Runtime>::selected_relayers(),
		};

		let result = selected_relayers
			.into_iter()
			.map(|address| Address(address.into()))
			.collect::<Vec<Address>>();

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(result).build(),
			logs: Default::default(),
		})
	}

	fn previous_selected_relayers(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 2)?;
		let round_index = input.read::<u32>(gasometer)?;
		let is_initial = input.read::<bool>(gasometer)?;

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
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
				return Err(PrecompileFailure::Error {
					exit_status: ExitError::Other("Out of round index".into()),
				})
			}
			for relayers in previous_selected_relayers {
				if round_index == relayers.0 {
					result =
						relayers.1.into_iter().map(|address| Address(address.into())).collect();
					break
				}
			}
		}

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(result).build(),
			logs: Default::default(),
		})
	}

	fn relayer_pool(gasometer: &mut Gasometer) -> EvmResult<PrecompileOutput> {
		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let relayer_pool = RelayManagerOf::<Runtime>::relayer_pool();

		let mut relayers: Vec<Address> = vec![];
		let mut controllers: Vec<Address> = vec![];

		for r in relayer_pool {
			relayers.push(Address(r.relayer.into()));
			controllers.push(Address(r.controller.into()));
		}

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(relayers).write(controllers).build(),
			logs: Default::default(),
		})
	}

	fn majority(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 1)?;
		let is_initial = input.read::<bool>(gasometer)?;

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let majority = match is_initial {
			true => RelayManagerOf::<Runtime>::initial_majority(),
			false => RelayManagerOf::<Runtime>::majority(),
		};

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(majority).build(),
			logs: Default::default(),
		})
	}

	fn previous_majority(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 2)?;
		let round_index = input.read::<u32>(gasometer)?;
		let is_initial = input.read::<bool>(gasometer)?;

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
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
				return Err(PrecompileFailure::Error {
					exit_status: ExitError::Other("Round index out of bound".into()),
				})
			}
			for majority in cached_majority {
				if round_index == majority.0 {
					result = majority.1;
					break
				}
			}
		}

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(result).build(),
			logs: Default::default(),
		})
	}

	fn latest_round(gasometer: &mut Gasometer) -> EvmResult<PrecompileOutput> {
		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let round = RelayManagerOf::<Runtime>::round();

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(round).build(),
			logs: Default::default(),
		})
	}

	fn relayer_state(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 1)?;
		let relayer = input.read::<Address>(gasometer)?.0;
		let relayer = Runtime::AddressMapping::into_account_id(relayer);

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let mut relayer_state = RelayerStates::<Runtime>::default();

		if let Some(state) = RelayManagerOf::<Runtime>::relayer_state(&relayer) {
			let mut new = RelayerState::<Runtime>::default();
			new.set_state(relayer, state);
			relayer_state.insert_state(new);
		} else {
			relayer_state.insert_empty();
		}

		let output = EvmDataWriter::new()
			.write(relayer_state.relayer[0])
			.write(relayer_state.controller[0])
			.write(relayer_state.status[0])
			.build();

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output,
			logs: Default::default(),
		})
	}

	fn relayer_states(gasometer: &mut Gasometer) -> EvmResult<PrecompileOutput> {
		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let mut relayer_states = RelayerStates::<Runtime>::default();

		for relayer in pallet_relay_manager::RelayerState::<Runtime>::iter() {
			let owner: Runtime::AccountId = relayer.0;
			let state = relayer.1;
			let mut new = RelayerState::<Runtime>::default();
			new.set_state(owner, state);
			relayer_states.insert_state(new);
		}

		let output = EvmDataWriter::new()
			.write(relayer_states.relayer)
			.write(relayer_states.controller)
			.write(relayer_states.status)
			.build();

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output,
			logs: Default::default(),
		})
	}

	// Dispatchable methods

	fn heartbeat(
		context: &Context,
	) -> EvmResult<(<Runtime::Call as Dispatchable>::Origin, RelayManagerCall<Runtime>)> {
		let origin = Runtime::AddressMapping::into_account_id(context.caller);
		let call = RelayManagerCall::<Runtime>::heartbeat {};

		Ok((Some(origin).into(), call))
	}
}
