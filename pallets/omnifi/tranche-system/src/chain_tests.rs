//! The chain registry: every address a product introduces must be on a registered chain and
//! in that chain's canonical form.

use crate::{
	mock::*, AdapterInfo, AdapterKey, ChainAddress, ChainInfo, ChainInfos, CollateralAsset,
	CrudAction, Error, Event, MultichainAdapterInfo, Origin, Products, SettlementMode,
	SingleChainValuationInfo, SourceType, TrancheInput, TrancheType, ValuationInfo, VaultId,
};
use bp_core::AccountId;
use frame_support::{assert_noop, assert_ok, BoundedBTreeMap, BoundedVec};
use sp_core::{H160, H256, U256};
use sp_runtime::DispatchError;

const PID: u64 = 1;
/// A single-chain product id (prefix 2).
const SC_PID: u64 = (2 << 32) | 1;
const EVM: u64 = 1;
const SOLANA: u64 = 2;
const UNREGISTERED: u64 = 3;

fn admin() -> RuntimeOrigin {
	Origin::<Test>::ProductAdmin(AccountId::from([1u8; 20])).into()
}

/// `n` as a canonical EVM address: 12 zero bytes, then 20 address bytes.
fn evm(n: u64) -> ChainAddress {
	H160::from_low_u64_be(n).into()
}

/// The same low 20 bytes as `evm(n)` but with a non-zero padding byte: another 32-byte
/// value for the same EVM address.
fn alias(n: u64) -> ChainAddress {
	let mut address = evm(n);
	address.0[0] = 1;
	address
}

fn register_chains() {
	for (chain_id, address_len, tx_hash_len) in [(EVM, 20, 32), (SOLANA, 32, 64)] {
		assert_ok!(TrancheSystem::set_chain_info(
			RuntimeOrigin::root(),
			chain_id,
			Some(ChainInfo { address_len, tx_hash_len })
		));
	}
}

/// Every address a multichain product introduces. The tests start from `Inputs::valid()`
/// and break one address at a time.
#[derive(Clone)]
struct Inputs {
	vault: VaultId,
	asset: ChainAddress,
	shares: ChainAddress,
	multichain_adapter: AdapterKey,
	adapter: ChainAddress,
	borrower: ChainAddress,
	collateral: (u64, ChainAddress),
	manager: (u64, ChainAddress),
}

impl Inputs {
	fn valid() -> Self {
		Self {
			vault: VaultId { chain_id: EVM, vault_address: evm(10) },
			asset: evm(11),
			shares: evm(12),
			multichain_adapter: AdapterKey { chain_id: EVM, address: evm(20) },
			adapter: evm(21),
			borrower: evm(22),
			collateral: (EVM, evm(23)),
			manager: (EVM, evm(30)),
		}
	}

	fn tranches<S: sp_core::Get<u32>>(&self) -> BoundedVec<TrancheInput, S> {
		BoundedVec::truncate_from(vec![TrancheInput {
			priority: 0,
			tranche_type: TrancheType::Senior { apr: U256::one() },
			vault: self.vault.clone(),
			asset: self.asset,
			shares: self.shares,
		}])
	}

	fn adapters<S: sp_core::Get<u32>>(&self) -> BoundedBTreeMap<ChainAddress, AdapterInfo, S> {
		let collaterals = BoundedVec::truncate_from(vec![CollateralAsset {
			chain_id: self.collateral.0,
			nft_contract: self.collateral.1,
			nft_token_id: U256::one(),
		}]);
		let source_type = SourceType::OffchainSource { borrower: self.borrower, collaterals };
		let mut adapters = BoundedBTreeMap::new();
		adapters
			.try_insert(self.adapter, AdapterInfo { source_type, weight_bps: 10_000 })
			.expect("one adapter");
		adapters
	}

	fn multichain_adapters<S: sp_core::Get<u32>>(
		&self,
	) -> BoundedBTreeMap<AdapterKey, MultichainAdapterInfo, S> {
		let mut multichain_adapters = BoundedBTreeMap::new();
		multichain_adapters
			.try_insert(
				self.multichain_adapter.clone(),
				MultichainAdapterInfo { weight_bps: 10_000, adapters: self.adapters() },
			)
			.expect("one multichain adapter");
		multichain_adapters
	}

	fn managers<S: sp_core::Get<u32>>(&self) -> BoundedBTreeMap<u64, ChainAddress, S> {
		let mut managers = BoundedBTreeMap::new();
		managers.try_insert(self.manager.0, self.manager.1).expect("one manager");
		managers
	}

