//! Service and ServiceFactory implementation. Specialized wrapper over substrate service.

use bp_core::*;
use fc_db::Backend;
use futures::StreamExt;
use jsonrpsee::RpcModule;
use sc_network_sync::SyncingService;
use sc_transaction_pool_api::OffchainTransactionPoolFactory;
use std::{collections::BTreeMap, sync::Arc, time::Duration};

use bifrost_common_node::{
	cli_opt::{EthApi as EthApiCmd, RpcConfig},
	rpc::{FullDeps, GrandpaDeps, SpawnTasksParams, TracingConfig},
	service::open_frontier_backend,
	tracing::{spawn_tracing_tasks, RpcRequesters},
};

use crate::rpc::create_full;

use fc_mapping_sync::{kv::MappingSyncWorker, SyncStrategy};
use fc_rpc::EthTask;
use fc_rpc_core::types::{FeeHistoryCache, FilterPool};

use sc_client_api::{BlockBackend, BlockchainEvents};
use sc_consensus_aura::{ImportQueueParams, SlotProportion, StartAuraParams};
pub use sc_executor::NativeElseWasmExecutor;
use sc_network::NetworkService;
use sc_rpc_api::DenyUnsafe;
use sc_service::{
	error::Error as ServiceError, Configuration, RpcHandlers, SpawnTaskHandle, TaskManager,
	WarpSyncParams,
};
use sc_telemetry::{Telemetry, TelemetryWorker};

use sp_api::NumberFor;
use sp_consensus_aura::sr25519::AuthorityPair as AuraPair;
use sp_runtime::traits::Block as BlockT;

/// The minimum period of blocks on which justifications will be
/// imported and generated.
const GRANDPA_JUSTIFICATION_PERIOD: u32 = 512;

/// Testnet runtime executor
pub mod testnet {
	pub use bifrost_testnet_runtime::RuntimeApi;

	pub struct ExecutorDispatch;
	impl sc_executor::NativeExecutionDispatch for ExecutorDispatch {
		#[cfg(feature = "runtime-benchmarks")]
		type ExtendHostFunctions = frame_benchmarking::benchmarking::HostFunctions;

		#[cfg(not(feature = "runtime-benchmarks"))]
		type ExtendHostFunctions = fp_ext::bifrost_ext::HostFunctions;

		fn dispatch(method: &str, data: &[u8]) -> Option<Vec<u8>> {
			bifrost_testnet_runtime::api::dispatch(method, data)
		}

		fn native_version() -> sc_executor::NativeVersion {
			bifrost_testnet_runtime::native_version()
		}
	}
}

/// The full client type definition.
type FullClient = sc_service::TFullClient<
	Block,
	testnet::RuntimeApi,
	NativeElseWasmExecutor<testnet::ExecutorDispatch>,
>;

/// The full backend type definition.
type FullBackend = sc_service::TFullBackend<Block>;

/// The full select chain type definition.
type FullSelectChain = sc_consensus::LongestChain<FullBackend, Block>;

/// The transaction pool type definition.
pub type TransactionPool = sc_transaction_pool::FullPool<Block, FullClient>;

/// Builds a new service for a full client.
pub fn new_full(config: Configuration, rpc_config: RpcConfig) -> Result<TaskManager, ServiceError> {
	new_full_base(config, rpc_config).map(|NewFullBase { task_manager, .. }| task_manager)
}

/// Result of [`new_full_base`].
pub struct NewFullBase {
	/// The task manager of the node.
	pub task_manager: TaskManager,
	/// The client instance of the node.
	pub client: Arc<FullClient>,
	/// The networking service of the node.
	pub network: Arc<NetworkService<Block, <Block as BlockT>::Hash>>,
	/// The transaction pool of the node.
	pub transaction_pool: Arc<TransactionPool>,
	/// The rpc handlers of the node.
	pub rpc_handlers: Option<RpcHandlers>,
}

/// Builder for rpc extensions handler
pub struct RpcExtensionsBuilder<'a> {
	pub task_manager: &'a TaskManager,
	pub spawn_handle: SpawnTaskHandle,

	pub justification_stream: sc_consensus_grandpa::GrandpaJustificationStream<Block>,
	pub shared_voter_state: sc_consensus_grandpa::SharedVoterState,
	pub shared_authority_set:
		sc_consensus_grandpa::SharedAuthoritySet<<Block as BlockT>::Hash, NumberFor<Block>>,

	pub client: Arc<FullClient>,
	pub backend: Arc<FullBackend>,
	pub select_chain: FullSelectChain,
	pub transaction_pool: Arc<TransactionPool>,
	pub network: Arc<NetworkService<Block, <Block as BlockT>::Hash>>,
	pub frontier_backend: fc_db::Backend<Block>,
	pub sync_service: Arc<SyncingService<Block>>,
}

