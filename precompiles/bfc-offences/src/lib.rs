#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(test, feature(assert_matches))]

use fp_evm::{Context, ExitError, ExitSucceed, PrecompileFailure, PrecompileOutput};
use frame_support::traits::Currency;
use pallet_bfc_offences::{OffenceCount, ValidatorOffenceInfo};
use pallet_evm::{AddressMapping, Precompile};
use precompile_utils::{
	Address, EvmDataReader, EvmDataWriter, EvmResult, FunctionModifier, Gasometer, RuntimeHelper,
};

use bp_staking::{RoundIndex, TierType};
use sp_core::{H160, H256};
use sp_std::{fmt::Debug, marker::PhantomData, vec, vec::Vec};

type BalanceOf<Runtime> = <<Runtime as pallet_bfc_offences::Config>::Currency as Currency<
	<Runtime as frame_system::Config>::AccountId,
>>::Balance;

type OffencesOf<Runtime> = pallet_bfc_offences::Pallet<Runtime>;

#[precompile_utils::generate_function_selector]
#[derive(Debug, PartialEq)]
enum Action {
	// Storage getters
	MaximumOffenceCount = "maximum_offence_count(uint256)",
	ValidatorOffence = "validator_offence(address)",
	ValidatorOffences = "validator_offences(address[])",
}

/// EVM struct for validator offence
struct ValidatorOffence<Runtime: pallet_bfc_offences::Config> {
	/// The address of this validator
	pub validator: Address,
	/// The latest round this validator earned an offence
	pub latest_offence_round_index: RoundIndex,
	/// The latest session this validator earned an offence
	pub latest_offence_session_index: RoundIndex,
	/// The total offences this validator earned
	pub offence_count: OffenceCount,
	/// A zero-sized type used to mark things that "act like" they own a T.
	phantom: PhantomData<Runtime>,
}

impl<Runtime> ValidatorOffence<Runtime>
where
	Runtime: pallet_bfc_offences::Config,
	Runtime::AccountId: Into<H160>,
{
	fn default() -> Self {
		ValidatorOffence {
			validator: Address(Default::default()),
			latest_offence_round_index: 0u32,
			latest_offence_session_index: 0u32,
			offence_count: 0u32,
			phantom: PhantomData,
		}
	}

	fn set_offence(
		&mut self,
		validator: Runtime::AccountId,
		offence: ValidatorOffenceInfo<BalanceOf<Runtime>>,
	) {
		self.validator = Address(validator.into());
		self.latest_offence_round_index = offence.latest_offence_round_index;
		self.latest_offence_session_index = offence.latest_offence_session_index;
		self.offence_count = offence.offence_count;
	}
}

/// EVM struct for validator offences
struct ValidatorOffences<Runtime: pallet_bfc_offences::Config> {
	/// The address of this validator
	pub validator: Vec<Address>,
	/// The latest round this validator earned an offence
	pub latest_offence_round_index: Vec<RoundIndex>,
	/// The latest session this validator earned an offence
	pub latest_offence_session_index: Vec<RoundIndex>,
	/// The total offences this validator earned
	pub offence_count: Vec<OffenceCount>,
	/// A zero-sized type used to mark things that "act like" they own a T.
	phantom: PhantomData<Runtime>,
}

impl<Runtime> ValidatorOffences<Runtime>
where
	Runtime: pallet_bfc_offences::Config,
	Runtime::AccountId: Into<H160>,
{
	fn default() -> Self {
		ValidatorOffences {
			validator: vec![],
			latest_offence_round_index: vec![],
			latest_offence_session_index: vec![],
			offence_count: vec![],
			phantom: PhantomData,
		}
	}

	fn insert_empty(&mut self) {
		self.validator.push(Address(Default::default()));
		self.latest_offence_round_index.push(0u32);
		self.latest_offence_session_index.push(0u32);
		self.offence_count.push(0u32);
	}

	fn insert_offence(&mut self, offence: ValidatorOffence<Runtime>) {
		self.validator.push(Address(offence.validator.into()));
		self.latest_offence_round_index.push(offence.latest_offence_round_index);
		self.latest_offence_session_index.push(offence.latest_offence_session_index);
		self.offence_count.push(offence.offence_count);
	}
}

/// A precompile to wrap the functionality from pallet_bfc_offences
pub struct BfcOffencesPrecompile<Runtime>(PhantomData<Runtime>);

