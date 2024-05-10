use bifrost_dev_runtime::{
	opaque::SessionKeys, AccountId, Balance, InflationInfo, Range, WASM_BINARY,
};

use bifrost_dev_constants::currency::{GWEI, SUPPLY_FACTOR, UNITS as BFC};

use bifrost_dev_runtime as devnet;
use bifrost_dev_runtime::EVMConfig;
use bifrost_dev_runtime::Precompiles;
use fp_evm::GenesisAccount;

use pallet_im_online::sr25519::AuthorityId as ImOnlineId;
use sc_service::ChainType;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_consensus_grandpa::AuthorityId as GrandpaId;
use sp_core::{Pair, Public};
use sp_runtime::{BoundedVec, Perbill};

use hex_literal::hex;

/// Specialized `ChainSpec`. This is a specialization of the general Substrate ChainSpec type.
pub type ChainSpec = sc_service::GenericChainSpec<devnet::RuntimeGenesisConfig>;

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
			BLOCKS_PER_YEAR / bifrost_dev_runtime::DefaultBlocksPerRound::get(),
		)
	}
	let annual = Range {
		min: Perbill::from_percent(70),
		ideal: Perbill::from_percent(130),
		max: Perbill::from_percent(150),
	};
	InflationInfo {
		// staking expectations
		expect: Range {
			min: 1_000 * BFC * SUPPLY_FACTOR,
			ideal: 2_000 * BFC * SUPPLY_FACTOR,
			max: 5_000 * BFC * SUPPLY_FACTOR,
		},
		// annual inflation
		annual,
		round: to_round_inflation(annual),
	}
}

pub fn development_config() -> Result<ChainSpec, String> {
	let wasm_binary = WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?;

	Ok(ChainSpec::from_genesis(
		// Name
		"Bifrost Development",
		// ID
		"dev",
		ChainType::Development,
		move || {
			development_genesis(
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
				// Sudo account
				AccountId::from(hex!("f24FF3a9CF04c71Dbc94D0b566f7A27B94566cac")),
				// Pre-funded accounts
				vec![
					// Stash accounts
					AccountId::from(hex!("912F9D002E46DF70C78495D29Faa523c2c0382a2")),
					AccountId::from(hex!("fc9B16D9ADe4712E762503C5801F59f2011D9Ad1")),
					AccountId::from(hex!("FA374f977f325Aa41c7EC7e98306ee531F8A2c32")),
					AccountId::from(hex!("C548bFa03FF5be8096Be0FAa2dbC66c3bC440258")),
					AccountId::from(hex!("E9dfCCE5F48A8896fC79A3e674E96443057ed2F4")),
					AccountId::from(hex!("761058f6Ffe8cC41fb40Bdc56FCcc2067bc5b5F2")),
					AccountId::from(hex!("ca1134B75604209B66a94e9Bc3278b978FbEE708")),
					AccountId::from(hex!("C7b701010559703508997Bd029A0F2aE689BEF20")),
					AccountId::from(hex!("7b5e2523fF3B55f4bf122D41D4202Fc2F469a27B")),
					AccountId::from(hex!("5f01df1aB45ef0542F04234DDCE70Aa455a83fC4")),
					// Controller accounts
					AccountId::from(hex!("f24FF3a9CF04c71Dbc94D0b566f7A27B94566cac")),
					AccountId::from(hex!("3Cd0A705a2DC65e5b1E1205896BaA2be8A07c6e0")),
					AccountId::from(hex!("798d4Ba9baf0064Ec19eB4F0a1a45785ae9D6DFc")),
					AccountId::from(hex!("773539d4Ac0e786233D90A233654ccEE26a613D9")),
					AccountId::from(hex!("Ff64d3F6efE2317EE2807d223a0Bdc4c0c49dfDB")),
					AccountId::from(hex!("C0F0f4ab324C46e55D02D0033343B4Be8A55532d")),
					AccountId::from(hex!("7BF369283338E12C90514468aa3868A551AB2929")),
					AccountId::from(hex!("931f3600a299fd9B24cEfB3BfF79388D19804BeA")),
					AccountId::from(hex!("C41C5F1123ECCd5ce233578B2e7ebd5693869d73")),
					AccountId::from(hex!("2898FE7a42Be376C8BC7AF536A940F7Fd5aDd423")),
					// Relayer accounts
					AccountId::from(hex!("d6D3f3a35Fab64F69b7885D6162e81B62e44bF58")),
					AccountId::from(hex!("12159710B13fe31Cca949BcAfB190772Fb0E220C")),
					AccountId::from(hex!("6E574113B9A9105ba6B5877379a25b4Fc8327c5A")),
					AccountId::from(hex!("a7e19a783c6BB2A3732CcAD33DDD022B0aE8A439")),
					AccountId::from(hex!("7Bd2836681618e229BE5E6912B6969Ae3565A5C5")),
					AccountId::from(hex!("8e0Ed0855D3E5244E4302CAA2154F6FFDeeAFA9f")),
					AccountId::from(hex!("f0d9Abf34208681da3BBc84A59d4244506D3D012")),
					AccountId::from(hex!("4EA8C2D0826Bc3242d093A05c92a3771c43B919A")),
					AccountId::from(hex!("f4fc2d9Be3D6e19cCAfd575dE7CB290A585A1a22")),
					AccountId::from(hex!("962dBf2aecF6545f552373487127976fD5B55105")),
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
fn development_genesis(
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
) -> devnet::RuntimeGenesisConfig {
	let revert_bytecode = vec![0x60, 0x00, 0x60, 0x00, 0xFD];

	devnet::RuntimeGenesisConfig {
		system: devnet::SystemConfig {
			// Add Wasm runtime to storage.
			code: wasm_binary.to_vec(),
			..Default::default()
		},
		balances: devnet::BalancesConfig {
			balances: endowed_accounts
				.iter()
				.cloned()
				.map(|k| (k, 100_000_000_000 * BFC))
				.collect(),
		},
		session: devnet::SessionConfig {
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
		sudo: devnet::SudoConfig { key: Some(root_key) },
		transaction_payment: Default::default(),
		evm: EVMConfig {
			// We need _some_ code inserted at the precompile address so that
			// the evm will actually call the address.
			accounts: Precompiles::used_addresses()
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
		base_fee: devnet::BaseFeeConfig::new(
			sp_core::U256::from(1_000 * GWEI * SUPPLY_FACTOR),
			sp_runtime::Permill::zero(),
		),
		relay_manager: Default::default(),
		bfc_staking: devnet::BfcStakingConfig {
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
		council_membership: devnet::CouncilMembershipConfig {
			phantom: Default::default(),
			members: BoundedVec::try_from(initial_council_members.clone())
				.expect("Membership must be initialized."),
		},
		technical_membership: devnet::TechnicalMembershipConfig {
			phantom: Default::default(),
			members: BoundedVec::try_from(initial_tech_committee_members.clone())
				.expect("Membership must be initialized"),
		},
		treasury: Default::default(),
	}
}
