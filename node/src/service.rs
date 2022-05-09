//! Service and ServiceFactory implementation. Specialized wrapper over substrate service.

use crain_runtime::{self, opaque::Block, RuntimeApi};
use sc_client_api::{BlockBackend, ExecutorProvider};
pub use sc_executor::NativeElseWasmExecutor;
use sc_finality_grandpa::SharedVoterState;
use sc_keystore::LocalKeystore;
use sc_service::{error::Error as ServiceError, Configuration, TaskManager};
use sc_telemetry::{Telemetry, TelemetryWorker};
use sha3::Sha3_224;
use sp_api::ProvideRuntimeApi;
use async_trait::async_trait;
use super::sha3pow::Sha3Algorithm;
use sp_timestamp::InherentDataProvider;
use sp_inherents::CreateInherentDataProviders;
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

type FullClient =
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
		// TODO delete grandpa here?
		(
			sc_finality_grandpa::GrandpaBlockImport<
				FullBackend,
				Block,
				FullClient,
				FullSelectChain,
			>,
			sc_finality_grandpa::LinkHalf<Block, FullClient, FullSelectChain>,
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

	let (grandpa_block_import, grandpa_link) = sc_finality_grandpa::block_import(
		client.clone(),
		&(client.clone() as Arc<_>),
		select_chain.clone(),
		telemetry.as_ref().map(|x| x.handle()),
	)?;


	let can_author_with = sp_consensus::CanAuthorWithNativeVersion::new(client.executor().clone());

	let algorithm = Sha3Algorithm::new(client.clone());

	let pow_block_import = sc_consensus_pow::PowBlockImport::new(
		client.clone(),
		client.clone(),
		algorithm.clone(),
		0,
		select_chain.clone(),
		InherentDataProvidersBuilder,
		can_author_with,
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
		// TODO more arguments that needed?
		task_manager,
		keystore_container,
		select_chain,
		import_queue,
		transaction_pool,
		// TODO delete telemetry?
		other: (pow_block_import, telemetry),
	})
}


