//! Service and ServiceFactory implementation. Specialized wrapper over substrate service.

use async_trait::async_trait;
use parity_scale_codec::Encode;
use crain_pow::compute::Error as ComputeError;
use crain_pow::compute::RandomxError;
use crain_pow::Error as PowError;
use crain_runtime::{self, opaque::Block, RuntimeApi};
use log::*;
use parking_lot::Mutex;
use sc_client_api::ExecutorProvider;
use sc_consensus::DefaultImportQueue;
pub use sc_executor::NativeElseWasmExecutor;
use sc_service::{error::Error as ServiceError, Configuration, TaskManager};
use sc_telemetry::{Telemetry, TelemetryWorker};
use sp_core::{
	crypto::{Ss58AddressFormat, Ss58AddressFormatRegistry, Ss58Codec, UncheckedFrom},
	Pair, H256,
};
use sp_keystore::{SyncCryptoStore, SyncCryptoStorePtr};
use sp_runtime::{generic::BlockId, traits::Block as BlockT};
use std::path::PathBuf;
use std::str::FromStr;
use std::{sync::Arc, time::Duration};
use std::thread;
use crain_pow::RandomXAlgorithm;


pub struct InherentDataProvidersBuilder;

#[async_trait]
impl sp_inherents::CreateInherentDataProviders<Block, ()> for InherentDataProvidersBuilder {
	type InherentDataProviders = sp_timestamp::InherentDataProvider;

	async fn create_inherent_data_providers(
		&self,
		_parent: <Block as BlockT>::Hash,
		_extra_args: (),
	) -> Result<Self::InherentDataProviders, Box<dyn std::error::Error + Send + Sync>> {
		Ok(sp_timestamp::InherentDataProvider::from_system_time())
	}
}

// Our native executor instance.
pub struct ExecutorDispatch;

impl sc_executor::NativeExecutionDispatch for ExecutorDispatch {
	/// Only enable the benchmarking host functions when we actually want to benchmark.
	#[cfg(feature = "runtime-benchmarks")]
	type ExtendHostFunctions = frame_benchmarking::benchmarking::HostFunctions;
	/// Otherwise we only use the default Substrate host functions.
	#[cfg(not(feature = "runtime-benchmarks"))]
	type ExtendHostFunctions = ();

	fn dispatch(method: &str, data: &[u8]) -> Option<Vec<u8>> {
		crain_runtime::api::dispatch(method, data)
	}

	fn native_version() -> sc_executor::NativeVersion {
		crain_runtime::native_version()
	}
}

pub(crate) type FullClient =
	sc_service::TFullClient<Block, RuntimeApi, NativeElseWasmExecutor<ExecutorDispatch>>;
type FullBackend = sc_service::TFullBackend<Block>;
type FullSelectChain = sc_consensus::LongestChain<FullBackend, Block>;



type PowBlockImport = crain_pow_consensus::PowBlockImport<
	Block,
	crain_pow::weak_sub::WeakSubjectiveBlockImport<
		Block,
		Arc<FullClient>,
		FullClient,
		FullSelectChain,
		crain_pow::RandomXAlgorithm<FullClient>,
		crain_pow::weak_sub::ExponentialWeakSubjectiveAlgorithm,
	>,
	FullClient,
	FullSelectChain,
	crain_pow::RandomXAlgorithm<FullClient>,
	sp_consensus::CanAuthorWithNativeVersion<
		sc_service::LocalCallExecutor<
			Block,
			sc_client_db::Backend<Block>,
			NativeElseWasmExecutor<ExecutorDispatch>,
		>,
	>,
	InherentDataProvidersBuilder,
>;


/// Returns most parts of a service. Not enough to run a full chain,
// But enough to perform chain operations like purge-chain
pub fn new_partial(
	config: &Configuration,
	check_inherents_after: u32,
	enable_weak_subjectivity: bool,
) -> Result<
	sc_service::PartialComponents<
		FullClient,
		FullBackend,
		FullSelectChain,
		DefaultImportQueue<Block, FullClient>,
		sc_transaction_pool::FullPool<Block, FullClient>,
		(PowBlockImport, Option<Telemetry>),
	>,
	ServiceError,
