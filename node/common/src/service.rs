use crate::cli_opt::{BackendTypeConfig, RpcConfig};

use std::{path::Path, sync::Arc};

use fc_db::DatabaseSource;
use fc_rpc::StorageOverrideHandler;
use sc_client_api::{
	backend::{Backend, StateBackend},
	AuxStore, StorageProvider,
};
use sc_service::Configuration;

use bp_core::Block;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::{Error as BlockChainError, HeaderBackend, HeaderMetadata};
use sp_runtime::traits::BlakeTwo256;

/// Only enable the benchmarking host functions when we actually want to benchmark.
#[cfg(feature = "runtime-benchmarks")]
pub type HostFunctions = (
	sp_io::SubstrateHostFunctions,
	frame_benchmarking::benchmarking::HostFunctions,
	cumulus_primitives_proof_size_hostfunction::storage_proof_size::HostFunctions,
);
/// Otherwise we use storage proof size host functions for compatibility.
#[cfg(not(feature = "runtime-benchmarks"))]
pub type HostFunctions = (
	sp_io::SubstrateHostFunctions,
	fp_ext::bifrost_ext::HostFunctions,
	cumulus_primitives_proof_size_hostfunction::storage_proof_size::HostFunctions,
);

/// Configure frontier database.
pub fn frontier_database_dir(config: &Configuration, path: &str) -> std::path::PathBuf {
	config.base_path.config_dir(config.chain_spec.id()).join("frontier").join(path)
}

// TODO This is copied from frontier. It should be imported instead after
// https://github.com/paritytech/frontier/issues/333 is solved
pub fn open_frontier_backend<C, BE>(
	client: Arc<C>,
	config: &Configuration,
	rpc_config: &RpcConfig,
) -> Result<fc_db::Backend<Block, C>, String>
where
	C: ProvideRuntimeApi<Block> + StorageProvider<Block, BE> + AuxStore,
	C: HeaderBackend<Block> + HeaderMetadata<Block, Error = BlockChainError>,
	C: Send + Sync + 'static,
	C::Api: fp_rpc::EthereumRuntimeRPCApi<Block>,
	BE: Backend<Block> + 'static,
	BE::State: StateBackend<BlakeTwo256>,
{
	let frontier_backend = match rpc_config.frontier_backend_type {
		BackendTypeConfig::KeyValue => {
			fc_db::Backend::KeyValue(Arc::new(fc_db::kv::Backend::<Block, C>::new(
				client,
				&fc_db::kv::DatabaseSettings {
					source: match config.database {
						DatabaseSource::RocksDb { .. } => DatabaseSource::RocksDb {
							path: frontier_database_dir(config, "db"),
							cache_size: 0,
						},
						DatabaseSource::ParityDb { .. } => DatabaseSource::ParityDb {
							path: frontier_database_dir(config, "paritydb"),
						},
						DatabaseSource::Auto { .. } => DatabaseSource::Auto {
							rocksdb_path: frontier_database_dir(config, "db"),
							paritydb_path: frontier_database_dir(config, "paritydb"),
							cache_size: 0,
						},
						_ => {
							return Err(
								"Supported db sources: `rocksdb` | `paritydb` | `auto`".to_string()
							)
						},
					},
				},
			)?))
		},
		BackendTypeConfig::Sql { pool_size, num_ops_timeout, thread_count, cache_size } => {
			let overrides = Arc::new(StorageOverrideHandler::<Block, _, _>::new(client.clone()));
			let sqlite_db_path = frontier_database_dir(config, "sql");
			std::fs::create_dir_all(&sqlite_db_path).expect("failed creating sql db directory");
			let backend = futures::executor::block_on(fc_db::sql::Backend::new(
				fc_db::sql::BackendConfig::Sqlite(fc_db::sql::SqliteBackendConfig {
					path: Path::new("sqlite:///")
						.join(sqlite_db_path)
						.join("frontier.db3")
						.to_str()
						.expect("frontier sql path error"),
					create_if_missing: true,
					thread_count,
					cache_size,
				}),
				pool_size,
				std::num::NonZeroU32::new(num_ops_timeout),
				overrides.clone(),
			))
				.unwrap_or_else(|err| panic!("failed creating sql backend: {:?}", err));
			fc_db::Backend::Sql(Arc::new(backend))
		},
	};

	Ok(frontier_backend)
}