pub fn new_partial(
	config: &Configuration,
	rpc_config: &RpcConfig,
) -> Result<
	sc_service::PartialComponents<
		FullClient,
		FullBackend,
		FullSelectChain,
		sc_consensus::DefaultImportQueue<Block>,
		sc_transaction_pool::FullPool<Block, FullClient>,
		(
			sc_consensus_grandpa::GrandpaBlockImport<
				FullBackend,
				Block,
				FullClient,
				FullSelectChain,
			>,
			sc_consensus_grandpa::LinkHalf<Block, FullClient, FullSelectChain>,
			fc_db::Backend<Block>,
			Option<Telemetry>,
		),
	>,
	ServiceError,
> {
	let telemetry = config
		.telemetry_endpoints
		.clone()
		.filter(|x| !x.is_empty())
		.map(|endpoints| -> Result<_, sc_telemetry::Error> {
			let worker = TelemetryWorker::new(16)?;
			let telemetry = worker.handle().new_telemetry(endpoints);
			Ok((worker, telemetry))
		})
		.transpose()?;

	let executor = sc_service::new_native_or_wasm_executor(&config);

	let (client, backend, keystore_container, task_manager) =
		sc_service::new_full_parts::<Block, testnet::RuntimeApi, _>(
			config,
			telemetry.as_ref().map(|(_, telemetry)| telemetry.handle()),
			executor,
		)?;
	let client = Arc::new(client);

	let telemetry = telemetry.map(|(worker, telemetry)| {
		task_manager.spawn_handle().spawn("telemetry", None, worker.run());
		telemetry
	});

	let select_chain = sc_consensus::LongestChain::new(backend.clone());

	let transaction_pool = sc_transaction_pool::BasicPool::new_full(
		config.transaction_pool.clone(),
		config.role.is_authority().into(),
		config.prometheus_registry(),
		task_manager.spawn_essential_handle(),
		client.clone(),
	);

	let frontier_backend = open_frontier_backend(client.clone(), config, &rpc_config)?;

	let (grandpa_block_import, grandpa_link) = sc_consensus_grandpa::block_import(
		client.clone(),
		GRANDPA_JUSTIFICATION_PERIOD,
		&(client.clone() as Arc<_>),
		select_chain.clone(),
		telemetry.as_ref().map(|x| x.handle()),
	)?;

	let slot_duration = sc_consensus_aura::slot_duration(&*client)?;

	let import_queue =
		sc_consensus_aura::import_queue::<AuraPair, _, _, _, _, _>(ImportQueueParams {
			block_import: grandpa_block_import.clone(),
			justification_import: Some(Box::new(grandpa_block_import.clone())),
			client: client.clone(),
			create_inherent_data_providers: move |_, ()| async move {
				let timestamp = sp_timestamp::InherentDataProvider::from_system_time();

				let slot =
					sp_consensus_aura::inherents::InherentDataProvider::from_timestamp_and_slot_duration(
						*timestamp,
						slot_duration,
					);

				Ok((slot, timestamp))
			},
			spawner: &task_manager.spawn_essential_handle(),
			registry: config.prometheus_registry(),
			check_for_equivocation: Default::default(),
			telemetry: telemetry.as_ref().map(|x| x.handle()),
			compatibility_mode: Default::default(),
		})?;

	Ok(sc_service::PartialComponents {
		client,
		backend,
		task_manager,
		import_queue,
		keystore_container,
		select_chain,
		transaction_pool,
		other: (grandpa_block_import, grandpa_link, frontier_backend, telemetry),
	})
}

