use frame_system::pallet_prelude::BlockNumberFor;
use miniscript::{
	bitcoin::{Network, PublicKey},
	Descriptor,
};
use sp_core::H256;
use sp_runtime::{transaction_validity::TransactionValidityError, DispatchError};
use sp_std::vec::Vec;

use crate::{
	blaze::{FailureReason, ScoredUtxo, SelectionStrategy, UtxoInfoWithSize},
	BoundedBitcoinAddress, MigrationSequence, Psbt, UnboundedBytes,
};

pub trait PoolManager<AccountId> {
	/// Get the refund address of the given user.
	fn get_refund_address(who: &AccountId) -> Option<BoundedBitcoinAddress>;

	/// Get the vault address of the given user.
	fn get_vault_address(who: &AccountId) -> Option<BoundedBitcoinAddress>;

	/// Get the descriptor of the given vault address.
	fn get_bonded_descriptor(who: &BoundedBitcoinAddress) -> Option<Descriptor<PublicKey>>;

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

	/// Process the pending set refunds.
	fn process_set_refunds();
}

pub trait SocketQueueManager<AccountId> {
	/// Check if the system is ready for migrate.
	fn is_ready_for_migrate() -> bool;

	/// Verify if the `authority_id` is valid.
	fn verify_authority(authority_id: &AccountId) -> Result<(), TransactionValidityError>;

	/// Replace an authority.
	fn replace_authority(old: &AccountId, new: &AccountId);

	/// Get the maximum fee rate that can be used for a transaction.
	fn get_max_fee_rate() -> u64;
}

pub trait SocketVerifier<AccountId> {
	/// Verify a Socket message whether it is valid.
	fn verify_socket_message(msg: &UnboundedBytes) -> Result<(), DispatchError>;
}

pub trait BlazeManager<T: frame_system::Config> {
	/// Check if BLAZE is activated.
	fn is_activated() -> bool;

	/// Get Utxos
	fn get_utxos() -> Vec<UtxoInfoWithSize>;

	/// Clear the utxos.
	fn clear_utxos();

	/// Lock the utxos.
	fn lock_utxos(txid: &H256, inputs: &Vec<UtxoInfoWithSize>) -> Result<(), DispatchError>;

	/// Unlock the utxos.
	fn unlock_utxos(txid: &H256) -> Result<(), DispatchError>;

	/// Extract the utxos from the given PSBT.
	fn extract_utxos_from_psbt(psbt: &Psbt) -> Result<Vec<UtxoInfoWithSize>, DispatchError>;

	/// Read the outbound pool.
	fn get_outbound_pool() -> Vec<UnboundedBytes>;

	/// Clear the outbound pool.
	fn clear_outbound_pool(targets: Vec<UnboundedBytes>);

	/// Take the executed requests.
	fn take_executed_requests() -> Vec<H256>;

	/// Try to finalize the fee rate.
	fn try_fee_rate_finalization(n: BlockNumberFor<T>) -> Option<(u64, u64)>;

	/// Select utxos for given target.
	fn select_coins(
		pool: Vec<ScoredUtxo>,
		target: u64,
		cost_of_change: u64,
		max_selection_weight: u64,
		max_tries: usize,
		change_target: u64,
	) -> Option<(Vec<UtxoInfoWithSize>, SelectionStrategy)>;

	/// Check the tolerance counter. If it exceeds the threshold, BLAZE will be deactivated.
	fn try_deactivation(reason: FailureReason);
}