> {
	if config.keystore_remote.is_some() {
		return Err(ServiceError::Other("Remote Keystores are not supported.".into()))
	}

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

	let executor = NativeElseWasmExecutor::<ExecutorDispatch>::new(
		config.wasm_method,
		config.default_heap_pages,
		config.max_runtime_instances,
		config.runtime_cache_size,
	);

	let (client, backend, keystore_container, task_manager) = sc_service::new_full_parts(
		&config,
		telemetry.as_ref().map(|(_, telemetry)| telemetry.handle()),
		executor,
	)?;

	let client = Arc::new(client);

	let telemetry = telemetry.map(|(worker, telemetry)| {
		task_manager.spawn_handle().spawn("telemetry", None, worker.run());
		telemetry
	});


	// The longest chain is selected to be the only one
	let select_chain = sc_consensus::LongestChain::new(backend.clone());

	let transaction_pool = sc_transaction_pool::BasicPool::new_full(
		config.transaction_pool.clone(),
		config.role.is_authority().into(),
		config.prometheus_registry(),
		task_manager.spawn_essential_handle(),
		client.clone(),
	);

	// Custom SHA3 PoW Algorithm
	let algorithm = crain_pow::RandomXAlgorithm::new(client.clone());

	let weak_sub_block_import = crain_pow::weak_sub::WeakSubjectiveBlockImport::new(
		client.clone(),
		client.clone(),
		algorithm.clone(),
		crain_pow::weak_sub::ExponentialWeakSubjectiveAlgorithm(30, 1.1),
		select_chain.clone(),
		enable_weak_subjectivity,
	);

	let pow_block_import = crain_pow_consensus::PowBlockImport::new(
		weak_sub_block_import,
		client.clone(),
		algorithm.clone(),
		check_inherents_after,
		select_chain.clone(),
		InherentDataProvidersBuilder,
		sp_consensus::CanAuthorWithNativeVersion::new(client.executor().clone()),
	);

	let import_queue = crain_pow_consensus::import_queue(
		Box::new(pow_block_import.clone()),
		None,
		algorithm.clone(),
		&task_manager.spawn_essential_handle(),
		config.prometheus_registry(),
	)?;


	Ok(sc_service::PartialComponents {
		client,
		backend,
		task_manager,
		import_queue,
		keystore_container,
		select_chain,
		transaction_pool,
		other: (pow_block_import, telemetry),
	})
}



// TODO copied from Kulupu service.rs
pub fn decode_author(
	author: Option<&str>,
	keystore: SyncCryptoStorePtr,
	keystore_path: Option<PathBuf>,
) -> Result<crain_pow::app::Public, String> {
	if let Some(author) = author {
		if author.starts_with("0x") {
			Ok(crain_pow::app::Public::unchecked_from(
				H256::from_str(&author[2..]).map_err(|_| "Invalid author account".to_string())?,
			)
			.into())
		} else {
			// This line compiles if sp_core::crypto std feature is enabled
			let (address, version) = crain_pow::app::Public::from_ss58check_with_version(author)
				.map_err(|_| "Invalid author address".to_string())?;
			if version != Ss58AddressFormat::from(Ss58AddressFormatRegistry::BareSr25519Account) {
				return Err("Invalid author version".to_string());
			}
			Ok(address)
		}
	} else {
		info!("The node is configured for mining, but no author key is provided.");

		// This line compiles if sp_application_crypto std feature is enabled
		let (pair, phrase, _) = crain_pow::app::Pair::generate_with_phrase(None);

		SyncCryptoStore::insert_unknown(
			&*keystore.as_ref(),
			crain_pow::app::ID,
			&phrase,
			pair.public().as_ref(),
		)
		.map_err(|e| format!("Registering mining key failed: {:?}", e))?;

		info!(
			"Generated a mining key with address: {}",
			pair.public()
				.to_ss58check_with_version(Ss58AddressFormat::from(Ss58AddressFormatRegistry::BareSr25519Account))
		);

		match keystore_path {
			Some(path) => info!("You can go to {:?} to find the seed phrase of the mining key.", path),
			None => warn!("Keystore is not local. This means that your mining key will be lost when exiting the program. This should only happen if you are in dev mode."),
		}

		Ok(pair.public())
	}
}