/// Creates a full service from the configuration.
pub fn new_full_base(
	config: Configuration,
	rpc_config: RpcConfig,
) -> Result<NewFullBase, ServiceError> {
	let sc_service::PartialComponents {
		client,
		backend,
		mut task_manager,
		import_queue,
		keystore_container,
		select_chain,
		transaction_pool,
		other: (grandpa_block_import, grandpa_link, frontier_backend, mut telemetry),
	} = new_partial(&config, &rpc_config)?;

	let mut net_config = sc_network::config::FullNetworkConfiguration::new(&config.network);

	let shared_voter_state = sc_consensus_grandpa::SharedVoterState::empty();
	let grandpa_protocol_name = sc_consensus_grandpa::protocol_standard_name(
		&client.block_hash(0).ok().flatten().expect("Genesis block exists; qed"),
		&config.chain_spec,
	);

	net_config.add_notification_protocol(sc_consensus_grandpa::grandpa_peers_set_config(
		grandpa_protocol_name.clone(),
	));

	let warp_sync = Arc::new(sc_consensus_grandpa::warp_proof::NetworkProvider::new(
		backend.clone(),
		grandpa_link.shared_authority_set().clone(),
		Vec::default(),
	));

	let (network, system_rpc_tx, tx_handler_controller, network_starter, sync_service) =
		sc_service::build_network(sc_service::BuildNetworkParams {
			config: &config,
			net_config,
			client: client.clone(),
			transaction_pool: transaction_pool.clone(),
			spawn_handle: task_manager.spawn_handle(),
			import_queue,
			block_announce_validator_builder: None,
			warp_sync_params: Some(WarpSyncParams::WithProvider(warp_sync)),
		})?;

	if config.offchain_worker.enabled {
		task_manager.spawn_handle().spawn(
			"offchain-workers-runner",
			"offchain-worker",
			sc_offchain::OffchainWorkers::new(sc_offchain::OffchainWorkerOptions {
				runtime_api_provider: client.clone(),
				is_validator: config.role.is_authority(),
				keystore: Some(keystore_container.keystore()),
				offchain_db: backend.offchain_storage(),
				transaction_pool: Some(OffchainTransactionPoolFactory::new(
					transaction_pool.clone(),
				)),
				network_provider: network.clone(),
				enable_http_requests: true,
				custom_extensions: |_| vec![],
			})
			.run(client.clone(), task_manager.spawn_handle())
			.boxed(),
		);
	}

	let role = config.role.clone();
	let force_authoring = config.force_authoring;
	let backoff_authoring_blocks: Option<()> = None;
	let name = config.network.node_name.clone();
	let enable_grandpa = !config.disable_grandpa;
	let prometheus_registry = config.prometheus_registry().cloned();
	let is_authority = config.role.is_authority();

	let rpc_extensions_builder = build_rpc_extensions_builder(
		&config,
		rpc_config,
		RpcExtensionsBuilder {
			spawn_handle: task_manager.spawn_handle(),
			task_manager: &mut task_manager,
			justification_stream: grandpa_link.justification_stream().clone(),
			shared_authority_set: grandpa_link.shared_authority_set().clone(),
			shared_voter_state: shared_voter_state.clone(),
			client: client.clone(),
			backend: backend.clone(),
			select_chain: select_chain.clone(),
			transaction_pool: transaction_pool.clone(),
			network: network.clone(),
			frontier_backend: frontier_backend.clone(),
			sync_service: sync_service.clone(),
		},
	);

	let rpc_handlers = sc_service::spawn_tasks(sc_service::SpawnTasksParams {
		config,
		backend: backend.clone(),
		client: client.clone(),
		network: network.clone(),
		keystore: keystore_container.keystore(),
		rpc_builder: Box::new(rpc_extensions_builder),
		transaction_pool: transaction_pool.clone(),
		task_manager: &mut task_manager,
		system_rpc_tx,
		tx_handler_controller,
		sync_service: sync_service.clone(),
		telemetry: telemetry.as_mut(),
	})
	.ok();

	if is_authority {
		let proposer_factory = sc_basic_authorship::ProposerFactory::new(
			task_manager.spawn_handle(),
			client.clone(),
			transaction_pool.clone(),
			prometheus_registry.as_ref(),
			telemetry.as_ref().map(|x| x.handle()),
		);

		let slot_duration = sc_consensus_aura::slot_duration(&*client)?;

		let aura = sc_consensus_aura::start_aura::<AuraPair, _, _, _, _, _, _, _, _, _, _>(
			StartAuraParams {
				slot_duration,
				client: client.clone(),
				select_chain,
				block_import: grandpa_block_import,
				proposer_factory,
				create_inherent_data_providers: move |_, ()| async move {
					let timestamp = sp_timestamp::InherentDataProvider::from_system_time();

					let slot =
						sp_consensus_aura::inherents::InherentDataProvider::from_timestamp_and_slot_duration(
							*timestamp,
							slot_duration,
						);

					Ok((slot, timestamp))
				},
				force_authoring,
				backoff_authoring_blocks,
				keystore: keystore_container.keystore(),
				sync_oracle: sync_service.clone(),
				justification_sync_link: sync_service.clone(),
				block_proposal_slot_portion: SlotProportion::new(2f32 / 3f32),
				max_block_proposal_slot_portion: None,
				telemetry: telemetry.as_ref().map(|x| x.handle()),
				compatibility_mode: Default::default(),
			},
		)?;

		task_manager
			.spawn_essential_handle()
			.spawn_blocking("aura", Some("block-authoring"), aura);
	}

	let keystore = if role.is_authority() { Some(keystore_container.keystore()) } else { None };

	let grandpa_config = sc_consensus_grandpa::Config {
		gossip_duration: Duration::from_millis(333),
		justification_generation_period: GRANDPA_JUSTIFICATION_PERIOD,
		name: Some(name),
		observer_enabled: false,
		keystore,
		local_role: role,
		telemetry: telemetry.as_ref().map(|x| x.handle()),
		protocol_name: grandpa_protocol_name,
	};

	if enable_grandpa {
		let grandpa_params = sc_consensus_grandpa::GrandpaParams {
			config: grandpa_config,
			link: grandpa_link,
			network: network.clone(),
			telemetry: telemetry.as_ref().map(|x| x.handle()),
			voting_rule: sc_consensus_grandpa::VotingRulesBuilder::default().build(),
			prometheus_registry,
			shared_voter_state,
			sync: sync_service.clone(),
			offchain_tx_pool_factory: OffchainTransactionPoolFactory::new(transaction_pool),
		};

		task_manager.spawn_essential_handle().spawn_blocking(
			"grandpa-voter",
			None,
			sc_consensus_grandpa::run_grandpa_voter(grandpa_params)?,
		);
	}

	network_starter.start_network();
	Ok(NewFullBase { task_manager, client, network, transaction_pool, rpc_handlers })
}

