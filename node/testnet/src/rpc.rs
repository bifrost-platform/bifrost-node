//! A collection of node-specific RPC methods.
//! Substrate provides the `sc-rpc` crate, which defines the core RPC layer
//! used by Substrate nodes. This file extends those RPC definitions with
//! capabilities that are specific to this project's runtime configuration.

#![warn(missing_docs)]

use std::sync::Arc;

use bifrost_common_node::{
	cli_opt::EthApi as EthApiCmd,
	rpc::{FullDeps, GrandpaDeps},
};
use bifrost_testnet_runtime::{opaque::Block, AccountId, Balance, Index};

use sp_api::ProvideRuntimeApi;
use sp_block_builder::BlockBuilder;
use sp_blockchain::{
	Backend as BlockchainBackend, Error as BlockChainError, HeaderBackend, HeaderMetadata,
};
use sp_consensus::SelectChain;
use sp_runtime::traits::BlakeTwo256;

use sc_client_api::backend::{Backend, StateBackend, StorageProvider};
pub use sc_client_api::{AuxStore, BlockOf, BlockchainEvents};
use sc_consensus_manual_seal::rpc::{ManualSeal, ManualSealApi};
use sc_finality_grandpa_rpc::GrandpaRpcHandler;
pub use sc_rpc_api::DenyUnsafe;
use sc_transaction_pool::ChainApi;
use sc_transaction_pool_api::TransactionPool;

/// Instantiate all full RPC extensions.
pub fn create_full<C, P, BE, SC, A>(
	deps: FullDeps<C, P, BE, SC, A>,
) -> jsonrpc_core::IoHandler<sc_rpc_api::Metadata>
where
	BE: Backend<Block> + Send + Sync + 'static,
	BE::State: StateBackend<BlakeTwo256>,
	BE::Blockchain: BlockchainBackend<Block>,
	C: ProvideRuntimeApi<Block>,
	C: BlockchainEvents<Block>,
	C: StorageProvider<Block, BE>,
	C: HeaderBackend<Block> + HeaderMetadata<Block, Error = BlockChainError> + 'static,
	C: AuxStore,
	C: StorageProvider<Block, BE>,
	C: Send + Sync + 'static,
	C::Api: substrate_frame_rpc_system::AccountNonceApi<Block, AccountId, Index>,
	C::Api: pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>,
	C::Api: BlockBuilder<Block>,
	C::Api: fp_rpc::EthereumRuntimeRPCApi<Block>,
	C::Api: fp_rpc::ConvertTransactionRuntimeApi<Block>,
	C::Api: fp_rpc::TxPoolRuntimeApi<Block>,
	P: TransactionPool<Block = Block> + 'static,
	A: ChainApi<Block = Block> + 'static,
	SC: SelectChain<Block> + 'static,
{
	use fc_rpc::{
		EthApi, EthApiServer, EthFilterApi, EthFilterApiServer, NetApi, NetApiServer, TxPoolApi,
		TxPoolApiServer, Web3Api, Web3ApiServer,
	};
	use pallet_transaction_payment_rpc::{TransactionPayment, TransactionPaymentApi};
	use substrate_frame_rpc_system::{FullSystem, SystemApi};

	let mut io = jsonrpc_core::IoHandler::default();

	let FullDeps {
		client,
		pool,
		select_chain: _,
		chain_spec: _,
		deny_unsafe,
		graph,
		network,
		filter_pool,
		ethapi_cmd,
		frontier_backend,
		backend: _,
		is_authority,
		overrides,
		block_data_cache,
		fee_history_limit,
		fee_history_cache,
		grandpa,
		command_sink,
		max_past_logs,
		max_logs_request_duration,
	} = deps;

	let GrandpaDeps {
		shared_voter_state,
		shared_authority_set,
		justification_stream,
		subscription_executor,
		finality_provider,
	} = grandpa;

	io.extend_with(SystemApi::to_delegate(FullSystem::new(
		client.clone(),
		pool.clone(),
		deny_unsafe,
	)));

	io.extend_with(TransactionPaymentApi::to_delegate(TransactionPayment::new(client.clone())));

	io.extend_with(sc_finality_grandpa_rpc::GrandpaApi::to_delegate(GrandpaRpcHandler::new(
		shared_authority_set.clone(),
		shared_voter_state,
		justification_stream,
		subscription_executor,
		finality_provider,
	)));

	io.extend_with(EthFilterApiServer::to_delegate(EthFilterApi::new(
		client.clone(),
		frontier_backend.clone(),
		filter_pool,
		500_usize,
		max_past_logs,
		max_logs_request_duration,
		block_data_cache.clone(),
	)));

	io.extend_with(NetApiServer::to_delegate(NetApi::new(client.clone(), network.clone(), true)));

	io.extend_with(Web3ApiServer::to_delegate(Web3Api::new(client.clone())));

	if ethapi_cmd.contains(&EthApiCmd::Txpool) {
		io.extend_with(TxPoolApiServer::to_delegate(TxPoolApi::new(
			Arc::clone(&client),
			graph.clone(),
		)));
	}

	// Nor any signers
	let signers = Vec::new();

	io.extend_with(EthApiServer::to_delegate(EthApi::new(
		client.clone(),
		pool.clone(),
		graph,
		Some(bifrost_testnet_runtime::TransactionConverter),
		network.clone(),
		signers,
		overrides.clone(),
		frontier_backend.clone(),
		is_authority,
		max_past_logs,
		max_logs_request_duration,
		block_data_cache.clone(),
		fee_history_limit,
		fee_history_cache,
		10,
	)));

	if let Some(command_sink) = command_sink {
		io.extend_with(ManualSealApi::to_delegate(ManualSeal::new(command_sink)));
	};

	io
}
