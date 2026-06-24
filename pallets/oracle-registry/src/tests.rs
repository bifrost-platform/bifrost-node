use crate::{mock::*, OracleManagerContract};
use bp_oracle::traits::{oracle_manager_abi, OracleRegistryManager};
use sp_core::{H160, H256};

/// Builds the bytecode for a minimal EVM contract that ignores its input and
/// always returns `return_value` as a `bytes32`.
///
/// Bytecode layout (41 bytes):
///   PUSH32 <return_value>   // push 32-byte value onto stack
///   PUSH1  0x00             // memory offset = 0
///   MSTORE                  // memory[0..32] = return_value
///   PUSH1  0x20             // return size = 32
///   PUSH1  0x00             // return offset = 0
///   RETURN
fn dummy_oracle_manager_bytecode(return_value: H256) -> Vec<u8> {
	let mut bytecode = Vec::with_capacity(41);
	bytecode.push(0x7f); // PUSH32
	bytecode.extend_from_slice(return_value.as_bytes());
	bytecode.extend_from_slice(&[
		0x60, 0x00, // PUSH1 0x00
		0x52, // MSTORE
		0x60, 0x20, // PUSH1 0x20
		0x60, 0x00, // PUSH1 0x00
		0xf3, // RETURN
	]);
	bytecode
}

/// Deploys the dummy oracle manager contract at `address` and registers it as
/// the oracle manager in storage.
fn setup_oracle_manager(address: H160, return_value: H256) {
	let bytecode = dummy_oracle_manager_bytecode(return_value);
	pallet_evm::AccountCodes::<Test>::insert(address, bytecode);
	OracleManagerContract::<Test>::put(address);
}

#[test]
fn get_latest_oracle_data_returns_value_from_contract() {
	new_test_ext().execute_with(|| {
		let contract = H160::from_low_u64_be(0x1234);
		let oracle_id = H256::from_low_u64_be(1);
		let expected = H256::from_low_u64_be(100_000_000); // 1 USD with 8 decimals

		setup_oracle_manager(contract, expected);

		let result = crate::pallet::Pallet::<Test>::get_latest_oracle_data(oracle_id);
		assert_eq!(result, Some(expected));
	});
}

#[test]
fn get_latest_oracle_data_returns_none_when_contract_not_set() {
	new_test_ext().execute_with(|| {
		let oracle_id = H256::from_low_u64_be(1);

		// OracleManagerContract is not set
		let result = crate::pallet::Pallet::<Test>::get_latest_oracle_data(oracle_id);
		assert_eq!(result, None);
	});
}

#[test]
fn get_latest_oracle_data_returns_none_when_contract_has_no_code() {
	new_test_ext().execute_with(|| {
		let contract = H160::from_low_u64_be(0x5678);
		let oracle_id = H256::from_low_u64_be(1);

		// Register the address but deploy no bytecode
		OracleManagerContract::<Test>::put(contract);

		let result = crate::pallet::Pallet::<Test>::get_latest_oracle_data(oracle_id);
		assert_eq!(result, None);
	});
}

#[test]
fn encode_calldata_has_correct_selector_and_oracle_id() {
	let oracle_id = H256::from_low_u64_be(42);
	let calldata = oracle_manager_abi::encode_calldata(oracle_id);

	assert_eq!(&calldata[..4], &oracle_manager_abi::LAST_ORACLE_DATA_SELECTOR);
	assert_eq!(&calldata[4..], oracle_id.as_bytes());
}

#[test]
fn decode_return_recovers_value() {
	let expected = H256::from_low_u64_be(99);
	let mut data = [0u8; 32];
	data.copy_from_slice(expected.as_bytes());

	assert_eq!(oracle_manager_abi::decode_return(&data), Some(expected));
}

#[test]
fn decode_return_returns_none_for_short_data() {
	assert_eq!(oracle_manager_abi::decode_return(&[0u8; 31]), None);
	assert_eq!(oracle_manager_abi::decode_return(&[]), None);
}
