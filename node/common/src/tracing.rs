#![allow(missing_docs)]

use crate::{
	cli_opt::{EthApi as EthApiCmd, RpcConfig},
	rpc::SpawnTasksParams,
};

use fc_rpc_debug::{DebugHandler, DebugRequester};
use fc_rpc_trace::{CacheRequester as TraceFilterCacheRequester, CacheTask};
use fp_rpc::EthereumRuntimeRPCApi;
use sc_client_api::{backend::Backend, BlockOf, BlockchainEvents, StateBackend, StorageProvider};

use sp_api::{BlockT, HeaderT, ProvideRuntimeApi};
use sp_block_builder::BlockBuilder;
use sp_blockchain::{Error as BlockChainError, HeaderBackend, HeaderMetadata};
use sp_core::H256;
use sp_runtime::traits::BlakeTwo256;

use std::{sync::Arc, time::Duration};
use tokio::sync::Semaphore;

#[derive(Clone)]
pub struct RpcRequesters {
	pub debug: Option<DebugRequester>,
	pub trace: Option<TraceFilterCacheRequester>,
}

// Spawn the tasks that are required to run a BIFROST tracing node.
pub fn spawn_tracing_tasks<B, C, BE>(
	rpc_config: &RpcConfig,
	params: SpawnTasksParams<B, C, BE>,
) -> RpcRequesters
where
	C: ProvideRuntimeApi<B> + BlockOf,
	C: StorageProvider<B, BE>,
	C: HeaderBackend<B> + HeaderMetadata<B, Error = BlockChainError> + 'static,
	C: BlockchainEvents<B>,
	C: Send + Sync + 'static,
	C::Api: EthereumRuntimeRPCApi<B> + fp_rpc_debug::DebugRuntimeApi<B>,
	C::Api: BlockBuilder<B>,
	B: BlockT<Hash = H256> + Send + Sync + 'static,
	B::Header: HeaderT<Number = u32>,
	BE: Backend<B> + 'static,
	BE::State: StateBackend<BlakeTwo256>,
{
	let permit_pool = Arc::new(Semaphore::new(rpc_config.ethapi_max_permits as usize));

	let (trace_filter_task, trace_filter_requester) =
		if rpc_config.ethapi.contains(&EthApiCmd::Trace) {
			let (trace_filter_task, trace_filter_requester) = CacheTask::create(
				Arc::clone(&params.client),
				Arc::clone(&params.substrate_backend),
				Duration::from_secs(rpc_config.ethapi_trace_cache_duration),
				Arc::clone(&permit_pool),
				Arc::clone(&params.overrides),
			);
			(Some(trace_filter_task), Some(trace_filter_requester))
		} else {
			(None, None)
		};

	let (debug_task, debug_requester) = if rpc_config.ethapi.contains(&EthApiCmd::Debug) {
		let (debug_task, debug_requester) = DebugHandler::task(
			Arc::clone(&params.client),
			Arc::clone(&params.substrate_backend),
			Arc::clone(&params.frontier_backend),
			Arc::clone(&permit_pool),
			Arc::clone(&params.overrides),
			rpc_config.tracing_raw_max_memory_usage,
		);
		(Some(debug_task), Some(debug_requester))
	} else {
		(None, None)
	};

	// `trace_filter` cache task. Essential.
	// Proxies rpc requests to it's handler.
	if let Some(trace_filter_task) = trace_filter_task {
		params.task_manager.spawn_essential_handle().spawn(
			"trace-filter-cache",
			Some("eth-tracing"),
			trace_filter_task,
		);
	}

	// `debug` task if enabled. Essential.
	// Proxies rpc requests to it's handler.
	if let Some(debug_task) = debug_task {
		params.task_manager.spawn_essential_handle().spawn(
			"ethapi-debug",
			Some("eth-tracing"),
			debug_task,
		);
	}

	RpcRequesters { debug: debug_requester, trace: trace_filter_requester }
}