	fn create(&self) -> sp_runtime::DispatchResult {
		TrancheSystem::create_product(
			admin(),
			PID,
			ValuationInfo {
				base_asset: H160::from_low_u64_be(1),
				valuation_address: H160::from_low_u64_be(2),
				settlement_start_timestamp: 4_000_000_000,
				settlement_length_secs: 86_400,
				settlement_offset_secs: 3_600,
			},
			self.tranches(),
			self.multichain_adapters(),
			self.managers(),
		)
	}

	fn create_single_chain(&self, valuation_address: ChainAddress) -> sp_runtime::DispatchResult {
		TrancheSystem::create_single_chain_product(
			admin(),
			SC_PID,
			EVM,
			SingleChainValuationInfo {
				base_asset: evm(1),
				valuation_address,
				settlement_mode: SettlementMode::Async {
					settlement_start_timestamp: 4_000_000_000,
					settlement_length_secs: 86_400,
					settlement_offset_secs: 3_600,
				},
			},
			self.tranches(),
			self.manager.1,
			self.adapters(),
			evm(40),
		)
	}
}

/// Each way of breaking one address of an otherwise valid product, by non-canonical form.
fn each_alias() -> Vec<(&'static str, Inputs)> {
	let ok = Inputs::valid;
	vec![
		("vault", Inputs { vault: VaultId { chain_id: EVM, vault_address: alias(10) }, ..ok() }),
		("asset", Inputs { asset: alias(11), ..ok() }),
		("shares", Inputs { shares: alias(12), ..ok() }),
		(
			"multichain adapter",
			Inputs { multichain_adapter: AdapterKey { chain_id: EVM, address: alias(20) }, ..ok() },
		),
		("adapter", Inputs { adapter: alias(21), ..ok() }),
		("borrower", Inputs { borrower: alias(22), ..ok() }),
		("collateral", Inputs { collateral: (EVM, alias(23)), ..ok() }),
		("tranche manager", Inputs { manager: (EVM, alias(30)), ..ok() }),
	]
}

/// The same, by putting the address on a chain nobody registered.
fn each_unregistered() -> Vec<(&'static str, Inputs)> {
	let ok = Inputs::valid;
	vec![
		("vault", Inputs { vault: VaultId { chain_id: UNREGISTERED, ..ok().vault }, ..ok() }),
		(
			"multichain adapter",
			Inputs { multichain_adapter: AdapterKey { chain_id: UNREGISTERED, address: evm(20) }, ..ok() },
		),
		("collateral", Inputs { collateral: (UNREGISTERED, evm(23)), ..ok() }),
		("tranche manager", Inputs { manager: (UNREGISTERED, evm(30)), ..ok() }),
	]
}

#[test]
fn set_chain_info_is_root_only_and_checks_the_lengths() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);
		let info = ChainInfo { address_len: 20, tx_hash_len: 32 };
		assert_noop!(
			TrancheSystem::set_chain_info(admin(), EVM, Some(info)),
			DispatchError::BadOrigin
		);
		for (address_len, tx_hash_len) in [(0, 32), (33, 32), (20, 0), (20, 65)] {
			assert_noop!(
				TrancheSystem::set_chain_info(
					RuntimeOrigin::root(),
					EVM,
					Some(ChainInfo { address_len, tx_hash_len })
				),
				Error::<Test>::InvalidChainInfo
			);
		}

		assert_ok!(TrancheSystem::set_chain_info(RuntimeOrigin::root(), EVM, Some(info)));
		assert_eq!(ChainInfos::<Test>::get(EVM), Some(info));
		System::assert_last_event(Event::ChainInfoSet { chain_id: EVM, info: Some(info) }.into());

		assert_ok!(TrancheSystem::set_chain_info(RuntimeOrigin::root(), EVM, None));
		assert_eq!(ChainInfos::<Test>::get(EVM), None);
	});
}

#[test]
fn create_product_refuses_unregistered_chains_and_non_canonical_addresses() {
	new_test_ext().execute_with(|| {
		// nothing registered yet: even the valid product is refused
		assert_noop!(Inputs::valid().create(), Error::<Test>::ChainNotRegistered);
		register_chains();

		for (what, inputs) in each_alias() {
			assert_noop!(inputs.create(), Error::<Test>::InvalidChainAddress);
			assert!(!Products::<Test>::contains_key(PID), "{what}");
		}
		for (what, inputs) in each_unregistered() {
			assert_noop!(inputs.create(), Error::<Test>::ChainNotRegistered);
			assert!(!Products::<Test>::contains_key(PID), "{what}");
		}

		assert_ok!(Inputs::valid().create());
	});
}

