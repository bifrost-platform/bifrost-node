extern crate alloc;

use crate::pallet::{BlockNumberFor, CallInfo};

use frame_support::pallet_prelude::Weight;
use frame_system::{pallet_prelude::OriginFor, RawOrigin};
use lite_json::json::JsonValue;
use pallet_ethereum::RawOrigin as EthereumRawOrigin;
use pallet_evm::{ExitError, ExitReason, GasWeightMapping, Runner};
use sp_core::{H160, H256, U256};
use sp_io::storage::{rollback_transaction, start_transaction};
use sp_runtime::{
	offchain::{http, Duration},
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

	/// Fetch current price and return the result in cents.
	pub fn fetch_price() -> Result<(), http::Error> {
		// We want to keep the offchain worker execution time reasonable, so we set a hard-coded
		// deadline to 2s to complete the external call.
		// You can also wait indefinitely for the response, however you may still get a timeout
		// coming from the host machine.
		let deadline = sp_io::offchain::timestamp().add(Duration::from_millis(2_000));
		// Initiate an external HTTP GET request.
		// This is using high-level wrappers from `sp_runtime`, for the low-level calls that
		// you can find in `sp_io`. The API is trying to be similar to `request`, but
		// since we are running in a custom WASM execution environment we can't simply
		// import the library here.
		let request = http::Request::get(
			"https://api.coingecko.com/api/v3/simple/price?ids=bitcoin&vs_currencies=usd",
		);
		// We set the deadline for sending of the request, note that awaiting response can
		// have a separate deadline. Next we send the request, before that it's also possible
		// to alter request headers or stream body content in case of non-GET requests.
		let pending = request.deadline(deadline).send().map_err(|_| http::Error::IoError)?;

		// The request is already being processed by the host, we are free to do anything
		// else in the worker (we can send multiple concurrent requests too).
		// At some point however we probably want to check the response though,
		// so we can block current thread and wait for it to finish.
		// Note that since the request is being driven by the host, we don't have to wait
		// for the request to have it complete, we will just not read the response.
		let response = pending.try_wait(deadline).map_err(|_| http::Error::DeadlineReached)??;
		// Let's check the status code before we proceed to reading the response.
		if response.code != 200 {
			log::warn!("Unexpected status code: {}", response.code);
			return Err(http::Error::Unknown);
		}
		log::info!("response: {:?}", response);

		// Next we want to fully read the response body and collect it to a vector of bytes.
		// Note that the return object allows you to read the body in chunks as well
		// with a way to control the deadline.
		let body = response.body().collect::<Vec<u8>>();
		log::info!("body: {:?}", body);

		// Create a str slice from the body.
		let body_str = alloc::str::from_utf8(&body).map_err(|_| {
			log::warn!("No UTF8 body");
			http::Error::Unknown
		})?;
		log::info!("body_str: {:?}", body_str);

		let price = Self::parse_price(body_str).ok_or(http::Error::Unknown)?;
		log::info!("price: {:?}", price);

		Ok(())
	}

	fn parse_price(price_str: &str) -> Option<u64> {
		let val = lite_json::parse_json(price_str);
		let price = match val.ok()? {
			JsonValue::Object(obj) => {
				// First get the bitcoin object
				let (_, bitcoin_obj) =
					obj.into_iter().find(|(k, _)| k.iter().copied().eq("bitcoin".chars()))?;
				// Then get the usd value from the bitcoin object
				match bitcoin_obj {
					JsonValue::Object(bitcoin) => {
						let (_, usd_value) = bitcoin
							.into_iter()
							.find(|(k, _)| k.iter().copied().eq("usd".chars()))?;
						match usd_value {
							JsonValue::Number(number) => number,
							_ => return None,
						}
					},
					_ => return None,
				}
			},
			_ => return None,
		};

		Some(price.integer)
	}
}
