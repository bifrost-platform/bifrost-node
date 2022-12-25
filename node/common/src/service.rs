use bp_core::Block;
use sc_service::{BasePath, Configuration};
use std::sync::Arc;

/// Configure frontier database.
pub fn frontier_database_dir(config: &Configuration) -> std::path::PathBuf {
	let config_dir = config
		.base_path
		.as_ref()
		.map(|base_path| base_path.config_dir(config.chain_spec.id()))
		.unwrap_or_else(|| {
			BasePath::from_project("", "", "bifrost").config_dir(config.chain_spec.id())
		});
	config_dir.join("frontier").join("db")
}

/// Open frontier database.
pub fn open_frontier_backend(config: &Configuration) -> Result<Arc<fc_db::Backend<Block>>, String> {
	Ok(Arc::new(fc_db::Backend::<Block>::new(&fc_db::DatabaseSettings {
		source: fc_db::DatabaseSettingsSrc::RocksDb {
			path: frontier_database_dir(&config),
			cache_size: 1024,
		},
	})?))
}