/// Builds a new service for a full client.
// TODO delete author from parameters?
pub fn new_full(
	config: Configuration,
	author: Option<&str>,
	threads: usize,
	round: u32,
	check_inherents_after: u32,
	enable_weak_subjectivity: bool,
) -> Result<TaskManager, ServiceError> {
	
	// Create partial components of the server first to be used for full node
	let sc_service::PartialComponents {
		client,
		backend,
		mut task_manager,
		import_queue,
		keystore_container,
		select_chain,
		transaction_pool,
		other: (pow_block_import, mut telemetry),
	} = new_partial(&config, check_inherents_after, enable_weak_subjectivity)?;


	// Create a network service, RPS sender and network status sinker
	let (network, system_rpc_tx, network_starter) =
		sc_service::build_network(sc_service::BuildNetworkParams {
			config: &config,
			client: client.clone(),
			transaction_pool: transaction_pool.clone(),
			spawn_handle: task_manager.spawn_handle(),
			import_queue,
			block_announce_validator_builder: None,
			warp_sync: None,
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
	let prometheus_registry = config.prometheus_registry().cloned();

	let rpc_extensions_builder = {
		let client = client.clone();
		let pool = transaction_pool.clone();

		Box::new(move |deny_unsafe, _| {
			let deps =
				crate::rpc::FullDeps { client: client.clone(), pool: pool.clone(), deny_unsafe };

			Ok(crate::rpc::create_full(deps))
		})
	};

	let keystore_path = config.keystore.path().map(|p| p.to_owned());

	let _rpc_handlers = sc_service::spawn_tasks(sc_service::SpawnTasksParams {
		config,
		client: client.clone(),
		backend,
		task_manager: &mut task_manager,
		keystore: keystore_container.sync_keystore(),
		transaction_pool: transaction_pool.clone(),
		rpc_extensions_builder,
		network: network.clone(),
		system_rpc_tx,
		telemetry: telemetry.as_mut(),
	})?;

	if role.is_authority() {

		// If author string was passed in the CLI - decode it
		// Else - generate new key pair
		let author = decode_author(author, keystore_container.sync_keystore(), keystore_path)?;
		let algorithm = RandomXAlgorithm::new(client.clone());
		
		let proposer = sc_basic_authorship::ProposerFactory::new(
			task_manager.spawn_handle(),
			client.clone(),
			transaction_pool.clone(),
			prometheus_registry.as_ref(),
			telemetry.as_ref().map(|x| x.handle()),
		);

		// `worker` allows quering the current mining metadata and submitting mined blocks
		// `worker_task` is a future which must be polled to fill in information in the worker
		let (worker, worker_task) = crain_pow_consensus::start_mining_worker(
			Box::new(pow_block_import.clone()),
			client.clone(),
			select_chain.clone(),
			algorithm,
			proposer,
			network.clone(),
			network.clone(),
			Some(author.encode()),
			InherentDataProvidersBuilder,
			Duration::new(10, 0),
			Duration::new(10, 0),
			sp_consensus::CanAuthorWithNativeVersion::new(client.executor().clone()),
		);


		task_manager
		// TODO add essential here?
			.spawn_handle()
			.spawn_blocking("pow", Some("block-authoring"), worker_task);

		let stats = Arc::new(Mutex::new(crain_pow::Stats::new()));
				for _ in 0..threads {
			if let Some(keystore) = keystore_container.local_keystore() {
				let worker = worker.clone();
				let client = client.clone();
				let stats = stats.clone();

				thread::spawn(move || loop {
					let metadata = worker.metadata();
					if let Some(metadata) = metadata {
						match crain_pow::mine(
							client.as_ref(),
							&keystore,
							&BlockId::Hash(metadata.best_hash),
							&metadata.pre_hash,
							metadata.pre_runtime.as_ref().map(|v| &v[..]),
							metadata.difficulty,
							round,
							&stats,
						) {
							Ok(Some(seal)) => {
								let current_metadata = worker.metadata();
								if current_metadata == Some(metadata) {
									let _ = futures::executor::block_on(worker.submit(seal));
								}
							}
							Ok(None) => (),
							Err(PowError::Compute(ComputeError::CacheNotAvailable)) => {
								thread::sleep(Duration::new(1, 0));
							}
							Err(PowError::Compute(ComputeError::Randomx(
								err @ RandomxError::CacheAllocationFailed,
							))) => {
								warn!("Mining failed: {}", err.description());
								thread::sleep(Duration::new(10, 0));
							}
							Err(err) => {
								warn!("Mining failed: {:?}", err);
							}
						}
					} else {
						thread::sleep(Duration::new(1, 0));
					}
				});
			} else {
				warn!("Local keystore is not available");
			}
		}

	}

	network_starter.start_network();
	Ok(task_manager)
}
