use crate::pallet::{BlockNumberFor, DispatchResult};

use frame_system::RawOrigin;
use pallet_evm::{ExitReason, Runner};
use sp_core::{H160, H256, U256};
use sp_runtime::SaturatedConversion;
use sp_std::{fmt::Debug, vec, vec::Vec};

use super::pallet::*;

impl<T: Config> Pallet<T>
where
	<<T as pallet_evm::Config>::Runner as pallet_evm::Runner<T>>::Error: Debug,
	Result<pallet_ethereum::RawOrigin, <T as frame_system::Config>::RuntimeOrigin>:
		From<<T as frame_system::Config>::RuntimeOrigin>,
	<T as frame_system::Config>::RuntimeOrigin: From<pallet_ethereum::RawOrigin>,
	<T as frame_system::Config>::AccountId: From<H160>,
{
	/// Execute a contract call
	pub fn execute_contract_call(n: BlockNumberFor<T>) -> DispatchResult
	where
		<<T as pallet_evm::Config>::Runner as pallet_evm::Runner<T>>::Error: Debug,
	{
		let source = hex::decode("3Cd0A705a2DC65e5b1E1205896BaA2be8A07c6e0")
			.map_err(|_| Error::<T>::InvalidHex)?;
		// 3Cd0A705a2DC65e5b1E1205896BaA2be8A07c6e0 (baltathar)
		// DA49B5eA4BD06d72D1909c46332Dd417841D28E7 (test#2)

		// 0000000000000000000000000000000000000400
		// 3DACFBA2a2a7526E4397ff691df1873C50eFB542 (test#1)
		let target = hex::decode("3DACFBA2a2a7526E4397ff691df1873C50eFB542")
			.map_err(|_| Error::<T>::InvalidHex)?;
		// let input = hex::decode("49df6eb3000000000000000000000000f24ff3a9cf04c71dbc94d0b566f7a27b94566cac00000000000000000000000000000000000000000000003635c9adc5dea00000000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000a")
		// 	.map_err(|_| Error::<T>::InvalidHex)?;
		let input = vec![];
		let value = U256::from(10).pow(U256::from(18)); // 1 BFC
												  // let value = U256::zero();

		// Get the nonce from the source account (using system pallet)
		let source_account: T::AccountId = H160::from_slice(&source).into();
		let nonce = frame_system::Pallet::<T>::account_nonce(&source_account);
		let nonce_u256 = U256::from(nonce.saturated_into::<u64>());

		let mut config = <T as pallet_evm::Config>::config().clone();
		config.estimate = true;

		// First, estimate the call
		let estimate_result = <T as pallet_evm::Config>::Runner::call(
			H160::from_slice(&source),
			H160::from_slice(&target),
			input.clone(),
			value,
			1000000,
			Some(U256::from(1000) * U256::from(10).pow(U256::from(9))), // 1000 Gwei
			None,
			Some(nonce_u256),
			vec![],
			false,
			true,
			None,
			None,
			&config,
		)
		.map_err(|e| {
			log::error!("EVM estimation failed: {:?}", e);
			Error::<T>::EstimationFailed
		})?;

		// Check if the estimation was successful
		match estimate_result.exit_reason {
			ExitReason::Succeed(_) => {
				log::info!(
					"Estimation succeeded, proceeding with transaction: {:?}",
					estimate_result
				);

				// Reset the nonce to the original value
				// The previous call will have incremented it (Since the context is the same)
				frame_system::Pallet::<T>::set_account_nonce(source_account, nonce);

				// Create a transaction that will be recorded in history
				let transaction =
					pallet_ethereum::Transaction::Legacy(ethereum::LegacyTransaction {
						nonce: U256::from(nonce_u256),
						gas_price: U256::from(1000) * U256::from(10).pow(U256::from(9)), // 1000 Gwei
						gas_limit: U256::from(1000000),
						action: pallet_ethereum::TransactionAction::Call(H160::from_slice(&target)),
						value,
						input,
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
				log::error!("EVM estimation failed: {:?}", estimate_result);
				return Err(Error::<T>::EstimationFailed.into());
			},
		}

		Ok(())
	}
}
