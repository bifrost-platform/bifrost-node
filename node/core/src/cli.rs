use bifrost_common_node::cli_opt::{BackendType, EthApi};
use sc_cli::RunCmd;

#[derive(Debug, clap::Parser)]
pub struct Cli {
	#[clap(subcommand)]
	pub subcommand: Option<Subcommand>,

	#[clap(flatten)]
	pub run: RunCmd,

	#[clap(long)]
	pub sealing: bool,

	/// Enable EVM tracing & Txpool module. ex) --ethapi=debug,trace,txpool
	#[clap(long, value_delimiter = ',')]
	pub ethapi: Vec<EthApi>,

	/// Number of concurrent tracing tasks.
	#[clap(long, default_value = "500")]
	pub ethapi_max_permits: u32,

	/// Maximum number of trace entries a single request of `trace_filter` is allowed to return.
	#[clap(long, default_value = "500")]
	pub ethapi_trace_max_count: u32,

	/// Duration (in seconds) after which the cache of `trace_filter` for a given block will be
	/// discarded.
	#[clap(long, default_value = "300")]
	pub ethapi_trace_cache_duration: u64,

	/// Size in bytes of the LRU cache for block data.
	#[clap(long, default_value = "100000000")]
	pub eth_log_block_cache: usize,

	/// Size in bytes of the LRU cache for transaction statuses data.
	#[clap(long, default_value = "100000000")]
	pub eth_statuses_cache: usize,

	/// Maximum number of logs in a query.
	#[clap(long, default_value = "10000")]
	pub max_past_logs: u32,

	/// Timeout for eth logs query RPCs in seconds. (default 10)
	#[clap(long, default_value = "10")]
	pub logs_request_timeout: u64,

	/// Maximum fee history cache size.
	#[clap(long, default_value = "2048")]
	pub fee_history_limit: u64,

	/// Size in bytes of data a raw tracing request is allowed to use.
	/// Bound the size of memory, stack and storage data.
	#[clap(long, default_value = "20000000")]
	pub tracing_raw_max_memory_usage: usize,

	/// Sets the frontier backend type (KeyValue or Sql)
	#[arg(long, value_enum, ignore_case = true, default_value_t = BackendType::default())]
	pub frontier_backend_type: BackendType,

	// Sets the SQL backend's pool size.
	#[arg(long, default_value = "100")]
	pub frontier_sql_backend_pool_size: u32,

	/// Sets the SQL backend's query timeout in number of VM ops.
	#[arg(long, default_value = "10000000")]
	pub frontier_sql_backend_num_ops_timeout: u32,

	/// Sets the SQL backend's auxiliary thread limit.
	#[arg(long, default_value = "4")]
	pub frontier_sql_backend_thread_count: u32,

	/// Sets the SQL backend's query timeout in number of VM ops.
	/// Default value is 200MB.
	#[arg(long, default_value = "209715200")]
	pub frontier_sql_backend_cache_size: u64,
}

#[derive(Debug, clap::Subcommand)]
pub enum Subcommand {
	/// Key management cli utilities
	#[clap(subcommand)]
	Key(sc_cli::KeySubcommand),

	/// Build a chain specification.
	BuildSpec(sc_cli::BuildSpecCmd),

	/// Validate blocks.
	CheckBlock(sc_cli::CheckBlockCmd),

	/// Export blocks.
	ExportBlocks(sc_cli::ExportBlocksCmd),

	/// Export the state of a given block into a chain spec.
	ExportState(sc_cli::ExportStateCmd),

	/// Import blocks.
	ImportBlocks(sc_cli::ImportBlocksCmd),

	/// Remove the whole chain.
	PurgeChain(sc_cli::PurgeChainCmd),

	/// Revert the chain to a previous state.
	Revert(sc_cli::RevertCmd),

	/// The custom benchmark subcommand benchmarking runtime pallets.
	#[clap(subcommand)]
	Benchmark(frame_benchmarking_cli::BenchmarkCmd),
}
