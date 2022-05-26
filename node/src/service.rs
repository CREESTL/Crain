//! Service and ServiceFactory implementation. Specialized wrapper over substrate service.

use crain_runtime::{self, opaque::Block, RuntimeApi};
use sc_client_api::ExecutorProvider;
pub use sc_executor::NativeElseWasmExecutor;
use std::str::FromStr;
use sc_service::{error::Error as ServiceError, Configuration, TaskManager};
use sc_telemetry::{Telemetry, TelemetryWorker};
use log::*;
use sp_core::H256;
use parity_scale_codec::Encode;
use sp_core::Pair;
use std::path::PathBuf;
use async_trait::async_trait;
use crain_pow::Sha3Algorithm;
use sp_core::crypto::{Ss58AddressFormat, Ss58AddressFormatRegistry, Ss58Codec, UncheckedFrom};
use sp_keystore::{SyncCryptoStore, SyncCryptoStorePtr};
use sp_runtime::traits::Block as BlockT;
use std::{sync::Arc, time::Duration};


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


/// Returns most parts of a service. Not enough to run a full chain,
// But enough to perform chain operations like purge-chain
pub fn new_partial(
	config: &Configuration,
) -> Result<
	sc_service::PartialComponents<
		FullClient,
		FullBackend,
		FullSelectChain,
		sc_consensus::DefaultImportQueue<Block, FullClient>,
		sc_transaction_pool::FullPool<Block, FullClient>,
		(
			sc_consensus_pow::PowBlockImport<
				Block,
				Arc<FullClient>,
				FullClient,
				FullSelectChain,
				Sha3Algorithm<FullClient>,
				sp_consensus::CanAuthorWithNativeVersion<
					sc_service::LocalCallExecutor<
						Block,
						sc_client_db::Backend<Block>,
						NativeElseWasmExecutor<ExecutorDispatch>,
					>,
				>,
				InherentDataProvidersBuilder,
			>,
			Option<Telemetry>,
		),
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

	let (client, backend, keystore_container, task_manager) =
		sc_service::new_full_parts::<Block, RuntimeApi, _>(
			&config,
			telemetry.as_ref().map(|(_, telemetry)| telemetry.handle()),
			executor,
		)?;

	let client = Arc::new(client);

	let telemetry = telemetry.map(|(worker, telemetry)| {
		task_manager.spawn_handle().spawn("telemetry", None, worker.run());
		telemetry
	});

	let select_chain = sc_consensus::LongestChain::new(backend.clone());

	let transaction_pool = sc_transaction_pool::BasicPool::new_full(
		config.transaction_pool.clone(),
		config.role.is_authority().into(),
		config.prometheus_registry(),
		task_manager.spawn_essential_handle(),
		client.clone(),
	);

	let algorithm = Sha3Algorithm::new(client.clone());

	let pow_block_import = sc_consensus_pow::PowBlockImport::new(
		client.clone(),
		client.clone(),
		algorithm.clone(),
		0,
		select_chain.clone(),
		InherentDataProvidersBuilder,
		sp_consensus::CanAuthorWithNativeVersion::new(client.executor().clone()), 
		);

	let boxed_import = Box::new(pow_block_import.clone());

	let import_queue = sc_consensus_pow::import_queue(
			boxed_import,
			None,
			algorithm.clone(),
			&task_manager.spawn_essential_handle(),
			config.prometheus_registry(),
		)?;


	Ok(sc_service::PartialComponents {
		client,
		backend,
		task_manager,
		keystore_container,
		select_chain,
		import_queue,
		transaction_pool,
		// TODO delete telemetry?
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
		// TODO can I just delete this whole block? 
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
pub fn new_full(config: Configuration, author: Option<&str>) -> Result<TaskManager, ServiceError> {
	let sc_service::PartialComponents {
		client,
		backend,
		mut task_manager,
		keystore_container,
		select_chain,
		import_queue,
		transaction_pool,
		other: (pow_block_import, 
			mut telemetry),
	} = new_partial(&config)?;



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

		let algorithm = Sha3Algorithm::new(client.clone());
		let author = decode_author(author, keystore_container.sync_keystore(), keystore_path)?;
		
		let proposer_factory = sc_basic_authorship::ProposerFactory::new(
			task_manager.spawn_handle(),
			client.clone(),
			transaction_pool,
			prometheus_registry.as_ref(),
			telemetry.as_ref().map(|x| x.handle()),
		);


		let can_author_with = sp_consensus::CanAuthorWithNativeVersion::new(client.executor().clone());


		let (_worker, worker_task) = sc_consensus_pow::start_mining_worker(
			Box::new(pow_block_import.clone()),
			client.clone(),
			select_chain.clone(),
			algorithm,
			proposer_factory,
			network.clone(),
			network.clone(),
			Some(author.encode()), // Include authorship into block
			// TODO might be wrong parameter
			InherentDataProvidersBuilder,
			// Time to wait for a new block before starting to mine a new one
			Duration::new(10, 0),
			// how long to take to actually build the block (i.e. executing extrinsics)
			Duration::new(10, 0),
			can_author_with
		);

		// the AURA authoring task is considered essential, i.e. if it
		// fails we take down the service with it.
		task_manager
			.spawn_essential_handle()
			.spawn_blocking("pow", Some("block-authoring"), worker_task);
	}

	network_starter.start_network();
	Ok(task_manager)
}
