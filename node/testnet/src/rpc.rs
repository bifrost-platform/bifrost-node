//! A collection of node-specific RPC methods.
//! Substrate provides the `sc-rpc` crate, which defines the core RPC layer
//! used by Substrate nodes. This file extends those RPC definitions with
//! capabilities that are specific to this project's runtime configuration.

#![warn(missing_docs)]

use jsonrpsee::RpcModule;
use std::sync::Arc;

use bifrost_common_node::{cli_opt::EthApi as EthApiCmd, rpc::TracingConfig};
use bifrost_testnet_runtime::{opaque::Block, AccountId, Balance, Index};

use sp_api::ProvideRuntimeApi;
use sp_block_builder::BlockBuilder;
use sp_blockchain::{
	Backend as BlockchainBackend, Error as BlockChainError, HeaderBackend, HeaderMetadata,
};
use sp_consensus::SelectChain;
use sp_runtime::traits::BlakeTwo256;

use bifrost_common_node::rpc::{FullDeps, GrandpaDeps};
use sc_client_api::backend::{Backend, StateBackend, StorageProvider};
pub use sc_client_api::{AuxStore, BlockOf, BlockchainEvents};
pub use sc_rpc_api::DenyUnsafe;
use sc_transaction_pool::ChainApi;
use sc_transaction_pool_api::TransactionPool;

/// Instantiate all full RPC extensions.
pub fn create_full<C, P, BE, SC, A>(
	deps: FullDeps<C, P, BE, SC, A>,
	maybe_tracing_config: Option<TracingConfig>,
) -> Result<RpcModule<()>, sc_service::Error>
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
	C::Api: fp_rpc_txpool::TxPoolRuntimeApi<Block>,
	P: TransactionPool<Block = Block> + 'static,
	A: ChainApi<Block = Block> + 'static,
	SC: SelectChain<Block> + 'static,
{
	use fc_rpc::{
		Eth, EthApiServer, EthFilter, EthFilterApiServer, EthPubSub, EthPubSubApiServer, Net,
		NetApiServer, Web3, Web3ApiServer,
	};
	use fc_rpc_debug::{Debug, DebugServer};
	use fc_rpc_trace::{Trace, TraceServer};
	use fc_rpc_txpool::{TxPool, TxPoolServer};
	use pallet_transaction_payment_rpc::{TransactionPayment, TransactionPaymentApiServer};
	use sc_consensus_grandpa_rpc::{Grandpa, GrandpaApiServer};
	use substrate_frame_rpc_system::{System, SystemApiServer};

	let mut io = RpcModule::new(());
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
		max_past_logs,
		logs_request_timeout,
	} = deps;

	let GrandpaDeps {
		shared_voter_state,
		shared_authority_set,
		justification_stream,
		subscription_executor,
		finality_provider,
	} = grandpa;

	io.merge(System::new(Arc::clone(&client), Arc::clone(&pool), deny_unsafe).into_rpc())
		.ok();
	io.merge(TransactionPayment::new(Arc::clone(&client)).into_rpc()).ok();

	io.merge(
		Grandpa::new(
			Arc::clone(&subscription_executor),
			shared_authority_set.clone(),
			shared_voter_state,
			justification_stream,
			finality_provider,
		)
		.into_rpc(),
	)
	.ok();

	io.merge(
		EthFilter::new(
			client.clone(),
			frontier_backend.clone(),
			filter_pool,
			500_usize, // max stored filters
			max_past_logs,
			logs_request_timeout,
			block_data_cache.clone(),
		)
		.into_rpc(),
	)
	.ok();

	io.merge(
		Net::new(
			Arc::clone(&client),
			network.clone(),
			// Whether to format the `peer_count` response as Hex (default) or not.
			true,
		)
		.into_rpc(),
	)
	.ok();

	io.merge(Web3::new(Arc::clone(&client)).into_rpc()).ok();

	io.merge(
		EthPubSub::new(
			Arc::clone(&pool),
			Arc::clone(&client),
			network.clone(),
			Arc::clone(&subscription_executor),
			Arc::clone(&overrides),
		)
		.into_rpc(),
	)
	.ok();

	if ethapi_cmd.contains(&EthApiCmd::Txpool) {
		io.merge(TxPool::new(Arc::clone(&client), graph.clone()).into_rpc()).ok();
	}

	// Nor any signers
	let signers = Vec::new();

	enum Never {}
	impl<T> fp_rpc::ConvertTransaction<T> for Never {
		fn convert_transaction(&self, _transaction: pallet_ethereum::Transaction) -> T {
			// The Never type is not instantiable, but this method requires the type to be
			// instantiated to be called (`&self` parameter), so if the code compiles we have the
			// guarantee that this function will never be called.
			unreachable!()
		}
	}
	let convert_transaction: Option<Never> = None;

	io.merge(
		Eth::new(
			Arc::clone(&client),
			Arc::clone(&pool),
			graph.clone(),
			convert_transaction,
			Arc::clone(&network),
			signers,
			Arc::clone(&overrides),
			Arc::clone(&frontier_backend),
			is_authority,
			Arc::clone(&block_data_cache),
			fee_history_cache,
			fee_history_limit,
			10,
		)
		.into_rpc(),
	)
	.ok();

	if let Some(tracing_config) = maybe_tracing_config {
		if let Some(trace_filter_requester) = tracing_config.tracing_requesters.trace {
			io.merge(
				Trace::new(client, trace_filter_requester, tracing_config.trace_filter_max_count)
					.into_rpc(),
			)
			.ok();
		}

		if let Some(debug_requester) = tracing_config.tracing_requesters.debug {
			io.merge(Debug::new(debug_requester).into_rpc()).ok();
		}
	}

	Ok(io)
}
