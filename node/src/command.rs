use crate::chain_spec;
use crate::cli::{Cli, RandomxFlag, Subcommand};
use crate::service;
use crate::command_helper::{inherent_benchmark_data, BenchmarkExtrinsicBuilder};
use log::warn;
use sc_cli::{ChainSpec, RuntimeVersion, SubstrateCli};
use sc_service::PartialComponents;
use crain_runtime::Block;
use std::sync::Arc;
use frame_benchmarking_cli::{BenchmarkCmd, SUBSTRATE_REFERENCE_HARDWARE};

const DEFAULT_CHECK_INHERENTS_AFTER: u32 = 152650;
const DEFAULT_ROUND: u32 = 1000;

impl SubstrateCli for Cli {
	fn impl_name() -> String {
		"Substrate Node".into()
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
		2017
	}

	fn load_spec(&self, id: &str) -> Result<Box<dyn sc_service::ChainSpec>, String> {
		Ok(match id {
			"dev" => Box::new(chain_spec::development_config()?),
			"" | "local" => Box::new(chain_spec::local_testnet_config()?),
			path =>
				Box::new(chain_spec::ChainSpec::from_json_file(std::path::PathBuf::from(path))?),
		})
	}

	fn native_runtime_version(_: &Box<dyn ChainSpec>) -> &'static RuntimeVersion {
		&crain_runtime::VERSION
	}
}

/// Parse and run command line arguments
pub fn run() -> sc_cli::Result<()> {
	let cli = Cli::from_args();

	let mut randomx_config = crain_pow::compute::Config::new();

	if cli.randomx_flags.contains(&RandomxFlag::LargePages) {
		warn!("Largepages flag is experimental and known to cause node instability. It is currently not recommended to run with this flag in a production environment.");
		randomx_config.large_pages = true;
	}
	if cli.randomx_flags.contains(&RandomxFlag::Secure) {
		randomx_config.secure = true;
	}	

	let _ = crain_pow::compute::set_global_config(randomx_config);

	match &cli.subcommand {

		Some(Subcommand::Key(cmd)) => cmd.run(&cli),
		
		Some(Subcommand::BuildSpec(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.sync_run(|config| cmd.run(config.chain_spec, config.network))
		},
		Some(Subcommand::CheckBlock(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.async_run(|config| {
				let PartialComponents {
					client,
					task_manager,
					import_queue,
					..
				} = crate::service::new_partial(
					&config,
					cli.check_inherents_after
						.unwrap_or(DEFAULT_CHECK_INHERENTS_AFTER),
					!cli.disable_weak_subjectivity,
				)?;
				Ok((cmd.run(client, import_queue), task_manager))
			})
		}
		Some(Subcommand::ExportBlocks(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.async_run(|config| {
				let PartialComponents {
					client,
					task_manager,
					..
				} = crate::service::new_partial(
					&config,
					cli.check_inherents_after
						.unwrap_or(DEFAULT_CHECK_INHERENTS_AFTER),
					!cli.disable_weak_subjectivity,
				)?;
				Ok((cmd.run(client, config.database), task_manager))
			})
		}
		Some(Subcommand::ExportState(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.async_run(|config| {
				let PartialComponents {
					client,
					task_manager,
					..
				} = crate::service::new_partial(
					&config,
					cli.check_inherents_after
						.unwrap_or(DEFAULT_CHECK_INHERENTS_AFTER),
					!cli.disable_weak_subjectivity,
				)?;
				Ok((cmd.run(client, config.chain_spec), task_manager))
			})
		}
		Some(Subcommand::ImportBlocks(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.async_run(|config| {
				let PartialComponents {
					client,
					task_manager,
					import_queue,
					..
				} = crate::service::new_partial(
					&config,
					cli.check_inherents_after
						.unwrap_or(DEFAULT_CHECK_INHERENTS_AFTER),
					!cli.disable_weak_subjectivity,
				)?;
				Ok((cmd.run(client, import_queue), task_manager))
			})
		}
		Some(Subcommand::PurgeChain(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.sync_run(|config| cmd.run(config.database))
		}

		Some(Subcommand::Revert(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.async_run(|config| {
				let PartialComponents {
					client,
					backend,
					task_manager,
					..
				} = crate::service::new_partial(
					&config,
					cli.check_inherents_after
						.unwrap_or(DEFAULT_CHECK_INHERENTS_AFTER),
					!cli.disable_weak_subjectivity,
				)?;
				Ok((cmd.run(client, backend, None), task_manager))
			})
		}

		// TODO not sure about this, taken from node template - not kulupu
		Some(Subcommand::Benchmark(cmd)) => {
			let runner = cli.create_runner(cmd)?;

			runner.sync_run(|config| {
				// This switch needs to be in the client, since the client decides
				// which sub-commands it wants to support.
				match cmd {
					BenchmarkCmd::Pallet(cmd) => {
						if !cfg!(feature = "runtime-benchmarks") {
							return Err(
								"Runtime benchmarking wasn't enabled when building the node. \
							You can enable it with `--features runtime-benchmarks`."
									.into(),
							)
						}

						cmd.run::<Block, service::ExecutorDispatch>(config)
					},
					BenchmarkCmd::Block(cmd) => {
						let PartialComponents { client, .. } = service::new_partial(
																											&config,
																						  cli.check_inherents_after
																											.unwrap_or(DEFAULT_CHECK_INHERENTS_AFTER),
																											!cli.disable_weak_subjectivity
																											)?;

						cmd.run(client)
					},
					BenchmarkCmd::Storage(cmd) => {
						let PartialComponents { client, backend, .. } =
							service::new_partial(
									&config,
									cli.check_inherents_after.unwrap_or(DEFAULT_CHECK_INHERENTS_AFTER),
									!cli.disable_weak_subjectivity)?;
						let db = backend.expose_db();
						let storage = backend.expose_storage();

						cmd.run(config, client, db, storage)
					},
					BenchmarkCmd::Overhead(cmd) => {
						let PartialComponents { client, .. } = service::new_partial(
																						&config,
																	  cli.check_inherents_after.unwrap_or(DEFAULT_CHECK_INHERENTS_AFTER),
																	  					!cli.disable_weak_subjectivity
																						)?;
						let ext_builder = BenchmarkExtrinsicBuilder::new(client.clone());

						cmd.run(config, client, inherent_benchmark_data()?, Arc::new(ext_builder))
					},
					BenchmarkCmd::Machine(cmd) =>
						cmd.run(&config, SUBSTRATE_REFERENCE_HARDWARE.clone()),			

				}
			})
		},

		None => {
			let runner = cli.create_runner(&cli.run)?;
			runner
				.run_node_until_exit(|config| async move {
					service::new_full(
							config,
							cli.author.as_ref().map(|s| s.as_str()),
							cli.threads.unwrap_or(1),
							cli.round.unwrap_or(DEFAULT_ROUND),
							cli.check_inherents_after
								.unwrap_or(DEFAULT_CHECK_INHERENTS_AFTER),
							!cli.disable_weak_subjectivity,
						)
				})
				.map_err(sc_cli::Error::Service)
		}
	}
}
