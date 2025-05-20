use crate::pallet::BlockNumberFor;

use frame_support::pallet_prelude::Weight;
use frame_system::{pallet_prelude::OriginFor, RawOrigin};
use pallet_ethereum::RawOrigin as EthereumRawOrigin;
use pallet_evm::{ExitReason, Runner};
use sp_core::{H160, H256, U256};
use sp_runtime::{
	traits::{BadOrigin, UniqueSaturatedInto},
	SaturatedConversion,
};
use sp_std::vec;

use super::pallet::*;

impl<T> Pallet<T>
where
	T: Config,
	T::AccountId: Into<H160>,
	H160: Into<T::AccountId>,
{
	pub fn ensure_whitelisted(origin: T::RuntimeOrigin) -> Result<T::AccountId, BadOrigin> {
		match origin.into() {
			Ok(RawOrigin::Signed(signer)) if <WhitelistedOwners<T>>::get().contains(&signer) => {
				Ok(signer)
			},
			_ => Err(BadOrigin),
		}
	}

	/// Execute a contract call
	pub fn execute_contract_call(n: BlockNumberFor<T>) -> Weight
	where
		OriginFor<T>: Into<Result<EthereumRawOrigin, OriginFor<T>>>,
		BlockNumberFor<T>: UniqueSaturatedInto<u32>,
	{
		let scheduled_calls = ScheduledCalls::<T>::get();
		let max_gas_limit_per_call = MaxGasLimitPerCall::<T>::get();
		let gas_price = U256::from(1000) * U256::from(10).pow(U256::from(9));

		let mut config = <T as pallet_evm::Config>::config().clone();
		config.estimate = true;

		let mut weight = Weight::from_parts(0, 0);

		for call in scheduled_calls.iter() {
			let block_number: u32 = n.unique_saturated_into();
			if block_number % call.interval == 0 {
				let nonce = frame_system::Pallet::<T>::account_nonce(&call.from);
				let nonce_u256 = U256::from(nonce.saturated_into::<u64>());

				match <T as pallet_evm::Config>::Runner::call(
					call.from.clone().into(),
					call.to.clone().into(),
					call.data.clone(),
					call.value,
					max_gas_limit_per_call,
					Some(gas_price),
					None,
					Some(nonce_u256),
					vec![],
					false,
					true,
					None,
					None,
					&config,
				) {
					Ok(estimate_result) => {
						match estimate_result.exit_reason {
							ExitReason::Succeed(_) => {
								log::info!(
									"Estimation succeeded, proceeding with transaction: {:?}",
									estimate_result
								);

								// Reset the nonce to the original value
								// The previous call will have incremented it (Since the context is the same)
								frame_system::Pallet::<T>::set_account_nonce(&call.from, nonce);

								// Create a transaction that will be recorded in history
								let transaction = pallet_ethereum::Transaction::Legacy(
									ethereum::LegacyTransaction {
										nonce: U256::from(nonce_u256),
										gas_price,
										gas_limit: U256::from(max_gas_limit_per_call),
										action: pallet_ethereum::TransactionAction::Call(
											call.to.clone().into(),
										),
										value: call.value,
										input: call.data.clone(),
										signature: ethereum::TransactionSignature::new(
											27,                         // v: 27 for valid signature
											H256::from_slice(&[1; 32]), // r: non-zero value
											H256::from_slice(&[2; 32]), // s: non-zero value
										)
										.unwrap(),
									},
								);

								// Execute the transaction with the source account as the sender
								let result = pallet_ethereum::Pallet::<T>::transact_unsigned(
									RawOrigin::None.into(),
									call.from.clone().into(),
									transaction,
								)
								.unwrap();

								weight +=
									weight.saturating_add(result.actual_weight.unwrap_or_default());

								Self::deposit_event(Event::CallSucceeded(call.clone()));
							},
							_ => {
								log::error!("EVM estimation failed: {:?}", estimate_result);
								todo!()
							},
						}
					},
					Err(_) => {
						log::error!("EVM estimation failed");
						continue;
					},
				}
			}
		}

		weight
	}
}