pub fn build_rpc_extensions_builder(
	config: &Configuration,
	rpc_config: RpcConfig,
	builder: RpcExtensionsBuilder,
) -> impl Fn(DenyUnsafe, sc_rpc::SubscriptionTaskExecutor) -> Result<RpcModule<()>, sc_service::Error>
{
	let justification_stream = builder.justification_stream.clone();
	let shared_authority_set = builder.shared_authority_set.clone();

	let finality_proof_provider = sc_consensus_grandpa::FinalityProofProvider::new_for_service(
		builder.backend.clone(),
		Some(shared_authority_set.clone()),
	);

	let client = builder.client.clone();
	let pool = builder.transaction_pool.clone();
	let network = builder.network.clone();
	let select_chain = builder.select_chain.clone();
	let chain_spec = config.chain_spec.cloned_box();
	let backend = builder.backend.clone();
	let frontier_backend = builder.frontier_backend.clone();
	let is_authority = config.role.is_authority();
	let prometheus_registry = config.prometheus_registry().cloned();
	let sync_service = builder.sync_service.clone();

	let fee_history_cache: FeeHistoryCache = Arc::new(std::sync::Mutex::new(BTreeMap::new()));
	let filter_pool: FilterPool = Arc::new(std::sync::Mutex::new(BTreeMap::new()));
	let ethapi_cmd = rpc_config.ethapi.clone();

	let overrides = bifrost_common_node::rpc::overrides_handle(client.clone());

	let block_data_cache = Arc::new(fc_rpc::EthBlockDataCacheTask::new(
		builder.spawn_handle,
		overrides.clone(),
		rpc_config.eth_log_block_cache,
		rpc_config.eth_statuses_cache,
		prometheus_registry.clone(),
	));

	let slot_duration = sc_consensus_aura::slot_duration(&*client)?;
	let pending_create_inherent_data_providers = move |_, ()| async move {
		let current = sp_timestamp::InherentDataProvider::from_system_time();
		let next_slot = current.timestamp().as_millis() + slot_duration.as_millis();
		let timestamp = sp_timestamp::InherentDataProvider::new(next_slot.into());
		let slot =
			sp_consensus_aura::inherents::InherentDataProvider::from_timestamp_and_slot_duration(
				*timestamp,
				slot_duration,
			);
		Ok((slot, timestamp))
	};

	// Sinks for pubsub notifications.
	// Everytime a new subscription is created, a new mpsc channel is added to the sink pool.
	// The MappingSyncWorker sends through the channel on block import and the subscription emits a
	// notification to the subscriber on receiving a message through this channel.
	// This way we avoid race conditions when using native substrate block import notification
	// stream.
	let pubsub_notification_sinks: fc_mapping_sync::EthereumBlockNotificationSinks<
		fc_mapping_sync::EthereumBlockNotification<Block>,
	> = Default::default();
	let pubsub_notification_sinks = Arc::new(pubsub_notification_sinks);

	// Spawn Frontier FeeHistory cache maintenance task.
	builder.task_manager.spawn_essential_handle().spawn(
		"frontier-fee-history",
		Some("frontier"),
		EthTask::fee_history_task(
			client.clone(),
			overrides.clone(),
			fee_history_cache.clone(),
			rpc_config.fee_history_limit,
		),
	);

	// Frontier `EthFilterApi` maintenance.
	// Manages the pool of user-created Filters.
	const FILTER_RETAIN_THRESHOLD: u64 = 100;
	builder.task_manager.spawn_essential_handle().spawn(
		"frontier-filter-pool",
		Some("frontier"),
		EthTask::filter_pool_task(
			Arc::clone(&client),
			filter_pool.clone(),
			FILTER_RETAIN_THRESHOLD,
		),
	);

	match frontier_backend.clone() {
		Backend::KeyValue(b) => {
			// Frontier offchain DB task. Essential.
			// Maps emulated ethereum data to substrate native data.
			builder.task_manager.spawn_essential_handle().spawn(
				"frontier-mapping-sync-worker",
				Some("frontier"),
				MappingSyncWorker::new(
					client.import_notification_stream(),
					Duration::new(6, 0),
					client.clone(),
					backend.clone(),
					overrides.clone(),
					Arc::new(b),
					3,
					0,
					SyncStrategy::Normal,
					sync_service.clone(),
					pubsub_notification_sinks.clone(),
				)
				.for_each(|()| futures::future::ready(())),
			);
		},
		Backend::Sql(b) => {
			builder.task_manager.spawn_essential_handle().spawn_blocking(
				"frontier-mapping-sync-worker",
				Some("frontier"),
				fc_mapping_sync::sql::SyncWorker::run(
					client.clone(),
					backend.clone(),
					Arc::new(b),
					client.import_notification_stream(),
					fc_mapping_sync::sql::SyncWorkerConfig {
						read_notification_timeout: Duration::from_secs(10),
						check_indexed_blocks_interval: Duration::from_secs(60),
					},
					fc_mapping_sync::SyncStrategy::Parachain,
					sync_service.clone(),
					pubsub_notification_sinks.clone(),
				),
			);
		},
	}

	let tracing_requesters: RpcRequesters = {
		if ethapi_cmd.contains(&EthApiCmd::Debug) || ethapi_cmd.contains(&EthApiCmd::Trace) {
			spawn_tracing_tasks(
				&rpc_config,
				prometheus_registry.clone(),
				SpawnTasksParams {
					task_manager: &builder.task_manager,
					client: client.clone(),
					substrate_backend: backend.clone(),
					frontier_backend: frontier_backend.clone(),
					filter_pool: Some(filter_pool.clone()),
					overrides: overrides.clone(),
					fee_history_limit: rpc_config.fee_history_limit,
					fee_history_cache: fee_history_cache.clone(),
				},
			)
		} else {
			RpcRequesters { debug: None, trace: None }
		}
	};

	let rpc_extensions_builder = move |deny_unsafe, subscription_executor| {
		let deps = FullDeps {
			client: client.clone(),
			pool: pool.clone(),
			graph: pool.pool().clone(),
			select_chain: select_chain.clone(),
			chain_spec: chain_spec.cloned_box(),
			deny_unsafe,
			is_authority,
			filter_pool: filter_pool.clone(),
			ethapi_cmd: ethapi_cmd.clone(),
			network: network.clone(),
			backend: backend.clone(),
			frontier_backend: match frontier_backend.clone() {
				fc_db::Backend::KeyValue(b) => Arc::new(b),
				fc_db::Backend::Sql(b) => Arc::new(b),
			},
			fee_history_limit: rpc_config.fee_history_limit,
			fee_history_cache: fee_history_cache.clone(),
			block_data_cache: block_data_cache.clone(),
			overrides: overrides.clone(),
			grandpa: GrandpaDeps {
				shared_voter_state: builder.shared_voter_state.clone(),
				shared_authority_set: shared_authority_set.clone(),
				justification_stream: justification_stream.clone(),
				subscription_executor,
				finality_provider: finality_proof_provider.clone(),
			},
			max_past_logs: rpc_config.max_past_logs,
			logs_request_timeout: rpc_config.logs_request_timeout,
			forced_parent_hashes: None,
			sync_service: sync_service.clone(),
			pending_create_inherent_data_providers,
		};

		if ethapi_cmd.contains(&EthApiCmd::Debug) || ethapi_cmd.contains(&EthApiCmd::Trace) {
			create_full(
				deps,
				Some(TracingConfig {
					tracing_requesters: tracing_requesters.clone(),
					trace_filter_max_count: rpc_config.ethapi_trace_max_count,
				}),
				pubsub_notification_sinks.clone(),
			)
			.map_err(Into::into)
		} else {
			create_full(deps, None, pubsub_notification_sinks.clone()).map_err(Into::into)
		}
	};

	rpc_extensions_builder
}
