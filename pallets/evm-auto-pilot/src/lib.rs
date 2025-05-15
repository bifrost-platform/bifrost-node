#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;
pub mod weights;
use frame_support::dispatch::RawOrigin;
use hex;
use log;
use pallet_ethereum::{Call as EthereumCall, Transaction};
use pallet_evm::Call as EvmCall;
use weights::WeightInfo;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;
	use pallet_evm::{EvmConfig, ExitReason, Runner};
	use sp_core::{H160, H256, U256};
	use sp_runtime::traits::Dispatchable;
	use sp_std::{fmt::Debug, vec, vec::Vec};

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_evm::Config + pallet_ethereum::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		/// Weight information for extrinsics
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The scheduled call already exists
		CallAlreadyExists,
		/// The scheduled call does not exist
		CallDoesNotExist,
		/// The interval is too short
		IntervalTooShort,
		/// Too many scheduled calls
		TooManyScheduledCalls,
		/// The contract call failed
		ContractCallFailed,
		/// The contract call reverted
		ContractCallReverted,
		/// The contract call ran out of gas
		ContractCallOutOfGas,
		/// The contract call failed due to invalid input
		ContractCallInvalidInput,
		InvalidHex,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new contract call has been scheduled
		CallScheduled {
			call_id: T::Hash,
			target: H160,
			data: Vec<u8>,
			interval: u64,
		},
		/// A scheduled call has been executed
		CallExecuted {
			call_id: T::Hash,
			target: H160,
		},
		/// A scheduled call has been removed
		CallRemoved {
			call_id: T::Hash,
		},
		Done,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T>
	where
		<<T as pallet_evm::Config>::Runner as pallet_evm::Runner<T>>::Error: Debug,
		Result<pallet_ethereum::RawOrigin, <T as frame_system::Config>::RuntimeOrigin>:
			From<<T as frame_system::Config>::RuntimeOrigin>,
		<T as frame_system::Config>::RuntimeOrigin: From<pallet_ethereum::RawOrigin>,
	{
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			if let Err(e) = Self::execute_contract_call() {
				// Log the error but don't fail the block
				log::error!("Failed to execute contract call: {:?}", e);
			}

			Weight::from_parts(0, 0)
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn test(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			Self::deposit_event(Event::Done);

			Ok(().into())
		}
	}

	// #[pallet::validate_unsigned]
	// impl<T: Config> ValidateUnsigned for Pallet<T> {
	// 	type Call = Call<T>;

	// 	fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
	// 		match call {
	// 			Call::test {} => ValidTransaction::with_tag_prefix("EvmAutoPilotTest")
	// 				.priority(TransactionPriority::MAX)
	// 				.and_provides("EvmAutoPilotTest")
	// 				.propagate(true)
	// 				.build(),
	// 			_ => InvalidTransaction::Call.into(),
	// 		}
	// 	}
	// }

	impl<T: Config> Pallet<T>
	where
		<<T as pallet_evm::Config>::Runner as pallet_evm::Runner<T>>::Error: Debug,
		Result<pallet_ethereum::RawOrigin, <T as frame_system::Config>::RuntimeOrigin>:
			From<<T as frame_system::Config>::RuntimeOrigin>,
		<T as frame_system::Config>::RuntimeOrigin: From<pallet_ethereum::RawOrigin>,
	{
		/// Execute a contract call
		fn execute_contract_call() -> DispatchResult
		where
			<<T as pallet_evm::Config>::Runner as pallet_evm::Runner<T>>::Error: Debug,
		{
			let source = hex::decode("3Cd0A705a2DC65e5b1E1205896BaA2be8A07c6e0")
				.map_err(|_| Error::<T>::InvalidHex)?;
			let target = hex::decode("0000000000000000000000000000000000000400")
				.map_err(|_| Error::<T>::InvalidHex)?;
			let data = hex::decode("49df6eb3000000000000000000000000f24ff3a9cf04c71dbc94d0b566f7a27b94566cac00000000000000000000000000000000000000000000003635c9adc5dea00000000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000a")
				.map_err(|_| Error::<T>::InvalidHex)?;

			// Create a new config with estimate mode enabled
			let mut config = <T as pallet_evm::Config>::config().clone();
			config.estimate = true;

			let result = <T as pallet_evm::Config>::Runner::call(
				H160::from_slice(&source),
				H160::from_slice(&target),
				data.clone(),
				U256::zero(),
				1000000,
				Some(U256::from(1000) * U256::from(10).pow(U256::from(9))), // 1000 Gwei
				None,
				None,
				vec![],
				true,
				true,
				None,
				None,
				&config,
			)
			.map_err(|e| {
				log::error!("EVM call failed: {:?}", e);
				Error::<T>::ContractCallFailed
			})?;

			match result.exit_reason {
				ExitReason::Succeed(_) => {
					// Create a transaction that will be recorded in history
					let transaction =
						pallet_ethereum::Transaction::Legacy(ethereum::LegacyTransaction {
							nonce: U256::zero(),
							gas_price: U256::from(1000) * U256::from(10).pow(U256::from(9)), // 1000 Gwei
							gas_limit: U256::from(1000000),
							action: pallet_ethereum::TransactionAction::Call(H160::from_slice(
								&target,
							)),
							value: U256::zero(),
							input: data,
							signature: ethereum::TransactionSignature::new(
								27,                         // v: 27 for valid signature
								H256::from_slice(&[1; 32]), // r: non-zero value
								H256::from_slice(&[2; 32]), // s: non-zero value
							)
							.unwrap(),
						});

					// Execute the transaction with the source account as the sender
					pallet_ethereum::Pallet::<T>::transact_unsigned(
						RawOrigin::None.into(),
						H160::from_slice(&source),
						transaction,
					)
					.map_err(|e| {
						log::error!("Transaction failed: {:?}", e);
						Error::<T>::ContractCallFailed
					})?;
				},
				_ => {
					log::error!("EVM call failed: {:?}", result.exit_reason);
					return Err(Error::<T>::ContractCallFailed.into());
				},
			}

			Ok(())
		}
	}
}
