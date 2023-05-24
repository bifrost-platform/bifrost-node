//! Service and ServiceFactory implementation. Specialized wrapper over substrate service.

use bp_core::*;
use futures::StreamExt;
use jsonrpsee::RpcModule;
use std::{collections::BTreeMap, sync::Arc, time::Duration};

use bifrost_common_node::{
	cli_opt::{EthApi as EthApiCmd, RpcConfig},
	rpc::{FullDevDeps, GrandpaDeps, SpawnTasksParams, TracingConfig},
	service::open_frontier_backend,
	tracing::{spawn_tracing_tasks, RpcRequesters},
};

use crate::rpc::create_full;

use fc_mapping_sync::{MappingSyncWorker, SyncStrategy};
use fc_rpc::EthTask;
use fc_rpc_core::types::{FeeHistoryCache, FilterPool};

use sc_client_api::{BlockBackend, BlockchainEvents};
use sc_consensus_aura::{ImportQueueParams, SlotProportion, StartAuraParams};
use sc_consensus_manual_seal::EngineCommand;
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
use sp_keystore::SyncCryptoStorePtr;
use sp_runtime::traits::Block as BlockT;

/// Development runtime executor
pub mod dev {
	pub use bifrost_dev_runtime::RuntimeApi;

	pub struct ExecutorDispatch;
	impl sc_executor::NativeExecutionDispatch for ExecutorDispatch {
		#[cfg(feature = "runtime-benchmarks")]
		type ExtendHostFunctions = frame_benchmarking::benchmarking::HostFunctions;

		#[cfg(not(feature = "runtime-benchmarks"))]
		type ExtendHostFunctions = fp_ext::bifrost_ext::HostFunctions;

		fn dispatch(method: &str, data: &[u8]) -> Option<Vec<u8>> {
			bifrost_dev_runtime::api::dispatch(method, data)
		}

		fn native_version() -> sc_executor::NativeVersion {
			bifrost_dev_runtime::native_version()
		}
	}
}

/// The full client type definition.
type FullClient =
	sc_service::TFullClient<Block, dev::RuntimeApi, NativeElseWasmExecutor<dev::ExecutorDispatch>>;

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

/// Builds a new service for test client.
pub fn new_manual(
	config: Configuration,
	rpc_config: RpcConfig,
) -> Result<TaskManager, ServiceError> {
	new_manual_base(config, rpc_config).map(|NewFullBase { task_manager, .. }| task_manager)
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

	pub justification_stream: sc_finality_grandpa::GrandpaJustificationStream<Block>,
	pub shared_voter_state: sc_finality_grandpa::SharedVoterState,
	pub shared_authority_set:
		sc_finality_grandpa::SharedAuthoritySet<<Block as BlockT>::Hash, NumberFor<Block>>,

	pub client: Arc<FullClient>,
	pub backend: Arc<FullBackend>,
	pub select_chain: FullSelectChain,
	pub transaction_pool: Arc<TransactionPool>,
	pub network: Arc<NetworkService<Block, <Block as BlockT>::Hash>>,
	pub keystore_container: SyncCryptoStorePtr,
	pub frontier_backend: Arc<fc_db::Backend<Block>>,

	pub command_sink: Option<futures::channel::mpsc::Sender<EngineCommand<Hash>>>,
}

