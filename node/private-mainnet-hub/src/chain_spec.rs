use bifrost_private_mainnet_hub_runtime::{opaque::SessionKeys, AccountId, WASM_BINARY};

use bifrost_private_mainnet_hub_constants::currency::{GWEI, SUPPLY_FACTOR, UNITS as BFC};
use bifrost_private_mainnet_hub_runtime as private_mainnet_hub;

use fp_evm::GenesisAccount;
use pallet_im_online::sr25519::AuthorityId as ImOnlineId;
use sc_chain_spec::Properties;
use sc_service::ChainType;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_consensus_grandpa::AuthorityId as GrandpaId;
use sp_core::{Pair, Public, H160};

use hex_literal::hex;
use std::collections::BTreeMap;

/// Specialized `ChainSpec`. This is a specialization of the general Substrate ChainSpec type.
pub type ChainSpec = sc_service::GenericChainSpec;

/// Generate a crypto pair from seed.
pub fn get_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
	TPublic::Pair::from_string(&format!("//{}", seed), None)
		.expect("static values are valid; qed")
		.public()
}

fn session_keys(aura: AuraId, grandpa: GrandpaId, im_online: ImOnlineId) -> SessionKeys {
	SessionKeys { aura, grandpa, im_online }
}

fn properties() -> Properties {
	let mut properties = Properties::new();
	properties.insert("tokenDecimals".into(), 18.into());
	properties.insert("tokenSymbol".into(), "BFC".into());
	properties
}

/// Local template for the private mainnet hub chain.
///
/// Uses the well-known `//Alice` session keys and placeholder validator/relayer accounts — a real deployment must generate its own keys and replace the
/// accounts below before building the raw chain spec (`./specs/bifrost-private-mainnet-hub.json`).
pub fn private_mainnet_hub_config() -> ChainSpec {
	ChainSpec::builder(WASM_BINARY.expect("WASM not available"), Default::default())
		.with_name("Bifrost Private Mainnet Hub")
		.with_id("private-mainnet-hub")
		.with_chain_type(ChainType::Live)
		.with_properties(properties())
		.with_genesis_config_patch(private_mainnet_hub_genesis(
			// Authorities: (validator, relayer, session keys)
			vec![(
				// Validator account
				AccountId::from(hex!("f24FF3a9CF04c71Dbc94D0b566f7A27B94566cac")),
				// Relayer account
				AccountId::from(hex!("d6D3f3a35Fab64F69b7885D6162e81B62e44bF58")),
				get_from_seed::<AuraId>("Alice"),
				get_from_seed::<GrandpaId>("Alice"),
				get_from_seed::<ImOnlineId>("Alice"),
			)],
			// Sudo account
			AccountId::from(hex!("f24FF3a9CF04c71Dbc94D0b566f7A27B94566cac")),
			// Pre-funded accounts
			vec![
				// Validator accounts
				AccountId::from(hex!("f24FF3a9CF04c71Dbc94D0b566f7A27B94566cac")),
				// Relayer accounts
				AccountId::from(hex!("d6D3f3a35Fab64F69b7885D6162e81B62e44bF58")),
			],
		))
		.build()
}

/// Configure initial storage state for FRAME modules.
fn private_mainnet_hub_genesis(
	initial_authorities: Vec<(AccountId, AccountId, AuraId, GrandpaId, ImOnlineId)>,
	root_key: AccountId,
	endowed_accounts: Vec<AccountId>,
) -> serde_json::Value {
	let revert_bytecode = vec![0x60, 0x00, 0x60, 0x00, 0xFD];

	serde_json::json!({
		"balances": {
			"balances": endowed_accounts
				.iter()
				.cloned()
				.map(|k| (k, 10_000_000 * BFC))
				.collect::<Vec<_>>()
		},
		"session": {
			"keys": initial_authorities
				.iter()
				.map(|x| {
					(x.0.clone(), x.0.clone(), session_keys(x.2.clone(), x.3.clone(), x.4.clone()))
				})
				.collect::<Vec<_>>()
		},
		"sudo": {
			"key": Some(root_key)
		},
		"evm": {
			"accounts":
				// We need _some_ code inserted at the precompile address so that
				// the evm will actually call the address.
				private_mainnet_hub::Precompiles::used_addresses()
					.map(|addr| {
						(
							addr.into(),
							GenesisAccount {
								nonce: Default::default(),
								balance: Default::default(),
								storage: Default::default(),
								code: revert_bytecode.clone(),
							},
						)
					})
					.collect::<BTreeMap<H160, GenesisAccount>>()
		},
		"baseFee": {
			"baseFeePerGas": sp_core::U256::from(1_000 * GWEI * SUPPLY_FACTOR),
			"elasticity": sp_runtime::Permill::zero()
		},
		"permissionedAuthority": {
			"authorities": initial_authorities
				.iter()
				.map(|(validator, relayer, _, _, _)| (validator.clone(), relayer.clone()))
				.collect::<Vec<_>>()
		}
	})
}
