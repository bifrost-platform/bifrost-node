use miniscript::bitcoin::Network;

use crate::{BoundedBitcoinAddress, MigrationSequence};

pub trait PoolManager<AccountId> {
	/// Get the refund address of the given user.
	fn get_refund_address(who: &AccountId) -> Option<BoundedBitcoinAddress>;

	/// Get the system vault address.
	fn get_system_vault() -> Option<BoundedBitcoinAddress>;

	/// Get the Bitcoin network of the current runtime.
	fn get_bitcoin_network() -> Network;

	/// Get the Bitcoin chain ID.
	fn get_bitcoin_chain_id() -> u32;

	/// Get current service state.
	fn get_service_state() -> MigrationSequence;
}