pub fn new_partial(
	config: &Configuration,
) -> Result<
	sc_service::PartialComponents<
		FullClient,
		FullBackend,
		FullSelectChain,
		sc_consensus::DefaultImportQueue<Block, FullClient>,
		sc_transaction_pool::FullPool<Block, FullClient>,
		(
			sc_finality_grandpa::GrandpaBlockImport<
				FullBackend,
				Block,
				FullClient,
				FullSelectChain,
			>,
			sc_finality_grandpa::LinkHalf<Block, FullClient, FullSelectChain>,
			Arc<fc_db::Backend<Block>>,
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

	let executor = NativeElseWasmExecutor::<dev::ExecutorDispatch>::new(
		config.wasm_method,
		config.default_heap_pages,
		config.max_runtime_instances,
		config.runtime_cache_size,
	);

	let (client, backend, keystore_container, task_manager) =
		sc_service::new_full_parts::<Block, dev::RuntimeApi, _>(
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

	let frontier_backend = open_frontier_backend(client.clone(), config)?;

	let (grandpa_block_import, grandpa_link) = sc_finality_grandpa::block_import(
		client.clone(),
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
	mut config: Configuration,
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
	} = new_partial(&config)?;

	let shared_voter_state = sc_finality_grandpa::SharedVoterState::empty();
	let grandpa_protocol_name = sc_finality_grandpa::protocol_standard_name(
		&client.block_hash(0).ok().flatten().expect("Genesis block exists; qed"),
		&config.chain_spec,
	);

	config
		.network
		.extra_sets
		.push(sc_finality_grandpa::grandpa_peers_set_config(grandpa_protocol_name.clone()));
	let warp_sync = Arc::new(sc_finality_grandpa::warp_proof::NetworkProvider::new(
		backend.clone(),
		grandpa_link.shared_authority_set().clone(),
		Vec::default(),
	));

	let (network, system_rpc_tx, tx_handler_controller, network_starter) =
		sc_service::build_network(sc_service::BuildNetworkParams {
			config: &config,
			client: client.clone(),
			transaction_pool: transaction_pool.clone(),
			spawn_handle: task_manager.spawn_handle(),
			import_queue,
			block_announce_validator_builder: None,
			warp_sync_params: Some(WarpSyncParams::WithProvider(warp_sync)),
		})?;

	if config.offchain_worker.enabled {
		sc_service::build_offchain_workers(
			&config,
			task_manager.spawn_handle(),
			client.clone(),
			network.clone(),
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
			keystore_container: keystore_container.sync_keystore(),
			frontier_backend: frontier_backend.clone(),
			command_sink: None,
		},
	);

	let rpc_handlers = sc_service::spawn_tasks(sc_service::SpawnTasksParams {
		config,
		backend: backend.clone(),
		client: client.clone(),
		network: network.clone(),
		keystore: keystore_container.sync_keystore(),
		rpc_builder: Box::new(rpc_extensions_builder),
		transaction_pool: transaction_pool.clone(),
		task_manager: &mut task_manager,
		system_rpc_tx,
		tx_handler_controller,
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
				keystore: keystore_container.sync_keystore(),
				sync_oracle: network.clone(),
				justification_sync_link: network.clone(),
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

	let keystore =
		if role.is_authority() { Some(keystore_container.sync_keystore()) } else { None };

	let grandpa_config = sc_finality_grandpa::Config {
		gossip_duration: Duration::from_millis(333),
		justification_period: 512,
		name: Some(name),
		observer_enabled: false,
		keystore,
		local_role: role,
		telemetry: telemetry.as_ref().map(|x| x.handle()),
		protocol_name: grandpa_protocol_name,
	};

	if enable_grandpa {
		let grandpa_params = sc_finality_grandpa::GrandpaParams {
			config: grandpa_config,
			link: grandpa_link,
			network: network.clone(),
			telemetry: telemetry.as_ref().map(|x| x.handle()),
			voting_rule: sc_finality_grandpa::VotingRulesBuilder::default().build(),
			prometheus_registry,
			shared_voter_state,
		};

		task_manager.spawn_essential_handle().spawn_blocking(
			"grandpa-voter",
			None,
			sc_finality_grandpa::run_grandpa_voter(grandpa_params)?,
		);
	}

	network_starter.start_network();
	Ok(NewFullBase { task_manager, client, network, transaction_pool, rpc_handlers })
}

/// Creates a test service from the configuration.
pub fn new_manual_base(
	mut config: Configuration,
	rpc_config: RpcConfig,
) -> Result<NewFullBase, ServiceError> {
	use sc_consensus_manual_seal::{
		consensus::{aura::AuraConsensusDataProvider, timestamp::SlotTimestampProvider},
		run_manual_seal, ManualSealParams,
	};

	let sc_service::PartialComponents {
		client,
		backend,
		mut task_manager,
		import_queue,
		keystore_container,
		select_chain,
		transaction_pool,
		other: (grandpa_block_import, grandpa_link, frontier_backend, mut telemetry),
	} = new_partial(&config)?;

	let shared_voter_state = sc_finality_grandpa::SharedVoterState::empty();
	let grandpa_protocol_name = sc_finality_grandpa::protocol_standard_name(
		&client.block_hash(0).ok().flatten().expect("Genesis block exists; qed"),
		&config.chain_spec,
	);

	config
		.network
		.extra_sets
		.push(sc_finality_grandpa::grandpa_peers_set_config(grandpa_protocol_name.clone()));
	let warp_sync = Arc::new(sc_finality_grandpa::warp_proof::NetworkProvider::new(
		backend.clone(),
		grandpa_link.shared_authority_set().clone(),
		Vec::default(),
	));

	let (network, system_rpc_tx, tx_handler_controller, network_starter) =
		sc_service::build_network(sc_service::BuildNetworkParams {
			config: &config,
			client: client.clone(),
			transaction_pool: transaction_pool.clone(),
			spawn_handle: task_manager.spawn_handle(),
			import_queue,
			block_announce_validator_builder: None,
			warp_sync_params: Some(WarpSyncParams::WithProvider(warp_sync)),
		})?;

	if config.offchain_worker.enabled {
		sc_service::build_offchain_workers(
			&config,
			task_manager.spawn_handle(),
			client.clone(),
			network.clone(),
		);
	}

	let role = config.role.clone();
	let name = config.network.node_name.clone();
	let enable_grandpa = !config.disable_grandpa;
	let prometheus_registry = config.prometheus_registry().cloned();

	let keystore =
		if role.is_authority() { Some(keystore_container.sync_keystore()) } else { None };

	let mut rpc_handlers = None;
	if let sc_service::config::Role::Authority { .. } = &role {
		let proposer = sc_basic_authorship::ProposerFactory::new(
			task_manager.spawn_handle(),
			client.clone(),
			transaction_pool.clone(),
			prometheus_registry.as_ref(),
			telemetry.as_ref().map(|x| x.handle()),
		);

		let (command_sink, commands_stream) = futures::channel::mpsc::channel(10);
		let consensus_data_provider = AuraConsensusDataProvider::new(client.clone());

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
				keystore_container: keystore_container.sync_keystore(),
				frontier_backend: frontier_backend.clone(),
				command_sink: Some(command_sink.clone()),
			},
		);

		rpc_handlers = sc_service::spawn_tasks(sc_service::SpawnTasksParams {
			config,
			backend: backend.clone(),
			client: client.clone(),
			network: network.clone(),
			keystore: keystore_container.sync_keystore(),
			rpc_builder: Box::new(rpc_extensions_builder),
			transaction_pool: transaction_pool.clone(),
			task_manager: &mut task_manager,
			system_rpc_tx,
			tx_handler_controller,
			telemetry: telemetry.as_mut(),
		})
		.ok();

		let client_clone = client.clone();
		let create_inherent_data_providers = Box::new(move |_, _| {
			let client = client_clone.clone();
			async move {
				let timestamp = SlotTimestampProvider::new_aura(client.clone())
					.map_err(|err| format!("{:?}", err))?;
				let slot = sp_consensus_aura::inherents::InherentDataProvider::new(
					timestamp.slot().into(),
				);
				Ok((timestamp, slot))
			}
		});

		task_manager.spawn_essential_handle().spawn_blocking(
			"authorship_task",
			Some("block-authoring"),
			run_manual_seal(ManualSealParams {
				block_import: grandpa_block_import,
				env: proposer,
				client: client.clone(),
				pool: transaction_pool.clone(),
				commands_stream,
				select_chain,
				consensus_data_provider: Some(Box::new(consensus_data_provider)),
				create_inherent_data_providers,
			}),
		);
	}

	let grandpa_config = sc_finality_grandpa::Config {
		gossip_duration: Duration::from_millis(333),
		justification_period: 512,
		name: Some(name),
		observer_enabled: false,
		keystore,
		local_role: role,
		telemetry: telemetry.as_ref().map(|x| x.handle()),
		protocol_name: grandpa_protocol_name,
	};

	if enable_grandpa {
		let grandpa_params = sc_finality_grandpa::GrandpaParams {
			config: grandpa_config,
			link: grandpa_link,
			network: network.clone(),
			telemetry: telemetry.as_ref().map(|x| x.handle()),
			voting_rule: sc_finality_grandpa::VotingRulesBuilder::default().build(),
			prometheus_registry,
			shared_voter_state,
		};

		task_manager.spawn_essential_handle().spawn_blocking(
			"grandpa-voter",
			None,
			sc_finality_grandpa::run_grandpa_voter(grandpa_params)?,
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

	let finality_proof_provider = sc_finality_grandpa::FinalityProofProvider::new_for_service(
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

	let command_sink = builder.command_sink.clone();
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
			frontier_backend.clone(),
			3,
			0,
			SyncStrategy::Normal,
		)
		.for_each(|()| futures::future::ready(())),
	);

	let tracing_requesters: RpcRequesters = {
		if ethapi_cmd.contains(&EthApiCmd::Debug) || ethapi_cmd.contains(&EthApiCmd::Trace) {
			spawn_tracing_tasks(
				&rpc_config,
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
		let deps = FullDevDeps {
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
			frontier_backend: frontier_backend.clone(),
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
			command_sink: command_sink.clone(),
			max_past_logs: rpc_config.max_past_logs,
			logs_request_timeout: rpc_config.logs_request_timeout,
		};

		if ethapi_cmd.contains(&EthApiCmd::Debug) || ethapi_cmd.contains(&EthApiCmd::Trace) {
			create_full(
				deps,
				Some(TracingConfig {
					tracing_requesters: tracing_requesters.clone(),
					trace_filter_max_count: rpc_config.ethapi_trace_max_count,
				}),
			)
			.map_err(Into::into)
		} else {
			create_full(deps, None).map_err(Into::into)
		}
	};

	rpc_extensions_builder
}
