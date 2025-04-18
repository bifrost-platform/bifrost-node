use miniscript::bitcoin::Network;
use sp_core::H256;
use sp_runtime::{transaction_validity::TransactionValidityError, DispatchError};

use crate::{BoundedBitcoinAddress, MigrationSequence, UnboundedBytes};

pub trait PoolManager<AccountId> {
	/// Get the refund address of the given user.
	fn get_refund_address(who: &AccountId) -> Option<BoundedBitcoinAddress>;

	/// Get the vault address of the given user.
	fn get_vault_address(who: &AccountId) -> Option<BoundedBitcoinAddress>;

	/// Get the system vault address.
	fn get_system_vault(round: u32) -> Option<BoundedBitcoinAddress>;

	/// Get the Bitcoin network of the current runtime.
	fn get_bitcoin_network() -> Network;

	/// Get the Bitcoin chain ID.
	fn get_bitcoin_chain_id() -> u32;

	/// Get current service state.
	fn get_service_state() -> MigrationSequence;

	/// Get the current pool round.
	fn get_current_round() -> u32;

	/// Add a migration transaction.
	fn add_migration_tx(txid: H256);

	/// Remove a migration transaction.
	fn remove_migration_tx(txid: H256);

	/// Execute a migration transaction.
	fn execute_migration_tx(txid: H256);

	/// Replace an authority.
	fn replace_authority(old: &AccountId, new: &AccountId);
}

pub trait SocketQueueManager<AccountId> {
	/// Check if the system is ready for migrate.
	fn is_ready_for_migrate() -> bool;

	/// Verify if the `authority_id` is valid.
	fn verify_authority(authority_id: &AccountId) -> Result<(), TransactionValidityError>;

	/// Replace an authority.
	fn replace_authority(old: &AccountId, new: &AccountId);
}

pub trait SocketVerifier<AccountId> {
	fn verify_socket_message(msg: &UnboundedBytes) -> Result<(), DispatchError>;
}
