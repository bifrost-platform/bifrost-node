//! Storage migrations for `pallet-tranche-system`.
//!
//! Only the latest migration is kept: every live chain running this pallet is
//! already at the version the previous ones targeted (v6), so they could never
//! run again.

use crate::{
	AdapterInfo, AdapterKey, ChainTranches, CollateralAsset, Config, MultichainAdapterInfo,
	MultichainProductDetails, Pallet, ProductDetails, ProductId, SettlementMode,
	SingleChainProductDetails, SingleChainValuationInfo, SourceType, Tranche, TrancheType,
	ValuationInfo, VaultId, VaultRegistration, MAX_ADAPTERS_PER_MULTICHAIN_ADAPTER,
	MAX_ADAPTERS_PER_SINGLE_CHAIN_PRODUCT, MAX_COLLATERALS, MAX_MULTICHAIN_ADAPTERS,
	MAX_TRANCHES_PER_CHAIN, MAX_TRANCHE_CHAINS, MAX_TRANCHE_MANAGERS,
};

use bp_tranche::from_evm;
use frame_support::{
	migrations::VersionedMigration, pallet_prelude::*, storage_alias,
	traits::UncheckedOnRuntimeUpgrade, weights::Weight, Blake2_128Concat,
};
use sp_core::{ConstU32, H160};
use sp_runtime::{BoundedBTreeMap, BoundedVec};
use sp_std::{marker::PhantomData, vec::Vec};

pub(crate) const LOG_TARGET: &str = "runtime::tranche-system";

macro_rules! log {
	($level:tt, $patter:expr $(, $values:expr)* $(,)?) => {
		log::$level!(
			target: LOG_TARGET,
			$patter $(, $values)*
		)
	};
}

/// v6 -> v7: non-EVM support. Every address that lives on a product/spoke chain
/// (vault, adapter, tranche asset/shares, collateral NFT contract, tranche
/// manager, single-chain valuation/base asset/ledger) widens from `H160` to a
/// 32-byte `ChainAddress` (EVM addresses left-padded). Hub-local addresses
/// (multichain `ValuationInfo`, `OrchestratorAddress`) are unchanged.
///
/// The adapter `borrower` (previously the Hub `AccountId`) becomes a
/// `ChainAddress` too, and the `AccountId` generic is dropped from every
/// product type.
///
/// Rewrites every `Products` value and re-keys `Vaults`, `AdapterIndex` and
/// `MultichainAdapterIndex` (their keys embed `VaultId`/`AdapterKey`).
pub mod v7 {
	use super::*;

	/// The v6 (pre-non-EVM) shapes, field-for-field. Types whose encoding didn't
	/// change (`TrancheType`, `ValuationInfo`, `SettlementMode`,
	/// `VaultRegistration`) are reused from the crate.
	pub mod old {
		use super::*;

		#[derive(Clone, Encode, Decode, PartialEq, Eq, Ord, PartialOrd, RuntimeDebug)]
		pub struct VaultId {
			pub chain_id: u64,
			pub vault_address: H160,
		}

		#[derive(Clone, Encode, Decode, PartialEq, Eq, Ord, PartialOrd, RuntimeDebug)]
		pub struct AdapterKey {
			pub address: H160,
			pub chain_id: u64,
		}

		#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug)]
		pub struct Tranche {
			pub tranche_type: TrancheType,
			pub vault: VaultId,
			pub asset: H160,
			pub shares: H160,
		}

		#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug)]
		pub struct CollateralAsset {
			pub chain_id: u64,
			pub nft_contract: H160,
			pub nft_token_id: sp_core::U256,
		}

		/// `borrower` was the Hub `AccountId` — `AccountId20` on every Bifrost
		/// runtime, so it's decoded here as the byte-identical `H160`.
		#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug)]
		pub enum SourceType {
			OffchainSource {
				borrower: H160,
				collaterals: BoundedVec<CollateralAsset, ConstU32<MAX_COLLATERALS>>,
			},
			OnchainSource,
		}

		#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug)]
		pub struct AdapterInfo {
			pub source_type: SourceType,
			pub weight_bps: u16,
		}

		#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug)]
		pub struct MultichainAdapterInfo {
			pub weight_bps: u16,
			pub adapters:
				BoundedBTreeMap<H160, AdapterInfo, ConstU32<MAX_ADAPTERS_PER_MULTICHAIN_ADAPTER>>,
		}

