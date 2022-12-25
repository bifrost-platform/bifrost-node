use crate::cli::{Cli, Subcommand};

use bifrost_common_node::cli_opt::RpcConfig;

use sc_cli::{ChainSpec, RuntimeVersion, SubstrateCli};
use sc_service::PartialComponents;

trait IdentifyChain {
	fn is_dev(&self) -> bool;
	fn is_testnet(&self) -> bool;
	fn is_mainnet(&self) -> bool;
}

impl IdentifyChain for dyn sc_service::ChainSpec {
	fn is_dev(&self) -> bool {
		self.id().starts_with("dev")
	}
	fn is_testnet(&self) -> bool {
		self.id().starts_with("testnet")
	}
	fn is_mainnet(&self) -> bool {
		self.id().starts_with("mainnet")
	}
}

impl<T: sc_service::ChainSpec + 'static> IdentifyChain for T {
	fn is_dev(&self) -> bool {
		<dyn sc_service::ChainSpec>::is_dev(self)
	}
	fn is_testnet(&self) -> bool {
		<dyn sc_service::ChainSpec>::is_testnet(self)
	}
	fn is_mainnet(&self) -> bool {
		<dyn sc_service::ChainSpec>::is_mainnet(self)
	}
}

impl SubstrateCli for Cli {
	fn impl_name() -> String {
		"BIFROST Network".into()
	}

	fn impl_version() -> String {
		env!("SUBSTRATE_CLI_IMPL_VERSION").into()
	}

	fn description() -> String {
		env!("CARGO_PKG_DESCRIPTION").into()
	}

	fn author() -> String {
		env!("CARGO_PKG_AUTHORS").into()
	}

	fn support_url() -> String {
		"support.anonymous.an".into()
	}

	fn copyright_start_year() -> i32 {
		2022
	}

	fn load_spec(&self, id: &str) -> Result<Box<dyn sc_service::ChainSpec>, String> {
		Ok(match id {
			"dev" => Box::new(bifrost_dev_node::chain_spec::development_config()?),
			"testnet-local" => Box::new(bifrost_testnet_node::chain_spec::testnet_config()?),
			"testnet" => Box::new(bifrost_testnet_node::chain_spec::ChainSpec::from_json_file(
				std::path::PathBuf::from("./specs/bifrost-testnet.json"),
			)?),
			path => Box::new(bifrost_dev_node::chain_spec::ChainSpec::from_json_file(
				std::path::PathBuf::from(path),
			)?),
		})
	}

	fn native_runtime_version(chain_spec: &Box<dyn ChainSpec>) -> &'static RuntimeVersion {
		if chain_spec.is_dev() {
			&bifrost_dev_runtime::VERSION
		} else if chain_spec.is_testnet() {
			&bifrost_testnet_runtime::VERSION
		} else {
			&bifrost_dev_runtime::VERSION
		}
	}
}

