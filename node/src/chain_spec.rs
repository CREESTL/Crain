use crain_runtime::{
	AccountId, BalancesConfig, DifficultyConfig, GenesisConfig, Signature, SudoConfig,
	SystemConfig, WASM_BINARY,
};
use sc_service::ChainType;
use sp_core::{sr25519, Pair, Public, U256};
use sp_runtime::traits::{IdentifyAccount, Verify};

// The URL for the telemetry server.
// const STAGING_TELEMETRY_URL: &str = "wss://telemetry.polkadot.io/submit/";

/// Specialized `ChainSpec`. This is a specialization of the general Substrate ChainSpec type.
pub type ChainSpec = sc_service::GenericChainSpec<GenesisConfig>;

/// Generate a crypto pair from seed.
pub fn get_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
	TPublic::Pair::from_string(&format!("//{}", seed), None)
		.expect("static values are valid; qed")
		.public()
}



/// Configure initial storage state for FRAME modules.
/// Used a genesis constructor in the functions below
// fn testnet_genesis(
// 	wasm_binary: &[u8],
// 	root_key: AccountId,
// 	endowed_accounts: Vec<AccountId>,
// 	_enable_println: bool,
// ) -> GenesisConfig {
// 	GenesisConfig {
// 		system: SystemConfig {
// 			// Add Wasm runtime to storage.
// 			code: wasm_binary.to_vec(),
// 		},
// 		balances: BalancesConfig {
// 			// Configure endowed accounts with initial balance of 1 << 60.
// 			balances: endowed_accounts.iter().cloned().map(|k| (k, 1 << 60)).collect(),
// 		},
// 		sudo: SudoConfig {
// 			// Assign network admin rights.
// 			key: Some(root_key),
// 		},
// 		transaction_payment: Default::default(),
// 		// TODO put this U256 into arguments
// 		// Define genesis configuration of difficulty pallet that forms a global chain genesis
// 		// TODO any other value in the string here?
// 		difficulty: DifficultyConfig { initial_difficulty: sp_core::U256::from_dec_str("1401562").unwrap()},
// 	}
// }

type AccountPublic = <Signature as Verify>::Signer;

/// Generate an account ID from seed.
pub fn get_account_id_from_seed<TPublic: Public>(seed: &str) -> AccountId
where
	AccountPublic: From<<TPublic::Pair as Pair>::Public>,
{
	AccountPublic::from(get_from_seed::<TPublic>(seed)).into_account()
}

// Helper function to generate basic genesis config for more complicated configs
fn testnet_genesis(
	// Code
	wasm_binary: &[u8],
	// Initial difficulty for `difficulty` pallet
	initial_difficulty: U256,
	// Prefunded accounts
	endowed_accounts: Vec<AccountId>,
	_enable_println: bool,
) -> GenesisConfig {
	GenesisConfig {
		system: SystemConfig {
			code: wasm_binary.to_vec(),
		},
		balances: BalancesConfig {
			// Configure endowed accounts with initial balance of 1 << 60.
			balances: endowed_accounts
			.iter()
			.cloned()
			.map(|k| (k, 1 << 60))
			.collect(),
		},
		difficulty: DifficultyConfig { initial_difficulty },
		..Default::default()
	}
}

pub fn development_config() -> Result<ChainSpec, String> {
	let wasm_binary = WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?;

	Ok(ChainSpec::from_genesis(
		// Name
		"Development",
		// ID
		"dev",
		ChainType::Development,
		move || {
			testnet_genesis(
				wasm_binary,
				U256::from(1000),
				vec![
					get_account_id_from_seed::<sr25519::Public>("Alice"),
					get_account_id_from_seed::<sr25519::Public>("Bob"),
					get_account_id_from_seed::<sr25519::Public>("Alice//stash"),
					get_account_id_from_seed::<sr25519::Public>("Bob//stash"),
				],
				true,
			)
		},
		// Bootnodes
		// TODO change properties like in kulupu
		vec![],
		// Telemetry
		None,
		// Protocol ID
		None,
		None,
		// Properties
		None,
		// Extensions
		None,
	))
}

pub fn local_testnet_config() -> Result<ChainSpec, String> {
	let wasm_binary = WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?;

	Ok(ChainSpec::from_genesis(
		// Name
		"Local Testnet",
		// ID
		"local_testnet",
		ChainType::Local,
		move || {
			testnet_genesis(
				wasm_binary,
				U256::from(200),
				vec![
					get_account_id_from_seed::<sr25519::Public>("Alice"),
					get_account_id_from_seed::<sr25519::Public>("Bob"),
					get_account_id_from_seed::<sr25519::Public>("Charlie"),
					get_account_id_from_seed::<sr25519::Public>("Dave"),
					get_account_id_from_seed::<sr25519::Public>("Eve"),
					get_account_id_from_seed::<sr25519::Public>("Ferdie"),
					get_account_id_from_seed::<sr25519::Public>("Alice//stash"),
					get_account_id_from_seed::<sr25519::Public>("Bob//stash"),
					get_account_id_from_seed::<sr25519::Public>("Charlie//stash"),
					get_account_id_from_seed::<sr25519::Public>("Dave//stash"),
					get_account_id_from_seed::<sr25519::Public>("Eve//stash"),
					get_account_id_from_seed::<sr25519::Public>("Ferdie//stash"),
				],
				false,
			)
		},
		// Bootnodes
		vec![],
		// Telemetry
		None,
		// Protocol ID
		None,
		// Properties
		None,
		None,
		// Extensions
		None,
	))
}