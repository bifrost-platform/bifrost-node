use crate::pallet::{BlockNumberFor, CallInfo};

use frame_support::pallet_prelude::Weight;
use frame_system::{pallet_prelude::OriginFor, RawOrigin};
use pallet_ethereum::RawOrigin as EthereumRawOrigin;
use pallet_evm::{ExitError, ExitReason, GasWeightMapping, Runner};
use sp_core::{H160, H256, U256};
use sp_io::storage::{rollback_transaction, start_transaction};
use sp_runtime::{
	traits::{BadOrigin, UniqueSaturatedInto},
	SaturatedConversion,
};
use sp_std::{vec, vec::Vec};

use super::pallet::*;

impl<T> Pallet<T>
where
	T: Config,
	T::AccountId: Into<H160>,
	H160: Into<T::AccountId>,
{
	/// Ensure the caller is whitelisted.
	pub fn ensure_whitelisted(origin: T::RuntimeOrigin) -> Result<(), BadOrigin> {
		match origin.into() {
			Ok(RawOrigin::Signed(signer)) if <WhitelistedOwners<T>>::get().contains(&signer) => {
				Ok(())
			},
			_ => Err(BadOrigin),
		}
	}

	/// Execute scheduled contract calls. Before the actual execution, it will estimate the gas cost.
	pub fn execute_contract_calls(n: BlockNumberFor<T>) -> Weight
	where
		OriginFor<T>: Into<Result<EthereumRawOrigin, OriginFor<T>>>,
		BlockNumberFor<T>: UniqueSaturatedInto<u32>,
	{
		let scheduled_calls = ScheduledCalls::<T>::iter();
		let gas_price = U256::from(1000) * U256::from(10).pow(U256::from(9));

		let mut config = <T as pallet_evm::Config>::config().clone();
		config.estimate = true;

		let mut weight = Weight::from_parts(0, 0);

		let mut executable_calls = vec![];
		let mut events: Vec<Event<T>> = vec![];

		start_transaction();
		for (_, call) in scheduled_calls {
			let block_number: u32 = n.unique_saturated_into();
			let CallInfo::<T::AccountId> { interval, ref from, ref to, ref data, value, gas } =
				call.info;

			if block_number % interval == 0 {
				let estimate_result: pallet_evm::CallInfo =
					match <T as pallet_evm::Config>::Runner::call(
						from.clone().into(),
						to.clone().into(),
						data.clone(),
						value,
						gas.saturated_into::<u64>(),
						Some(gas_price),
						None,
						None,
						vec![],
						false,
						true,
						None,
						None,
						&config,
					)
					.map_err(|e| e.error.into())
					{
						Ok(result) => result,
						Err(_) => {
							events.push(Event::Estimated {
								from: from.clone(),
								to: to.clone(),
								value: value.clone(),
								input: data.clone(),
								gas_used: U256::zero(),
								exit_reason: ExitReason::Error(ExitError::Other(
									"Estimation::UnknownError".into(),
								)),
							});
							continue;
						},
					};

				let estimated_gas = estimate_result.used_gas.standard;
				events.push(Event::Estimated {
					from: from.clone(),
					to: to.clone(),
					value: value.clone(),
					input: data.clone(),
					gas_used: estimated_gas,
					exit_reason: estimate_result.exit_reason.clone(),
				});
				match estimate_result.exit_reason {
					ExitReason::Succeed(_) => {
						executable_calls.push((call, estimated_gas));
					},
					_ => {},
				}
				weight += T::GasWeightMapping::gas_to_weight(estimated_gas.as_u64(), true);
			}
		}
		rollback_transaction();

		for event in events {
			Self::deposit_event(event);
		}

		for (call, estimated_gas) in executable_calls {
			let CallInfo::<T::AccountId> { from, to, data, value, .. } = call.info;
			let transaction = pallet_ethereum::Transaction::Legacy(ethereum::LegacyTransaction {
				nonce: U256::zero(), // use dynamic nonce
				gas_price,
				gas_limit: estimated_gas,
				action: pallet_ethereum::TransactionAction::Call(to.clone().into()),
				value,
				input: data.clone(),
				signature: ethereum::TransactionSignature::new(
					27,
					H256::from_slice(&[1; 32]),
					H256::from_slice(&[2; 32]),
				)
				.unwrap(),
			});

			match pallet_ethereum::Pallet::<T>::transact_unsigned(
				RawOrigin::None.into(),
				from.clone().into(),
				transaction,
			) {
				Ok(result) => {
					weight += weight.saturating_add(result.actual_weight.unwrap_or_default());
				},
				Err(_) => {
					Self::deposit_event(Event::Executed {
						from: from.clone(),
						to: to.clone(),
						value: value.clone(),
						input: data.clone(),
						gas_used: estimated_gas,
						exit_reason: ExitReason::Error(ExitError::Other(
							"Execution::UnknownError".into(),
						)),
					});
					continue;
				},
			}
		}

		weight
	}
}
