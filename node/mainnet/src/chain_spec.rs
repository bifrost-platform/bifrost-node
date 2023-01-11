use bifrost_mainnet_runtime::{
	opaque::SessionKeys, AccountId, Balance, InflationInfo, Precompiles, Range, WASM_BINARY,
};

use bifrost_mainnet_constants::currency::{GWEI, SUPPLY_FACTOR, UNITS as BFC};
use bifrost_mainnet_runtime as mainnet;

use pallet_im_online::sr25519::AuthorityId as ImOnlineId;
use sc_service::ChainType;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_core::{Pair, Public};
use sp_finality_grandpa::AuthorityId as GrandpaId;
use sp_runtime::Perbill;

use std::collections::BTreeMap;

use hex_literal::hex;

use crate::test_accounts::{
	get_council_member_accounts, get_master_account, get_registrar_account, get_sudo_account,
	get_technical_committee_member_accounts,
};

/// Specialized `ChainSpec`. This is a specialization of the general Substrate ChainSpec type.
pub type ChainSpec = sc_service::GenericChainSpec<mainnet::GenesisConfig>;

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

pub fn mainnet_config() -> Result<ChainSpec, String> {
	let wasm_binary = WASM_BINARY.ok_or_else(|| "Mainnet wasm not available".to_string())?;

	// The following accounts are all dummy data for tests
	let genesis_stash = AccountId::from(hex!("6a0bD757b3FE28A1a665CC33cBD621B99240122c"));
	let genesis_controller = AccountId::from(hex!("90082BBFed3ff32d5ac604b01C34F09E233C393b"));
	let genesis_relayer = AccountId::from(hex!("6ffb96F8B765e5DCD2Bf0b485fC1C2e9c3746FEf"));
	let session_key_inspector =
		"0x757cd694d91f412d5388857bab4b85b542df8f6cb9eee7a9a82d1b76f235b4f2";

	let mut prefunded_accounts = vec![
		genesis_stash,
		genesis_controller,
		genesis_relayer,
		get_sudo_account(),
		get_registrar_account(),
		get_master_account(),
	];
	get_council_member_accounts().iter().for_each(|account| {
		prefunded_accounts.push(*account);
	});
	get_technical_committee_member_accounts().iter().for_each(|account| {
		prefunded_accounts.push(*account);
	});

	Ok(ChainSpec::from_genesis(
		// Name
		"BIFROST Mainnet",
		// ID
		"mainnet",
		ChainType::Live,
		move || {
			mainnet_genesis(
				wasm_binary,
				// Validator candidates
				vec![(
					// Stash account
					genesis_stash,
					// Controller account
					genesis_controller,
					// Relayer account
					genesis_relayer,
					inspect_key::<AuraId>(session_key_inspector),
					inspect_key::<GrandpaId>(session_key_inspector),
					inspect_key::<ImOnlineId>(session_key_inspector),
					4_000_000 * SUPPLY_FACTOR * BFC,
				)],
				// Nominations
				vec![],
				// Council Members
				get_council_member_accounts(),
				// Technical Committee Members
				get_technical_committee_member_accounts(),
				// Sudo account
				get_sudo_account(),
				// Pre-funded accounts
				prefunded_accounts.clone(),
				true,
			)
		},
		// Bootnodes
		vec![],
		// Telemetry
		None,
		// Protocol ID
		None,
		// Fork ID
		None,
		// Properties
		Some(
			serde_json::from_str("{\"tokenDecimals\": 18, \"tokenSymbol\": \"BFC\"}")
				.expect("Provided valid json map"),
		),
		// Extensions
		None,
	))
}

/// Configure initial storage state for FRAME modules.
fn mainnet_genesis(
	wasm_binary: &[u8],
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
	root_key: AccountId,
	endowed_accounts: Vec<AccountId>,
	_enable_println: bool,
) -> mainnet::GenesisConfig {
	// This is the simplest bytecode to revert without returning any data.
	// We will pre-deploy it under all of our precompiles to ensure they can be called from
	// within contracts.
	// (PUSH1 0x00 PUSH1 0x00 REVERT)
	let revert_bytecode = vec![0x60, 0x00, 0x60, 0x00, 0xFD];
	mainnet::GenesisConfig {
		system: mainnet::SystemConfig {
			// Add Wasm runtime to storage.
			code: wasm_binary.to_vec(),
		},
		balances: mainnet::BalancesConfig {
			balances: {
				endowed_accounts
					.iter()
					.enumerate()
					.map(|(idx, account)| {
						if idx == 0 {
							// genesis stash
							(*account, 4_010_000 * SUPPLY_FACTOR * BFC)
						} else if idx == 1 {
							// genesis controller
							(*account, 10_000 * SUPPLY_FACTOR * BFC)
						} else if idx == 2 {
							// genesis relayer
							(*account, 10_000 * SUPPLY_FACTOR * BFC)
						} else if idx == 3 {
							// sudo
							(*account, 10_000 * SUPPLY_FACTOR * BFC)
						} else if idx == 4 {
							// registrar
							(*account, 10_000 * SUPPLY_FACTOR * BFC)
						} else if idx > 5 && idx < 16 {
							// council & tech
							(*account, 30_000 * SUPPLY_FACTOR * BFC)
						} else {
							// master
							(*account, (746_271_000 + 100_000_000) * SUPPLY_FACTOR * BFC)
						}
					})
					.collect::<Vec<_>>()
			},
		},
		session: mainnet::SessionConfig {
			keys: initial_validators
				.iter()
				.map(|x| {
					(x.1.clone(), x.1.clone(), session_keys(x.3.clone(), x.4.clone(), x.5.clone()))
				})
				.collect::<Vec<_>>(),
		},
		aura: Default::default(),
		grandpa: Default::default(),
		im_online: Default::default(),
		sudo: mainnet::SudoConfig { key: Some(root_key) },
		transaction_payment: Default::default(),
		evm: mainnet::EVMConfig {
			accounts: {
				let accounts: BTreeMap<_, _> = Precompiles::used_addresses()
					.map(|addr| {
						(
							addr,
							pallet_evm::GenesisAccount {
								nonce: Default::default(),
								balance: Default::default(),
								storage: Default::default(),
								code: revert_bytecode.clone(),
							},
						)
					})
					.collect();
				accounts
			},
		},
		ethereum: Default::default(),
		base_fee: mainnet::BaseFeeConfig::new(
			sp_core::U256::from(1_000 * GWEI * SUPPLY_FACTOR),
			false,
			sp_runtime::Permill::from_parts(125_000),
		),
		relay_manager: Default::default(),
		bfc_staking: mainnet::BfcStakingConfig {
			candidates: initial_validators
				.iter()
				.cloned()
				.map(|(stash, controller, relayer, _, _, _, bond)| {
					(stash, controller, relayer, bond)
				})
				.collect(),
			nominations: initial_nominators,
			inflation_config: inflation_config(),
		},
		bfc_utility: Default::default(),
		bfc_offences: Default::default(),
		democracy: Default::default(),
		council: Default::default(),
		technical_committee: Default::default(),
		council_membership: mainnet::CouncilMembershipConfig {
			phantom: Default::default(),
			members: initial_council_members.clone(),
		},
		technical_membership: mainnet::TechnicalMembershipConfig {
			phantom: Default::default(),
			members: initial_tech_committee_members.clone(),
		},
		treasury: Default::default(),
	}
}