impl<Runtime> Precompile for BfcOffencesPrecompile<Runtime>
where
	Runtime: pallet_bfc_offences::Config + pallet_evm::Config + frame_system::Config,
	Runtime::Hash: From<H256> + Into<H256>,
	Runtime::AccountId: Into<H160>,
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
				Action::MaximumOffenceCount |
				Action::ValidatorOffence |
				Action::ValidatorOffences => FunctionModifier::View,
			},
		)?;

		match selector {
			// Storage getters
			Action::MaximumOffenceCount => return Self::maximum_offence_count(input, gasometer),
			Action::ValidatorOffence => return Self::validator_offence(input, gasometer),
			Action::ValidatorOffences => return Self::validator_offences(input, gasometer),
		};
	}
}

impl<Runtime> BfcOffencesPrecompile<Runtime>
where
	Runtime: pallet_bfc_offences::Config + pallet_evm::Config + frame_system::Config,
	Runtime::Hash: From<H256> + Into<H256>,
	Runtime::AccountId: Into<H160>,
{
	// Storage getters

	fn maximum_offence_count(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 1)?;
		let raw_tier = input.read::<u32>(gasometer)?;
		let tier = match raw_tier {
			2u32 => TierType::Full,
			1u32 => TierType::Basic,
			0u32 => TierType::All,
			_ =>
				return Err(PrecompileFailure::Error {
					exit_status: ExitError::Other("Tier out of bound".into()),
				}),
		};

		let mut maximum_offence_count = vec![];
		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		match tier {
			TierType::Full => {
				maximum_offence_count.push(OffencesOf::<Runtime>::full_maximum_offence_count());
			},
			TierType::Basic => {
				maximum_offence_count.push(OffencesOf::<Runtime>::basic_maximum_offence_count());
			},
			TierType::All => {
				maximum_offence_count.push(OffencesOf::<Runtime>::full_maximum_offence_count());
				maximum_offence_count.push(OffencesOf::<Runtime>::basic_maximum_offence_count());
			},
		}

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(maximum_offence_count).build(),
			logs: Default::default(),
		})
	}

	fn validator_offence(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 1)?;
		let validator = input.read::<Address>(gasometer)?.0;
		let validator = Runtime::AddressMapping::into_account_id(validator);

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let mut validator_offence = ValidatorOffences::<Runtime>::default();

		if let Some(offence) = OffencesOf::<Runtime>::validator_offences(&validator) {
			let mut new = ValidatorOffence::<Runtime>::default();
			new.set_offence(validator, offence);
			validator_offence.insert_offence(new);
		} else {
			validator_offence.insert_empty();
		}

		let output = EvmDataWriter::new()
			.write(validator_offence.validator[0])
			.write(validator_offence.latest_offence_round_index[0])
			.write(validator_offence.latest_offence_session_index[0])
			.write(validator_offence.offence_count[0])
			.build();

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output,
			logs: Default::default(),
		})
	}

	fn validator_offences(
		input: &mut EvmDataReader,
		gasometer: &mut Gasometer,
	) -> EvmResult<PrecompileOutput> {
		input.expect_arguments(gasometer, 1)?;
		let validators = input.read::<Vec<Address>>(gasometer)?;

		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let mut unique_validators = validators
			.clone()
			.into_iter()
			.map(|address| Runtime::AddressMapping::into_account_id(address.0))
			.collect::<Vec<Runtime::AccountId>>();

		let previous_len = unique_validators.len();
		unique_validators.sort();
		unique_validators.dedup();
		let current_len = unique_validators.len();
		if current_len < previous_len {
			return Err(PrecompileFailure::Error {
				exit_status: ExitError::Other("Duplicate validator address received".into()),
			})
		}

		let mut validator_offences = ValidatorOffences::<Runtime>::default();
		unique_validators.clone().into_iter().for_each(|v| {
			if let Some(offence) = OffencesOf::<Runtime>::validator_offences(&v) {
				let mut new = ValidatorOffence::<Runtime>::default();
				new.set_offence(v, offence);
				validator_offences.insert_offence(new);
			} else {
				validator_offences.insert_empty();
			}
		});

		let output = EvmDataWriter::new()
			.write(validator_offences.validator)
			.write(validator_offences.latest_offence_round_index)
			.write(validator_offences.latest_offence_session_index)
			.write(validator_offences.offence_count)
			.build();

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output,
			logs: Default::default(),
		})
	}
}