/// Parse and run command line arguments
pub fn run() -> sc_cli::Result<()> {
	let cli = Cli::from_args();

	match &cli.subcommand {
		Some(Subcommand::Key(cmd)) => cmd.run(&cli),
		Some(Subcommand::BuildSpec(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.sync_run(|config| cmd.run(config.chain_spec, config.network))
		},
		Some(Subcommand::CheckBlock(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			let chain_spec = &runner.config().chain_spec;

			if chain_spec.is_dev() {
				runner.async_run(|config| {
					let PartialComponents { client, task_manager, import_queue, .. } =
						bifrost_dev_node::service::new_partial(&config)?;
					Ok((cmd.run(client, import_queue), task_manager))
				})
			} else if chain_spec.is_testnet() {
				runner.async_run(|config| {
					let PartialComponents { client, task_manager, import_queue, .. } =
						bifrost_testnet_node::service::new_partial(&config)?;
					Ok((cmd.run(client, import_queue), task_manager))
				})
			} else {
				runner.async_run(|config| {
					let PartialComponents { client, task_manager, import_queue, .. } =
						bifrost_dev_node::service::new_partial(&config)?;
					Ok((cmd.run(client, import_queue), task_manager))
				})
			}
		},
		Some(Subcommand::ExportBlocks(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			let chain_spec = &runner.config().chain_spec;

			if chain_spec.is_dev() {
				runner.async_run(|config| {
					let PartialComponents { client, task_manager, .. } =
						bifrost_dev_node::service::new_partial(&config)?;
					Ok((cmd.run(client, config.database), task_manager))
				})
			} else if chain_spec.is_testnet() {
				runner.async_run(|config| {
					let PartialComponents { client, task_manager, .. } =
						bifrost_testnet_node::service::new_partial(&config)?;
					Ok((cmd.run(client, config.database), task_manager))
				})
			} else {
				runner.async_run(|config| {
					let PartialComponents { client, task_manager, .. } =
						bifrost_dev_node::service::new_partial(&config)?;
					Ok((cmd.run(client, config.database), task_manager))
				})
			}
		},
		Some(Subcommand::ExportState(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			let chain_spec = &runner.config().chain_spec;

			if chain_spec.is_dev() {
				runner.async_run(|config| {
					let PartialComponents { client, task_manager, .. } =
						bifrost_dev_node::service::new_partial(&config)?;
					Ok((cmd.run(client, config.chain_spec), task_manager))
				})
			} else if chain_spec.is_testnet() {
				runner.async_run(|config| {
					let PartialComponents { client, task_manager, .. } =
						bifrost_testnet_node::service::new_partial(&config)?;
					Ok((cmd.run(client, config.chain_spec), task_manager))
				})
			} else {
				runner.async_run(|config| {
					let PartialComponents { client, task_manager, .. } =
						bifrost_dev_node::service::new_partial(&config)?;
					Ok((cmd.run(client, config.chain_spec), task_manager))
				})
			}
		},
		Some(Subcommand::ImportBlocks(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			let chain_spec = &runner.config().chain_spec;

			if chain_spec.is_dev() {
				runner.async_run(|config| {
					let PartialComponents { client, task_manager, import_queue, .. } =
						bifrost_dev_node::service::new_partial(&config)?;
					Ok((cmd.run(client, import_queue), task_manager))
				})
			} else if chain_spec.is_testnet() {
				runner.async_run(|config| {
					let PartialComponents { client, task_manager, import_queue, .. } =
						bifrost_testnet_node::service::new_partial(&config)?;
					Ok((cmd.run(client, import_queue), task_manager))
				})
			} else {
				runner.async_run(|config| {
					let PartialComponents { client, task_manager, import_queue, .. } =
						bifrost_dev_node::service::new_partial(&config)?;
					Ok((cmd.run(client, import_queue), task_manager))
				})
			}
		},
		Some(Subcommand::PurgeChain(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.sync_run(|config| cmd.run(config.database))
		},
		Some(Subcommand::Revert(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			let chain_spec = &runner.config().chain_spec;

			if chain_spec.is_dev() {
				runner.async_run(|config| {
					let PartialComponents { client, task_manager, backend, .. } =
						bifrost_dev_node::service::new_partial(&config)?;
					Ok((cmd.run(client, backend), task_manager))
				})
			} else if chain_spec.is_testnet() {
				runner.async_run(|config| {
					let PartialComponents { client, task_manager, backend, .. } =
						bifrost_testnet_node::service::new_partial(&config)?;
					Ok((cmd.run(client, backend), task_manager))
				})
			} else {
				runner.async_run(|config| {
					let PartialComponents { client, task_manager, backend, .. } =
						bifrost_dev_node::service::new_partial(&config)?;
					Ok((cmd.run(client, backend), task_manager))
				})
			}
		},
		Some(Subcommand::Benchmark(cmd)) =>
			if cfg!(feature = "runtime-benchmarks") {
				let runner = cli.create_runner(cmd)?;
				let chain_spec = &runner.config().chain_spec;

				if chain_spec.is_dev() {
					runner.sync_run(|config| {
						cmd.run::<bifrost_dev_runtime::Block, bifrost_dev_node::service::dev::ExecutorDispatch>(
							config,
						)
					})
				} else if chain_spec.is_testnet() {
					runner.sync_run(|config| {
						cmd.run::<bifrost_testnet_runtime::Block, bifrost_testnet_node::service::testnet::ExecutorDispatch>(config)
					})
				} else {
					runner.sync_run(|config| {
						cmd.run::<bifrost_dev_runtime::Block, bifrost_dev_node::service::dev::ExecutorDispatch>(
							config,
						)
					})
				}
			} else {
				Err("Benchmarking wasn't enabled when building the node. You can enable it with \
				     `--features runtime-benchmarks`."
					.into())
			},
		None => {
			let rpc_config = RpcConfig {
				ethapi: cli.ethapi.clone(),
				ethapi_max_permits: cli.ethapi_max_permits,
				ethapi_trace_max_count: cli.ethapi_trace_max_count,
				ethapi_trace_cache_duration: cli.ethapi_trace_cache_duration,
				eth_log_block_cache: cli.eth_log_block_cache,
				eth_statuses_cache: cli.eth_statuses_cache,
				fee_history_limit: cli.fee_history_limit,
				max_past_logs: cli.max_past_logs,
				max_logs_request_duration: cli.max_logs_request_duration,
			};

			let runner = cli.create_runner(&cli.run)?;
			let chain_spec = &runner.config().chain_spec;

			if chain_spec.is_dev() {
				if cli.sealing {
					runner.run_node_until_exit(|config| async move {
						bifrost_dev_node::service::new_manual(config, rpc_config)
							.map_err(sc_cli::Error::Service)
					})
				} else {
					runner.run_node_until_exit(|config| async move {
						bifrost_dev_node::service::new_full(config, rpc_config)
							.map_err(sc_cli::Error::Service)
					})
				}
			} else if chain_spec.is_testnet() {
				runner.run_node_until_exit(|config| async move {
					bifrost_testnet_node::service::new_full(config, rpc_config)
						.map_err(sc_cli::Error::Service)
				})
			} else {
				runner.run_node_until_exit(|config| async move {
					bifrost_dev_node::service::new_full(config, rpc_config)
						.map_err(sc_cli::Error::Service)
				})
			}
		},
	}
}
