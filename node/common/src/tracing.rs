#![allow(missing_docs)]

use bp_core::*;

use fc_rpc::{
	CacheRequester as TraceFilterCacheRequester, Debug, DebugRequester, DebugServer, Trace,
	TraceServer,
};
use sc_client_api::{backend::Backend, AuxStore, BlockchainEvents, StateBackend, StorageProvider};

use sp_api::ProvideRuntimeApi;
use sp_blockchain::{
	Backend as BlockchainBackend, Error as BlockChainError, HeaderBackend, HeaderMetadata,
};
use sp_runtime::traits::BlakeTwo256;

use std::sync::Arc;

#[derive(Clone)]
pub struct RpcRequesters {
	pub debug: Option<DebugRequester>,
	pub trace: Option<TraceFilterCacheRequester>,
}

pub fn extend_with_tracing<C, BE>(
	client: Arc<C>,
	requesters: RpcRequesters,
	trace_filter_max_count: u32,
	io: &mut jsonrpc_core::IoHandler<sc_rpc::Metadata>,
) where
	BE: Backend<Block> + 'static,
	BE::State: StateBackend<BlakeTwo256>,
	BE::Blockchain: BlockchainBackend<Block>,
	C: ProvideRuntimeApi<Block> + StorageProvider<Block, BE> + AuxStore,
	C: BlockchainEvents<Block>,
	C: HeaderBackend<Block> + HeaderMetadata<Block, Error = BlockChainError> + 'static,
	C: Send + Sync + 'static,
	C::Api: fp_rpc_debug::DebugRuntimeApi<Block>,
{
	if let Some(trace_filter_requester) = requesters.trace {
		io.extend_with(TraceServer::to_delegate(Trace::new(
			client,
			trace_filter_requester,
			trace_filter_max_count,
		)));
	}

	if let Some(debug_requester) = requesters.debug {
		io.extend_with(DebugServer::to_delegate(Debug::new(debug_requester)));
	}
}
