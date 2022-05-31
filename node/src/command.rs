use crate::chain_spec;
use crate::cli::{Cli, RandomxFlag, Subcommand};
use crate::service;
use log::{info, warn};
use sc_cli::{ChainSpec, Role, RuntimeVersion, SubstrateCli};
use sc_keystore::LocalKeystore;
use sc_service::{config::KeystoreConfig, PartialComponents};
use sp_core::{
	crypto::{Pair, Ss58AddressFormat, Ss58Codec},
	hexdisplay::HexDisplay,
};
use sp_keystore::SyncCryptoStore;
use std::{fs::File, io::Write, path::PathBuf};


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

		Some(Subcommand::ExportBuiltinWasm(cmd)) => {
			let wasm_binary_bloaty = kulupu_runtime::WASM_BINARY_BLOATY
				.ok_or("Wasm binary not available".to_string())?;
			let wasm_binary = kulupu_runtime::WASM_BINARY
				.ok_or("Compact Wasm binary not available".to_string())?;

			info!("Exporting builtin wasm binary to folder: {}", cmd.folder);

			let folder = PathBuf::from(cmd.folder.clone());
			{
				let mut path = folder.clone();
				path.push("kulupu_runtime.compact.wasm");
				let mut file = File::create(path)?;
				file.write_all(&wasm_binary)?;
				file.flush()?;
			}

			{
				let mut path = folder.clone();
				path.push("kulupu_runtime.wasm");
				let mut file = File::create(path)?;
				file.write_all(&wasm_binary_bloaty)?;
				file.flush()?;
			}

			Ok(())
		}
		Some(Subcommand::ImportMiningKey(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.sync_run(|config| {
				let keystore = match &config.keystore {
					KeystoreConfig::Path { path, password } => {
						LocalKeystore::open(path.clone(), password.clone())
							.map_err(|e| format!("Open keystore failed: {:?}", e))?
					}
					KeystoreConfig::InMemory => LocalKeystore::in_memory(),
				};

				let pair = kulupu_pow::app::Pair::from_string(&cmd.suri, None)
					.map_err(|e| format!("Invalid seed: {:?}", e))?;

				SyncCryptoStore::insert_unknown(
					&keystore,
					kulupu_pow::app::ID,
					&cmd.suri,
					pair.public().as_ref(),
				)
				.map_err(|e| format!("Registering mining key failed: {:?}", e))?;

				info!(
					"Registered one mining key (public key 0x{}).",
					HexDisplay::from(&pair.public().as_ref())
				);

				Ok(())
			})
		}
		Some(Subcommand::GenerateMiningKey(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.sync_run(|config| {
				let keystore = match &config.keystore {
					KeystoreConfig::Path { path, password } => {
						LocalKeystore::open(path.clone(), password.clone())
							.map_err(|e| format!("Open keystore failed: {:?}", e))?
					}
					KeystoreConfig::InMemory => LocalKeystore::in_memory(),
				};

				let (pair, phrase, _) = kulupu_pow::app::Pair::generate_with_phrase(None);

				SyncCryptoStore::insert_unknown(
					&keystore,
					kulupu_pow::app::ID,
					&phrase,
					pair.public().as_ref(),
				)
				.map_err(|e| format!("Registering mining key failed: {:?}", e))?;

				info!("Generated one mining key.");

				println!(
					"Public key: 0x{}\nSecret seed: {}\nAddress: {}",
					HexDisplay::from(&pair.public().as_ref()),
					phrase,
					pair.public()
						.to_ss58check_with_version(Ss58AddressFormat::KulupuAccount),
				);

				Ok(())
			})
		}
		Some(Subcommand::Benchmark(cmd)) => {
			if cfg!(feature = "runtime-benchmarks") {
				let runner = cli.create_runner(cmd)?;

				runner.sync_run(|config| {
					cmd.run::<kulupu_runtime::Block, crate::service::ExecutorDispatch>(config)
				})
			} else {
				Err("Benchmarking wasn't enabled when building the node. \
				You can enable it with `--features runtime-benchmarks`."
					.into())
			}
		},

		None => {
			let runner = cli.create_runner(&cli.run)?;
			runner
				.run_node_until_exit(|config| async move {
					match config.role {
						Role::Light => service::new_light(
							config,
							cli.check_inherents_after
								.unwrap_or(DEFAULT_CHECK_INHERENTS_AFTER),
							!cli.disable_weak_subjectivity,
						),
						_ => service::new_full(
							config,
							cli.author.as_ref().map(|s| s.as_str()),
							cli.threads.unwrap_or(1),
							cli.round.unwrap_or(DEFAULT_ROUND),
							cli.check_inherents_after
								.unwrap_or(DEFAULT_CHECK_INHERENTS_AFTER),
							!cli.disable_weak_subjectivity,
						),
					}
				})
				.map_err(sc_cli::Error::Service)
		}
	}
}
