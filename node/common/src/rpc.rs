use crate::cli_opt::EthApi as EthApiCmd;

use std::{collections::BTreeMap, sync::Arc};

use fc_rpc::{EthBlockDataCacheTask, StorageOverride};
use fc_rpc_core::types::{FeeHistoryCache, FilterPool};
use sc_client_api::{backend::Backend, StorageProvider};
use sc_consensus_grandpa::{
	FinalityProofProvider, GrandpaJustificationStream, SharedAuthoritySet, SharedVoterState,
};
use sc_consensus_manual_seal::EngineCommand;
use sc_network::service::traits::NetworkService;
use sc_network_sync::SyncingService;
use sc_rpc::SubscriptionTaskExecutor;
use sc_service::TaskManager;

use bp_core::{BlockNumber, Hash, Header};
use sp_core::H256;
use sp_runtime::{generic, traits::Block as BlockT, OpaqueExtrinsic as UncheckedExtrinsic};

pub type Block = generic::Block<Header, UncheckedExtrinsic>;

pub struct DefaultEthConfig<C, BE>(std::marker::PhantomData<(C, BE)>);

impl<C, BE> fc_rpc::EthConfig<Block, C> for DefaultEthConfig<C, BE>
where
	C: StorageProvider<Block, BE> + Sync + Send + 'static,
	BE: Backend<Block> + 'static,
{
	type EstimateGasAdapter = ();
	type RuntimeStorageOverride =
		fc_rpc::frontier_backend_client::SystemAccountId20StorageOverride<Block, C, BE>;
}

/// Extra dependencies for GRANDPA
pub struct GrandpaDeps<B> {
	/// Voting round info.
	pub shared_voter_state: SharedVoterState,
	/// Authority set info.
	pub shared_authority_set: SharedAuthoritySet<Hash, BlockNumber>,
	/// Receives notifications about justification events from Grandpa.
	pub justification_stream: GrandpaJustificationStream<Block>,
	/// Executor to drive the subscription manager in the Grandpa RPC handler.
	pub subscription_executor: SubscriptionTaskExecutor,
	/// Finality proof provider.
	pub finality_provider: Arc<FinalityProofProvider<B, Block>>,
}

/// Full client dependencies.
pub struct FullDevDeps<C, P, BE, SC, CIDP> {
	/// Client version.
	pub client_version: String,
	/// The client instance to use.
	pub client: Arc<C>,
	/// Transaction pool instance.
	pub pool: Arc<P>,
	/// The SelectChain Strategy
	pub select_chain: SC,
	/// A copy of the chain spec.
	pub chain_spec: Box<dyn sc_chain_spec::ChainSpec>,
	/// Graph pool instance.
	pub graph: Arc<P>,
	/// GRANDPA specific dependencies.
	pub grandpa: GrandpaDeps<BE>,
	/// The Node authority flag
	pub is_authority: bool,
	/// Network service
	pub network: Arc<dyn NetworkService>,
	/// EthFilterApi pool.
	pub filter_pool: FilterPool,
	/// List of optional RPC extensions.
	pub ethapi_cmd: Vec<EthApiCmd>,
	/// Frontier backend.
	pub frontier_backend: Arc<dyn fc_api::Backend<Block> + Send + Sync>,
	/// Backend.
	pub backend: Arc<BE>,
	/// Maximum fee history cache size.
	pub fee_history_limit: u64,
	/// Fee history cache.
	pub fee_history_cache: FeeHistoryCache,
	/// Ethereum data access overrides.
	pub overrides: Arc<dyn StorageOverride<Block>>,
	/// Cache for Ethereum block data.
	pub block_data_cache: Arc<EthBlockDataCacheTask<Block>>,
	/// Manual seal command sink
	pub command_sink: Option<futures::channel::mpsc::Sender<EngineCommand<Hash>>>,
	/// Maximum number of logs in one query.
	pub max_past_logs: u32,
	/// Timeout for eth logs query in seconds. (default 10)
	pub logs_request_timeout: u64,
	/// Mandated parent hashes for a given block hash.
	pub forced_parent_hashes: Option<BTreeMap<H256, H256>>,
	/// Chain syncing service
	pub sync_service: Arc<SyncingService<Block>>,
	/// Something that can create the inherent data providers for pending state
	pub pending_create_inherent_data_providers: CIDP,
}

/// Mainnet/Testnet client dependencies.
pub struct FullDeps<C, P, BE, SC, CIDP> {
	/// Client version.
	pub client_version: String,
	/// The client instance to use.
	pub client: Arc<C>,
	/// Transaction pool instance.
	pub pool: Arc<P>,
	/// The SelectChain Strategy
	pub select_chain: SC,
	/// A copy of the chain spec.
	pub chain_spec: Box<dyn sc_chain_spec::ChainSpec>,
	/// Graph pool instance.
	pub graph: Arc<P>,
	/// GRANDPA specific dependencies.
	pub grandpa: GrandpaDeps<BE>,
	/// The Node authority flag
	pub is_authority: bool,
	/// Network service
	pub network: Arc<dyn NetworkService>,
	/// EthFilterApi pool.
	pub filter_pool: FilterPool,
	/// List of optional RPC extensions.
	pub ethapi_cmd: Vec<EthApiCmd>,
	/// Frontier backend.
	pub frontier_backend: Arc<dyn fc_api::Backend<Block> + Send + Sync>,
	/// Backend.
	pub backend: Arc<BE>,
	/// Maximum fee history cache size.
	pub fee_history_limit: u64,
	/// Fee history cache.
	pub fee_history_cache: FeeHistoryCache,
	/// Ethereum data access overrides.
	pub overrides: Arc<dyn StorageOverride<Block>>,
	/// Cache for Ethereum block data.
	pub block_data_cache: Arc<EthBlockDataCacheTask<Block>>,
	/// Maximum number of logs in one query.
	pub max_past_logs: u32,
	/// Timeout for eth logs query in seconds. (default 10)
	pub logs_request_timeout: u64,
	/// Mandated parent hashes for a given block hash.
	pub forced_parent_hashes: Option<BTreeMap<H256, H256>>,
	/// Chain syncing service
	pub sync_service: Arc<SyncingService<Block>>,
	/// Something that can create the inherent data providers for pending state
	pub pending_create_inherent_data_providers: CIDP,
}

pub struct SpawnTasksParams<'a, B: BlockT, C, BE> {
	pub task_manager: &'a TaskManager,
	pub client: Arc<C>,
	pub substrate_backend: Arc<BE>,
	pub frontier_backend: Arc<fc_db::Backend<B, C>>,
	pub filter_pool: Option<FilterPool>,
	pub overrides: Arc<dyn StorageOverride<B>>,
	pub fee_history_limit: u64,
	pub fee_history_cache: FeeHistoryCache,
}

pub struct TracingConfig {
	pub tracing_requesters: crate::tracing::RpcRequesters,
	pub trace_filter_max_count: u32,
}
