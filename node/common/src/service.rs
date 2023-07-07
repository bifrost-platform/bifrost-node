use bp_core::Block;
use fc_db::DatabaseSource;
use sc_service::{BasePath, Configuration};
use std::sync::Arc;

/// Configure frontier database.
pub fn frontier_database_dir(config: &Configuration, path: &str) -> std::path::PathBuf {
	let config_dir = config
		.base_path
		.as_ref()
		.map(|base_path| base_path.config_dir(config.chain_spec.id()))
		.unwrap_or_else(|| {
			BasePath::from_project("", "", "bifrost").config_dir(config.chain_spec.id())
		});
	config_dir.join("frontier").join(path)
}

/// Open frontier database.
// TODO This is copied from frontier. It should be imported instead after
// https://github.com/paritytech/frontier/issues/333 is solved
pub fn open_frontier_backend<C>(
	client: Arc<C>,
	config: &Configuration,
) -> Result<Arc<fc_db::kv::Backend<Block>>, String>
where
	C: sp_blockchain::HeaderBackend<Block>,
{
	Ok(Arc::new(fc_db::kv::Backend::<Block>::new(
		client,
		&fc_db::kv::DatabaseSettings {
			source: match config.database {
				DatabaseSource::RocksDb { .. } => DatabaseSource::RocksDb {
					path: frontier_database_dir(config, "db"),
					cache_size: 0,
				},
				DatabaseSource::ParityDb { .. } =>
					DatabaseSource::ParityDb { path: frontier_database_dir(config, "paritydb") },
				DatabaseSource::Auto { .. } => DatabaseSource::Auto {
					rocksdb_path: frontier_database_dir(config, "db"),
					paritydb_path: frontier_database_dir(config, "paritydb"),
					cache_size: 0,
				},
				_ =>
					return Err("Supported db sources: `rocksdb` | `paritydb` | `auto`".to_string()),
			},
		},
	)?))
}