		pub type ChainTranches = BoundedVec<Tranche, ConstU32<MAX_TRANCHES_PER_CHAIN>>;

		#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug)]
		pub struct MultichainProductDetails {
			pub valuation: ValuationInfo,
			pub tranches: BoundedBTreeMap<u64, ChainTranches, ConstU32<MAX_TRANCHE_CHAINS>>,
			pub multichain_adapters: BoundedBTreeMap<
				AdapterKey,
				MultichainAdapterInfo,
				ConstU32<MAX_MULTICHAIN_ADAPTERS>,
			>,
			pub multichain_tranche_managers:
				BoundedBTreeMap<u64, H160, ConstU32<MAX_TRANCHE_MANAGERS>>,
		}

		#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug)]
		pub struct SingleChainValuationInfo {
			pub base_asset: H160,
			pub valuation_address: H160,
			pub settlement_mode: SettlementMode,
		}

		#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug)]
		pub struct SingleChainProductDetails {
			pub valuation: SingleChainValuationInfo,
			pub chain_id: u64,
			pub tranches: ChainTranches,
			pub tranche_manager: H160,
			pub adapters:
				BoundedBTreeMap<H160, AdapterInfo, ConstU32<MAX_ADAPTERS_PER_SINGLE_CHAIN_PRODUCT>>,
			pub ledger: H160,
		}

		#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug)]
		pub enum ProductDetails {
			Multichain(MultichainProductDetails),
			SingleChain(SingleChainProductDetails),
		}
	}

	#[storage_alias]
	pub type Products<T: Config> =
		StorageMap<Pallet<T>, Blake2_128Concat, ProductId, old::ProductDetails>;

	#[storage_alias]
	pub type Vaults<T: Config> =
		StorageMap<Pallet<T>, Blake2_128Concat, old::VaultId, VaultRegistration>;

	#[storage_alias]
	pub type AdapterIndex<T: Config> =
		StorageMap<Pallet<T>, Blake2_128Concat, old::AdapterKey, ProductId>;

	#[storage_alias]
	pub type MultichainAdapterIndex<T: Config> =
		StorageMap<Pallet<T>, Blake2_128Concat, old::AdapterKey, ProductId>;

	fn vault(v: old::VaultId) -> VaultId {
		VaultId { chain_id: v.chain_id, vault_address: from_evm(v.vault_address) }
	}

	fn adapter_key(k: old::AdapterKey) -> AdapterKey {
		AdapterKey { address: from_evm(k.address), chain_id: k.chain_id }
	}

	fn tranches(old: old::ChainTranches) -> ChainTranches {
		// Same bound on both sides (`MAX_TRANCHES_PER_CHAIN`), so this never truncates.
		BoundedVec::truncate_from(
			old.into_inner()
				.into_iter()
				.map(|t| Tranche {
					tranche_type: t.tranche_type,
					vault: vault(t.vault),
					asset: from_evm(t.asset),
					shares: from_evm(t.shares),
				})
				.collect(),
		)
	}

	fn adapter_info(old: old::AdapterInfo) -> AdapterInfo {
		let source_type = match old.source_type {
			old::SourceType::OffchainSource { borrower, collaterals } => {
				SourceType::OffchainSource {
					borrower: from_evm(borrower),
					collaterals: BoundedVec::truncate_from(
						collaterals
							.into_inner()
							.into_iter()
							.map(|c| CollateralAsset {
								chain_id: c.chain_id,
								nft_contract: from_evm(c.nft_contract),
								nft_token_id: c.nft_token_id,
							})
							.collect(),
					),
				}
			},
			old::SourceType::OnchainSource => SourceType::OnchainSource,
		};
		AdapterInfo { source_type, weight_bps: old.weight_bps }
	}

	/// Re-keys an `H160`-keyed bounded map by `from_evm`. Keys stay distinct (the
	/// mapping is injective) and the bound is unchanged, so no entry is dropped.
	fn rekey_adapters<S: Get<u32>>(
		old: BoundedBTreeMap<H160, old::AdapterInfo, S>,
	) -> BoundedBTreeMap<ChainAddress, AdapterInfo, S> {
		let mut new = BoundedBTreeMap::new();
		for (address, info) in old.into_inner() {
			let _ = new.try_insert(from_evm(address), adapter_info(info));
		}
		new
	}

	use crate::ChainAddress;

