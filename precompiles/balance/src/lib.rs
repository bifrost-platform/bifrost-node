#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(test, feature(assert_matches))]

use frame_support::dispatch::{Dispatchable, GetDispatchInfo, PostDispatchInfo};

use fp_evm::{Context, ExitSucceed, PrecompileOutput};
use pallet_balances::Call as BalanceCall;
use pallet_evm::Precompile;
use precompile_utils::{
	EvmData, EvmDataReader, EvmDataWriter, EvmResult, FunctionModifier, Gasometer, RuntimeHelper,
};

use sp_core::{H160, H256};
use sp_std::{fmt::Debug, marker::PhantomData};

#[precompile_utils::generate_function_selector]
#[derive(Debug, PartialEq)]
enum Action {
	// Storage getters
	TotalIssuance = "total_issuance()",
}

/// A precompile to wrap the functionality from pallet_balances
pub struct BalancePrecompile<Runtime>(PhantomData<Runtime>);

impl<Runtime> Precompile for BalancePrecompile<Runtime>
where
	Runtime: pallet_balances::Config + pallet_evm::Config + frame_system::Config,
	Runtime::Call: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	<Runtime::Call as Dispatchable>::Origin: From<Option<Runtime::AccountId>>,
	<Runtime as pallet_balances::Config>::Balance: EvmData,
	Runtime::Call: From<BalanceCall<Runtime>>,
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

		let (mut _input, selector) = EvmDataReader::new_with_selector(gasometer, input)?;

		gasometer.check_function_modifier(
			context,
			is_static,
			match selector {
				Action::TotalIssuance => FunctionModifier::NonPayable,
			},
		)?;

		match selector {
			// Storage getters
			Action::TotalIssuance => return Self::total_issuance(gasometer),
		};
	}
}

impl<Runtime> BalancePrecompile<Runtime>
where
	Runtime: pallet_balances::Config + pallet_evm::Config + frame_system::Config,
	Runtime::Call: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	<Runtime::Call as Dispatchable>::Origin: From<Option<Runtime::AccountId>>,
	<Runtime as pallet_balances::Config>::Balance: EvmData,
	Runtime::Call: From<BalanceCall<Runtime>>,
	Runtime::Hash: From<H256> + Into<H256>,
	Runtime::AccountId: Into<H160>,
{
	// Storage getters

	fn total_issuance(gasometer: &mut Gasometer) -> EvmResult<PrecompileOutput> {
		gasometer.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let total_issuance = <pallet_balances::Pallet<Runtime>>::total_issuance();

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			cost: gasometer.used_gas(),
			output: EvmDataWriter::new().write(total_issuance).build(),
			logs: Default::default(),
		})
	}
}
