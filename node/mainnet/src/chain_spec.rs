use bifrost_mainnet_runtime::{
	opaque::SessionKeys, AccountId, Balance, InflationInfo, Range, WASM_BINARY,
};

use bifrost_mainnet_constants::currency::{GWEI, SUPPLY_FACTOR, UNITS as BFC};
use bifrost_mainnet_runtime as mainnet;

use fp_evm::GenesisAccount;
use pallet_im_online::sr25519::AuthorityId as ImOnlineId;
use sc_chain_spec::Properties;
use sc_service::ChainType;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_consensus_grandpa::AuthorityId as GrandpaId;
use sp_core::{Pair, Public, H160};
use sp_runtime::Perbill;

use hex_literal::hex;
use std::collections::BTreeMap;

/// Specialized `ChainSpec`. This is a specialization of the general Substrate ChainSpec type.
pub type ChainSpec = sc_service::GenericChainSpec;

/// Generate a crypto pair from key.
pub fn inspect_key<TPublic: Public>(key: &str) -> <TPublic::Pair as Pair>::Public {
	TPublic::Pair::from_string(&format!("//{}", key), None)
		.expect("static values are valid; qed")
		.public()
}

/// Generate a crypto pair from seed.
pub fn get_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
	TPublic::Pair::from_string(&format!("//{}", seed), None)
		.expect("static values are valid; qed")
		.public()
}

fn session_keys(aura: AuraId, grandpa: GrandpaId, im_online: ImOnlineId) -> SessionKeys {
	SessionKeys { aura, grandpa, im_online }
}

pub fn inflation_config() -> InflationInfo<Balance> {
	fn to_round_inflation(annual: Range<Perbill>) -> Range<Perbill> {
		use pallet_bfc_staking::inflation::{perbill_annual_to_perbill_round, BLOCKS_PER_YEAR};
		perbill_annual_to_perbill_round(
			annual,
			BLOCKS_PER_YEAR / bifrost_mainnet_runtime::DefaultBlocksPerRound::get(),
		)
	}
	let annual = Range {
		min: Perbill::from_percent(13),
		ideal: Perbill::from_percent(13),
		max: Perbill::from_percent(13),
	};
	InflationInfo {
		// staking expectations
		expect: Range {
			min: 5_000 * BFC * SUPPLY_FACTOR,
			ideal: 10_000 * BFC * SUPPLY_FACTOR,
			max: 50_000 * BFC * SUPPLY_FACTOR,
		},
		// annual inflation
		annual,
		round: to_round_inflation(annual),
	}
}

fn properties() -> Properties {
	let mut properties = Properties::new();
	properties.insert("tokenDecimals".into(), 18.into());
	properties.insert("tokenSymbol".into(), "BFC".into());
	properties
}

pub fn mainnet_config() -> ChainSpec {
	ChainSpec::builder(WASM_BINARY.expect("WASM not available"), Default::default())
		.with_name("Bifrost Mainnet")
		.with_id("mainnet")
		.with_chain_type(ChainType::Live)
		.with_properties(properties())
		.with_genesis_config_patch(mainnet_genesis(
			// Validator candidates
			vec![(
				// Stash account
				AccountId::from(hex!("912F9D002E46DF70C78495D29Faa523c2c0382a2")),
				// Controller account
				AccountId::from(hex!("f24FF3a9CF04c71Dbc94D0b566f7A27B94566cac")),
				// Relayer account
				AccountId::from(hex!("d6D3f3a35Fab64F69b7885D6162e81B62e44bF58")),
				get_from_seed::<AuraId>("Alice"),
				get_from_seed::<GrandpaId>("Alice"),
				get_from_seed::<ImOnlineId>("Alice"),
				4_000_000 * BFC * SUPPLY_FACTOR,
			)],
			// Nominations
			vec![],
			// Council Members
			vec![
				AccountId::from(hex!("f24FF3a9CF04c71Dbc94D0b566f7A27B94566cac")),
				AccountId::from(hex!("3Cd0A705a2DC65e5b1E1205896BaA2be8A07c6e0")),
				AccountId::from(hex!("798d4Ba9baf0064Ec19eB4F0a1a45785ae9D6DFc")),
			],
			// Technical Committee Members
			vec![
				AccountId::from(hex!("f24FF3a9CF04c71Dbc94D0b566f7A27B94566cac")),
				AccountId::from(hex!("3Cd0A705a2DC65e5b1E1205896BaA2be8A07c6e0")),
				AccountId::from(hex!("798d4Ba9baf0064Ec19eB4F0a1a45785ae9D6DFc")),
			],
			// Relay Executives
			vec![AccountId::from(hex!("d6D3f3a35Fab64F69b7885D6162e81B62e44bF58"))],
			// Sudo account
			AccountId::from(hex!("f24FF3a9CF04c71Dbc94D0b566f7A27B94566cac")),
			// Socket queue authority
			AccountId::from(hex!("f24FF3a9CF04c71Dbc94D0b566f7A27B94566cac")),
			// Pre-funded accounts
			vec![
				// Stash accounts
				AccountId::from(hex!("912F9D002E46DF70C78495D29Faa523c2c0382a2")),
				// Controller accounts
				AccountId::from(hex!("f24FF3a9CF04c71Dbc94D0b566f7A27B94566cac")),
				// Relayer accounts
				AccountId::from(hex!("d6D3f3a35Fab64F69b7885D6162e81B62e44bF58")),
			],
		))
		.build()
}

/// Configure initial storage state for FRAME modules.
fn mainnet_genesis(
	initial_validators: Vec<(
		AccountId,
		AccountId,
		AccountId,
		AuraId,
		GrandpaId,
		ImOnlineId,
		Balance,
	)>,
	initial_nominators: Vec<(AccountId, AccountId, Balance)>,
	initial_council_members: Vec<AccountId>,
	initial_tech_committee_members: Vec<AccountId>,
	initial_relay_executives: Vec<AccountId>,
	root_key: AccountId,
	authority: AccountId,
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
			"keys": initial_validators
				.iter()
				.map(|x| {
					(x.1.clone(), x.1.clone(), session_keys(x.3.clone(), x.4.clone(), x.5.clone()))
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
				mainnet::Precompiles::used_addresses()
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
		"bfcStaking": {
			"candidates": initial_validators
				.iter()
				.cloned()
				.map(|(stash, controller, relayer, _, _, _, bond)| {
					(stash, controller, relayer, bond)
				})
				.collect::<Vec<_>>(),
			"nominations": initial_nominators,
			"inflationConfig": inflation_config()
		},
		"councilMembership": {
			"members": initial_council_members.clone()
		},
		"technicalMembership": {
			"members": initial_tech_committee_members.clone()
		},
		"relayExecutiveMembership": {
			"members": initial_relay_executives.clone()
		},
		"btcSocketQueue": {
			"authority": Some(authority)
		}
	})
}
