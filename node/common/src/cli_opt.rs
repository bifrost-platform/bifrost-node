use std::str::FromStr;

#[derive(Debug, PartialEq, Clone)]
pub enum EthApi {
	Txpool,
	Debug,
	Trace,
}

impl FromStr for EthApi {
	type Err = String;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Ok(match s {
			"txpool" => Self::Txpool,
			"debug" => Self::Debug,
			"trace" => Self::Trace,
			_ => return Err(format!("`{}` is not recognized as a supported Ethereum Api", s)),
		})
	}
}

/// Avalailable frontier backend types.
#[derive(Debug, Copy, Clone, Default, clap::ValueEnum)]
pub enum BackendType {
	/// Either RocksDb or ParityDb as per inherited from the global backend settings.
	#[default]
	KeyValue,
	/// Sql database with custom log indexing.
	Sql,
}

/// Defines the frontier backend configuration.
pub enum BackendTypeConfig {
	KeyValue,
	Sql { pool_size: u32, num_ops_timeout: u32, thread_count: u32, cache_size: u64 },
}

impl Default for BackendTypeConfig {
	fn default() -> BackendTypeConfig {
		BackendTypeConfig::KeyValue
	}
}

pub struct RpcConfig {
	pub ethapi: Vec<EthApi>,
	pub ethapi_max_permits: u32,
	pub ethapi_trace_max_count: u32,
	pub ethapi_trace_cache_duration: u64,
	pub eth_log_block_cache: usize,
	pub eth_statuses_cache: usize,
	pub fee_history_limit: u64,
	pub max_past_logs: u32,
	pub logs_request_timeout: u64,
	pub tracing_raw_max_memory_usage: usize,
	pub frontier_backend_type: BackendTypeConfig,
}