	fn product(old: old::ProductDetails) -> ProductDetails {
		match old {
			old::ProductDetails::Multichain(p) => {
				let mut by_chain = BoundedBTreeMap::new();
				for (chain_id, ts) in p.tranches.into_inner() {
					let _ = by_chain.try_insert(chain_id, tranches(ts));
				}
				let mut multichain_adapters = BoundedBTreeMap::new();
				for (key, info) in p.multichain_adapters.into_inner() {
					let _ = multichain_adapters.try_insert(
						adapter_key(key),
						MultichainAdapterInfo {
							weight_bps: info.weight_bps,
							adapters: rekey_adapters(info.adapters),
						},
					);
				}
				let mut managers = BoundedBTreeMap::new();
				for (chain_id, manager) in p.multichain_tranche_managers.into_inner() {
					let _ = managers.try_insert(chain_id, from_evm(manager));
				}
				ProductDetails::Multichain(MultichainProductDetails {
					valuation: p.valuation,
					tranches: by_chain,
					multichain_adapters,
					multichain_tranche_managers: managers,
				})
			},
			old::ProductDetails::SingleChain(p) => {
				ProductDetails::SingleChain(SingleChainProductDetails {
					valuation: SingleChainValuationInfo {
						base_asset: from_evm(p.valuation.base_asset),
						valuation_address: from_evm(p.valuation.valuation_address),
						settlement_mode: p.valuation.settlement_mode,
					},
					chain_id: p.chain_id,
					tranches: tranches(p.tranches),
					tranche_manager: from_evm(p.tranche_manager),
					adapters: rekey_adapters(p.adapters),
					ledger: from_evm(p.ledger),
				})
			},
		}
	}

	pub struct MigrateV6ToV7<T>(PhantomData<T>);

	impl<T: Config> UncheckedOnRuntimeUpgrade for MigrateV6ToV7<T> {
		fn on_runtime_upgrade() -> Weight {
			// Collect before re-inserting: old and new keys share each map's
			// prefix, so draining while inserting could revisit new entries.
			let products = Products::<T>::drain().collect::<Vec<_>>();
			let vaults = Vaults::<T>::drain().collect::<Vec<_>>();
			let adapters = AdapterIndex::<T>::drain().collect::<Vec<_>>();
			let multichain_adapters = MultichainAdapterIndex::<T>::drain().collect::<Vec<_>>();

			let total =
				(products.len() + vaults.len() + adapters.len() + multichain_adapters.len()) as u64;

			for (product_id, details) in products.iter().cloned() {
				crate::Products::<T>::insert(product_id, product(details));
			}
			for (key, registration) in vaults.iter().cloned() {
				crate::Vaults::<T>::insert(vault(key), registration);
			}
			for (key, product_id) in adapters.iter().cloned() {
				crate::AdapterIndex::<T>::insert(adapter_key(key), product_id);
			}
			for (key, product_id) in multichain_adapters.iter().cloned() {
				crate::MultichainAdapterIndex::<T>::insert(adapter_key(key), product_id);
			}

			log!(
				info,
				"tranche-system v7: migrated {} products, {} vaults, {} adapters, {} multichain adapters",
				products.len(),
				vaults.len(),
				adapters.len(),
				multichain_adapters.len()
			);

			// Each entry: 1 read + 1 write (drain) + 1 write (insert).
			T::DbWeight::get().reads_writes(total, total.saturating_mul(2))
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			let counts: (u32, u32, u32, u32) = (
				Products::<T>::iter_keys().count() as u32,
				Vaults::<T>::iter_keys().count() as u32,
				AdapterIndex::<T>::iter_keys().count() as u32,
				MultichainAdapterIndex::<T>::iter_keys().count() as u32,
			);
			Ok(counts.encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			let (products, vaults, adapters, multichain_adapters): (u32, u32, u32, u32) =
				Decode::decode(&mut &state[..]).map_err(|_| "v7: bad pre_upgrade state")?;
			ensure!(crate::Products::<T>::iter().count() as u32 == products, "v7: products lost");
			ensure!(crate::Vaults::<T>::iter().count() as u32 == vaults, "v7: vaults lost");
			ensure!(
				crate::AdapterIndex::<T>::iter().count() as u32 == adapters,
				"v7: adapters lost"
			);
			ensure!(
				crate::MultichainAdapterIndex::<T>::iter().count() as u32 == multichain_adapters,
				"v7: multichain adapters lost"
			);
			Ok(())
		}
	}

	pub type MigrateToV7<T> = VersionedMigration<
		6,
		7,
		MigrateV6ToV7<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;
}