#[test]
fn a_32_byte_chain_takes_any_address() {
	new_test_ext().execute_with(|| {
		register_chains();
		let wide = H256::repeat_byte(0xab);
		let inputs = Inputs {
			vault: VaultId { chain_id: SOLANA, vault_address: wide },
			asset: wide,
			shares: wide,
			manager: (SOLANA, wide),
			..Inputs::valid()
		};
		assert_ok!(inputs.create());
	});
}

#[test]
fn create_single_chain_product_checks_its_own_addresses_too() {
	new_test_ext().execute_with(|| {
		register_chains();
		assert_noop!(
			Inputs::valid().create_single_chain(alias(2)),
			Error::<Test>::InvalidChainAddress
		);
		for (what, inputs) in each_alias() {
			// a single-chain product has no multichain adapter key
			if what != "multichain adapter" {
				assert_noop!(
					inputs.create_single_chain(evm(2)),
					Error::<Test>::InvalidChainAddress
				);
			}
		}
		assert_ok!(Inputs::valid().create_single_chain(evm(2)));
	});
}

#[test]
fn product_updates_are_checked_like_creation() {
	new_test_ext().execute_with(|| {
		register_chains();
		let ok = Inputs::valid();
		assert_ok!(ok.create());
		let junior = VaultId { chain_id: EVM, vault_address: evm(50) };
		let add = |vault: VaultId, asset, shares| {
			TrancheSystem::set_tranche(
				admin(),
				PID,
				CrudAction::Add,
				vault,
				TrancheType::Junior,
				asset,
				shares,
				1,
			)
		};

		// set_tranche(Add): vault, asset and shares
		for (vault_address, asset, shares) in
			[(alias(50), evm(11), evm(12)), (evm(50), alias(11), evm(12)), (evm(50), evm(11), alias(12))]
		{
			assert_noop!(
				add(VaultId { chain_id: EVM, vault_address }, asset, shares),
				Error::<Test>::InvalidChainAddress
			);
		}
		assert_noop!(
			add(VaultId { chain_id: UNREGISTERED, vault_address: evm(50) }, evm(11), evm(12)),
			Error::<Test>::ChainNotRegistered
		);
		assert_ok!(add(junior.clone(), evm(11), evm(12)));
		// a Senior and a second Junior on the other chain, so the first Junior can be
		// removed below (each tranche type must still exist somewhere)
		for (n, tranche_type) in
			[TrancheType::Senior { apr: U256::one() }, TrancheType::Junior].into_iter().enumerate()
		{
			let wide = H256::repeat_byte(0xab + n as u8);
			assert_ok!(TrancheSystem::set_tranche(
				admin(),
				PID,
				CrudAction::Add,
				VaultId { chain_id: SOLANA, vault_address: wide },
				tranche_type,
				wide,
				wide,
				n as u8,
			));
		}

		// set_adapters, set_multichain_adapters, set_multichain_tranche_managers
		for (what, inputs) in each_alias() {
			if ["adapter", "borrower", "collateral"].contains(&what) {
				assert_noop!(
					TrancheSystem::set_adapters(admin(), PID, evm(20), EVM, inputs.adapters()),
					Error::<Test>::InvalidChainAddress
				);
			}
			if what == "tranche manager" {
				assert_noop!(
					TrancheSystem::set_multichain_tranche_managers(admin(), PID, inputs.managers()),
					Error::<Test>::InvalidChainAddress
				);
			}
			if ["multichain adapter", "adapter", "borrower", "collateral"].contains(&what) {
				assert_noop!(
					TrancheSystem::set_multichain_adapters(
						admin(),
						PID,
						inputs.multichain_adapters()
					),
					Error::<Test>::InvalidChainAddress
				);
			}
		}

		// Once a chain is unregistered nothing new can be added there, but a tranche
		// already on it can still be removed.
		assert_ok!(TrancheSystem::set_chain_info(RuntimeOrigin::root(), EVM, None));
		assert_noop!(
			add(VaultId { chain_id: EVM, vault_address: evm(51) }, evm(11), evm(12)),
			Error::<Test>::ChainNotRegistered
		);
		assert_ok!(TrancheSystem::set_tranche(
			admin(),
			PID,
			CrudAction::Remove,
			junior,
			TrancheType::Junior,
			evm(11),
			evm(12),
			1,
		));
	});
}
