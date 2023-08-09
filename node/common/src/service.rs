use bp_core::Block;
use fc_db::DatabaseSource;
use sc_client_api::{
	backend::{Backend, StateBackend},
	AuxStore, StorageProvider,
};
use sc_service::Configuration;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::{Error as BlockChainError, HeaderBackend, HeaderMetadata};
use sp_runtime::traits::BlakeTwo256;
use std::{path::Path, sync::Arc};

use crate::cli_opt::{BackendTypeConfig, RpcConfig};

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
) -> Result<fc_db::Backend<Block>, String>
where
	C: ProvideRuntimeApi<Block> + StorageProvider<Block, BE> + AuxStore,
	C: HeaderBackend<Block> + HeaderMetadata<Block, Error = BlockChainError>,
	C: Send + Sync + 'static,
	C::Api: fp_rpc::EthereumRuntimeRPCApi<Block>,
	BE: Backend<Block> + 'static,
	BE::State: StateBackend<BlakeTwo256>,
{
	let frontier_backend = match rpc_config.frontier_backend_type {
		BackendTypeConfig::KeyValue => fc_db::Backend::KeyValue(fc_db::kv::Backend::<Block>::new(
			client,
			&fc_db::kv::DatabaseSettings {
				source: match config.database {
					DatabaseSource::RocksDb { .. } => DatabaseSource::RocksDb {
						path: frontier_database_dir(config, "db"),
						cache_size: 0,
					},
					DatabaseSource::ParityDb { .. } => {
						DatabaseSource::ParityDb { path: frontier_database_dir(config, "paritydb") }
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
		)?),
		BackendTypeConfig::Sql { pool_size, num_ops_timeout, thread_count, cache_size } => {
			let overrides = crate::rpc::overrides_handle(client.clone());
			let sqlite_db_path = frontier_database_dir(config, "sql");
			std::fs::create_dir_all(&sqlite_db_path).expect("failed creating sql db directory");
			let backend = futures::executor::block_on(fc_db::sql::Backend::new(
				fc_db::sql::BackendConfig::Sqlite(fc_db::sql::SqliteBackendConfig {
					path: Path::new("sqlite:///")
						.join(sqlite_db_path)
						.join("frontier.db3")
						.to_str()
						.unwrap(),
					create_if_missing: true,
					thread_count,
					cache_size,
				}),
				pool_size,
				std::num::NonZeroU32::new(num_ops_timeout),
				overrides.clone(),
			))
			.unwrap_or_else(|err| panic!("failed creating sql backend: {:?}", err));
			fc_db::Backend::Sql(backend)
		},
	};

	Ok(frontier_backend)
}
