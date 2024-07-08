use bifrost_testnet_runtime::{
	opaque::SessionKeys, AccountId, Balance, InflationInfo, Range, WASM_BINARY,
};

use bifrost_testnet_constants::currency::{GWEI, SUPPLY_FACTOR, UNITS as BFC};
use bifrost_testnet_runtime as testnet;

use fp_evm::GenesisAccount;
use pallet_im_online::sr25519::AuthorityId as ImOnlineId;
use sc_service::ChainType;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_consensus_grandpa::AuthorityId as GrandpaId;
use sp_core::{Pair, Public};
use sp_runtime::{BoundedVec, Perbill};

use hex_literal::hex;

/// Specialized `ChainSpec`. This is a specialization of the general Substrate ChainSpec type.
pub type ChainSpec = sc_service::GenericChainSpec<testnet::RuntimeGenesisConfig>;

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
			BLOCKS_PER_YEAR / bifrost_testnet_runtime::DefaultBlocksPerRound::get(),
		)
	}
	let annual = Range {
		min: Perbill::from_percent(100),
		ideal: Perbill::from_percent(100),
		max: Perbill::from_percent(100),
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

pub fn testnet_config() -> Result<ChainSpec, String> {
	let wasm_binary = WASM_BINARY.ok_or_else(|| "Testnet wasm not available".to_string())?;

	Ok(ChainSpec::from_genesis(
		// Name
		"Bifrost Testnet",
		// ID
		"testnet",
		ChainType::Live,
		move || {
			testnet_genesis(
				wasm_binary,
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
					100_000 * BFC * SUPPLY_FACTOR,
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
fn testnet_genesis(
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
	initial_relay_executives: Vec<AccountId>,
	root_key: AccountId,
	authority: AccountId,
	endowed_accounts: Vec<AccountId>,
) -> testnet::RuntimeGenesisConfig {
	let revert_bytecode = vec![0x60, 0x00, 0x60, 0x00, 0xFD];
	testnet::RuntimeGenesisConfig {
		system: testnet::SystemConfig {
			// Add Wasm runtime to storage.
			code: wasm_binary.to_vec(),
			..Default::default()
		},
		balances: testnet::BalancesConfig {
			balances: endowed_accounts.iter().cloned().map(|k| (k, 200_000 * BFC)).collect(),
		},
		session: testnet::SessionConfig {
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
		sudo: testnet::SudoConfig { key: Some(root_key) },
		transaction_payment: Default::default(),
		evm: testnet::EVMConfig {
			// We need _some_ code inserted at the precompile address so that
			// the evm will actually call the address.
			accounts: testnet::Precompiles::used_addresses()
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
				.collect(),
			..Default::default()
		},
		ethereum: Default::default(),
		base_fee: testnet::BaseFeeConfig::new(
			sp_core::U256::from(1_000 * GWEI * SUPPLY_FACTOR),
			sp_runtime::Permill::zero(),
		),
		relay_manager: Default::default(),
		bfc_staking: testnet::BfcStakingConfig {
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
		relay_executive: Default::default(),
		council_membership: testnet::CouncilMembershipConfig {
			phantom: Default::default(),
			members: BoundedVec::try_from(initial_council_members.clone())
				.expect("Membership must be initialized."),
		},
		technical_membership: testnet::TechnicalMembershipConfig {
			phantom: Default::default(),
			members: BoundedVec::try_from(initial_tech_committee_members.clone())
				.expect("Membership must be initialized"),
		},
		relay_executive_membership: testnet::RelayExecutiveMembershipConfig {
			phantom: Default::default(),
			members: BoundedVec::try_from(initial_relay_executives.clone())
				.expect("Membership must be initialized"),
		},
		treasury: Default::default(),
		btc_registration_pool: Default::default(),
		btc_socket_queue: testnet::BtcSocketQueueConfig {
			authority: Some(authority),
			..Default::default()
		},
	}
}