/// Builds a new service for a full client.
pub fn new_full(mut config: Configuration) -> Result<TaskManager, ServiceError> {
	let sc_service::PartialComponents {
		client,
		backend,
		mut task_manager,
		import_queue,
		mut keystore_container,
		select_chain,
		transaction_pool,
		other: (pow_block_import, grandpa_link, mut telemetry),
	} = new_partial(&config)?;


	let grandpa_protocol_name = sc_finality_grandpa::protocol_standard_name(
		&client.block_hash(0).ok().flatten().expect("Genesis block exists; qed"),
		&config.chain_spec,
	);

	config
		.network
		.extra_sets
		.push(sc_finality_grandpa::grandpa_peers_set_config(grandpa_protocol_name.clone()));
	let warp_sync = Arc::new(sc_finality_grandpa::warp_proof::NetworkProvider::new(
		backend.clone(),
		grandpa_link.shared_authority_set().clone(),
		Vec::default(),
	));

	let (network, system_rpc_tx, network_starter) =
		sc_service::build_network(sc_service::BuildNetworkParams {
			config: &config,
			client: client.clone(),
			transaction_pool: transaction_pool.clone(),
			spawn_handle: task_manager.spawn_handle(),
			import_queue,
			block_announce_validator_builder: None,
			warp_sync: Some(warp_sync),
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
	let force_authoring = config.force_authoring;
	let backoff_authoring_blocks: Option<()> = None;
	let name = config.network.node_name.clone();
	let enable_grandpa = !config.disable_grandpa;
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

	let _rpc_handlers = sc_service::spawn_tasks(sc_service::SpawnTasksParams {
		network: network.clone(),
		client: client.clone(),
		keystore: keystore_container.sync_keystore(),
		task_manager: &mut task_manager,
		transaction_pool: transaction_pool.clone(),
		rpc_extensions_builder,
		backend,
		system_rpc_tx,
		config,
		telemetry: telemetry.as_mut(),
	})?;

	if role.is_authority() {
		let proposer_factory = sc_basic_authorship::ProposerFactory::new(
			task_manager.spawn_handle(),
			client.clone(),
			transaction_pool,
			prometheus_registry.as_ref(),
			telemetry.as_ref().map(|x| x.handle()),
		);

		let can_author_with =
			sp_consensus::CanAuthorWithNativeVersion::new(client.executor().clone());


		// the AURA authoring task is considered essential, i.e. if it
		// fails we take down the service with it.
		task_manager
			.spawn_essential_handle()
			.spawn_blocking("pow", Some("block-authoring"), pow);
	}

	// if the node isn't actively participating in consensus then it doesn't
	// need a keystore, regardless of which protocol we use below.
	let keystore =
		if role.is_authority() { Some(keystore_container.sync_keystore()) } else { None };

	let grandpa_config = sc_finality_grandpa::Config {
		// FIXME #1578 make this available through chainspec
		gossip_duration: Duration::from_millis(333),
		justification_period: 512,
		name: Some(name),
		observer_enabled: false,
		keystore,
		local_role: role,
		telemetry: telemetry.as_ref().map(|x| x.handle()),
		protocol_name: grandpa_protocol_name,
	};

	if enable_grandpa {
		// start the full GRANDPA voter
		// NOTE: non-authorities could run the GRANDPA observer protocol, but at
		// this point the full voter should provide better guarantees of block
		// and vote data availability than the observer. The observer has not
		// been tested extensively yet and having most nodes in a network run it
		// could lead to finality stalls.
		let grandpa_config = sc_finality_grandpa::GrandpaParams {
			config: grandpa_config,
			link: grandpa_link,
			network,
			voting_rule: sc_finality_grandpa::VotingRulesBuilder::default().build(),
			prometheus_registry,
			shared_voter_state: SharedVoterState::empty(),
			telemetry: telemetry.as_ref().map(|x| x.handle()),
		};

		// the GRANDPA voter task is considered infallible, i.e.
		// if it fails we take down the service with it.
		task_manager.spawn_essential_handle().spawn_blocking(
			"grandpa-voter",
			None,
			sc_finality_grandpa::run_grandpa_voter(grandpa_config)?,
		);
	}

	let (worker, worker_task) = sc_consensus_pow::start_mining_worker(
		Box::new(pow_block_import.clone()),
		client.clone(),
		select_chain.clone(),
		algorithm.clone(),
		proposer,
		network.clone(),
		None,
		Some(author.encode()),
		// TODO might be wrong parameter
		InherentDataProvidersBuilder,
		Duration::new(10, 0),
		Duration::new(10, 0),
		sp_consensus::CanAuthorWithNativeVersion::new(client.executor().clone()),
	);

	task_manager
		.spawn_essential_handle()
		.spawn_blocking("pow", worker_task);


	network_starter.start_network();
	Ok(task_manager)
}

// TODO fix it, NO GRANDPA HERE
// TODO Cloned fromn Kulupu
// Builds a new service for a light client.
// pub fn new_light(
// 	config: Configuration,
// 	check_inherents_after: u32,
// 	enable_weak_subjectivity: bool,
// ) -> Result<TaskManager, ServiceError> {
// 	let telemetry = config
// 		.telemetry_endpoints
// 		.clone()
// 		.filter(|x| !x.is_empty())
// 		.map(|endpoints| -> Result<_, sc_telemetry::Error> {
// 			let worker = TelemetryWorker::new(16)?;
// 			let telemetry = worker.handle().new_telemetry(endpoints);
// 			Ok((worker, telemetry))
// 		})
// 		.transpose()?;

// 	let executor = NativeElseWasmExecutor::<ExecutorDispatch>::new(
// 		config.wasm_method,
// 		config.default_heap_pages,
// 		config.max_runtime_instances,
// 	);

// 	let (client, backend, keystore_container, mut task_manager, on_demand) =
// 		sc_service::new_light_parts::<Block, RuntimeApi, _>(
// 			&config,
// 			telemetry.as_ref().map(|(_, telemetry)| telemetry.handle()),
// 			executor,
// 		)?;

// 	let mut telemetry = telemetry.map(|(worker, telemetry)| {
// 		task_manager.spawn_handle().spawn("telemetry", worker.run());
// 		telemetry
// 	});

// 	let transaction_pool = Arc::new(sc_transaction_pool::BasicPool::new_light(
// 		config.transaction_pool.clone(),
// 		config.prometheus_registry(),
// 		task_manager.spawn_essential_handle(),
// 		client.clone(),
// 		on_demand.clone(),
// 	));

// 	let select_chain = sc_consensus::LongestChain::new(backend.clone());

// 	let algorithm = Sha3Algorithm::new(client.clone());

// 	let pow_block_import = sc_consensus_pow::PowBlockImport::new(
// 	client.clone(),
// 	client.clone(),
// 	algorithm.clone(),
// 	0,
// 	select_chain.clone(),
// 	InherentDataProvidersBuilder,
// 	sp_consensus::NeverCanAuthor
// 	);

// 	let boxed_import = Box::new(pow_block_import.clone());

// 	let import_queue = sc_consensus_pow::import_queue(
// 			boxed_import,
// 			None,
// 			algorithm.clone(),
// 			&task_manager.spawn_essential_handle(),
// 			config.prometheus_registry(),
// 		)?;

// 	let (network, system_rpc_tx, network_starter) =
// 		sc_service::build_network(sc_service::BuildNetworkParams {
// 			config: &config,
// 			client: client.clone(),
// 			transaction_pool: transaction_pool.clone(),
// 			spawn_handle: task_manager.spawn_handle(),
// 			import_queue,
// 			block_announce_validator_builder: None,
// 			warp_sync: None,
// 		})?;


// 	if config.offchain_worker.enabled {
// 		sc_service::build_offchain_workers(
// 			&config,
// 			task_manager.spawn_handle(),
// 			client.clone(),
// 			network.clone(),
// 		);
// 	}

// 	sc_service::spawn_tasks(sc_service::SpawnTasksParams {
// 		network: network.clone(),
// 		client: client.clone(),
// 		keystore: keystore_container.sync_keystore(),
// 		task_manager: &mut task_manager,
// 		transaction_pool: transaction_pool.clone(),
// 		rpc_extensions_builder: Box::new(|_, _| Ok(())),
// 		backend, 
// 		system_rpc_tx,
// 		config,
// 		telemetry: telemetry.as_mut(),
// 	})?;

// 	network_starter.start_network();

// 	Ok(task_manager)
// }