//! Benchmarks for `pallet-tranche-system`.
//!
//! Every extrinsic is benchmarked at a fixed worst-case shape — the product
//! creators build a full product (`MAX_TRANCHE_CHAINS` chains ×
//! `MAX_TRANCHES_PER_CHAIN` tranches, `MAX_MULTICHAIN_ADAPTERS` adapters each
//! with `MAX_ADAPTERS_PER_MULTICHAIN_ADAPTER` nested adapters,
//! `MAX_TRANCHE_MANAGERS` managers), the mutators act on such a product. No
//! linear components yet — the trait's `WeightInfo` methods take no arguments,
//! so the generated weights are the (conservative) worst case applied
//! uniformly. Widening the signatures for `create_product`/`set_*` is tracked
//! as follow-up (security-review M5).

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::{
	AdapterInfo, AdapterKey, CrudAction, MultichainAdapterInfo, SettlementMode,
	SingleChainValuationInfo, SourceType, TrancheInput, TrancheType, ValuationInfo, VaultId,
	MAX_ADAPTERS_PER_MULTICHAIN_ADAPTER, MAX_ADAPTERS_PER_SINGLE_CHAIN_PRODUCT, MAX_COLLATERALS,
	MAX_MULTICHAIN_ADAPTERS, MAX_TRANCHES_PER_CHAIN, MAX_TRANCHE_CHAINS, MAX_TRANCHE_MANAGERS,
};
use frame_benchmarking::v2::*;
use frame_support::{traits::EnsureOrigin, BoundedBTreeMap, BoundedVec};
use frame_system::RawOrigin;
use sp_core::{H160, U256};
use sp_std::vec::Vec;

const PID: ProductId = 1;

fn admin_origin<T: Config>() -> T::RuntimeOrigin {
	T::ProductAdminOrigin::try_successful_origin().expect("ProductAdminOrigin benchmark helper")
}

fn h160(n: u64) -> H160 {
	H160::from_low_u64_be(n)
}

fn valuation() -> ValuationInfo {
	ValuationInfo {
		base_asset: h160(0xba5e),
		valuation_address: h160(0x7a1),
		settlement_start_timestamp: 4_000_000_000,
		settlement_length_secs: 86_400,
		settlement_offset_secs: 3_600,
	}
}

/// `n` tranches for one chain: `n-1` distinct-APR Seniors (priority `0..n-1`)
/// then one Junior (priority `n-1`). Satisfies every per-chain composition rule.
fn chain_tranches<T: Config>(chain_id: u64, n: u32, vault_seed: &mut u64) -> Vec<TrancheInput> {
	(0..n)
		.map(|i| {
			*vault_seed += 1;
			let is_junior = i == n - 1;
			TrancheInput {
				priority: i as u8,
				tranche_type: if is_junior {
					TrancheType::Junior
				} else {
					TrancheType::Senior { apr: U256::from(i + 1) }
				},
				vault: VaultId { chain_id, vault_address: h160(*vault_seed) },
				asset: h160(0xa55e7),
				shares: h160(0x_5_a4e5),
			}
		})
		.collect()
}

fn full_tranches<T: Config>() -> BoundedVec<TrancheInput, ConstU32<{ crate::MAX_TRANCHE_INPUTS }>> {
	let mut seed = 1_000u64;
	let mut all = Vec::new();
	for c in 1..=MAX_TRANCHE_CHAINS as u64 {
		all.extend(chain_tranches::<T>(c, MAX_TRANCHES_PER_CHAIN, &mut seed));
	}
	BoundedVec::try_from(all).expect("MAX_TRANCHE_INPUTS")
}

fn nested_adapters<T: Config>(
	parent: u64,
) -> BoundedBTreeMap<H160, AdapterInfo<T::AccountId>, ConstU32<MAX_ADAPTERS_PER_MULTICHAIN_ADAPTER>>
{
	let mut m = BoundedBTreeMap::new();
	let n = MAX_ADAPTERS_PER_MULTICHAIN_ADAPTER;
	for j in 0..n {
		let collaterals = BoundedVec::try_from(
			(0..MAX_COLLATERALS)
				.map(|k| crate::CollateralAsset {
					chain_id: 900 + k as u64,
					nft_contract: h160(0x2_000_000 + parent * 1_000 + j as u64 * 10 + k as u64),
					nft_token_id: U256::from(k),
				})
				.collect::<Vec<_>>(),
		)
		.expect("MAX_COLLATERALS");
		m.try_insert(
			h160(0x1_000_000 + parent * 1_000 + j as u64),
			AdapterInfo {
				source_type: SourceType::OffchainSource {
					borrower: whitelisted_caller(),
					collaterals,
				},
				weight_bps: (10_000 / n) as u16 + if j == 0 { (10_000 % n) as u16 } else { 0 },
			},
		)
		.expect("nested adapter insert");
	}
	m
}

fn full_multichain_adapters<T: Config>() -> BoundedBTreeMap<
	AdapterKey,
	MultichainAdapterInfo<T::AccountId>,
	ConstU32<MAX_MULTICHAIN_ADAPTERS>,
> {
	let mut m = BoundedBTreeMap::new();
	let n = MAX_MULTICHAIN_ADAPTERS;
	for i in 0..n as u64 {
		m.try_insert(
			AdapterKey { address: h160(0x9_000_000 + i), chain_id: 100 + i },
			MultichainAdapterInfo {
				weight_bps: (10_000 / n) as u16 + if i == 0 { (10_000 % n) as u16 } else { 0 },
				adapters: nested_adapters::<T>(i),
			},
		)
		.expect("multichain adapter insert");
	}
	m
}

fn full_managers() -> BoundedBTreeMap<u64, H160, ConstU32<MAX_TRANCHE_MANAGERS>> {
	let mut m = BoundedBTreeMap::new();
	for c in 1..=MAX_TRANCHE_MANAGERS as u64 {
		m.try_insert(c, h160(0x3_000_000 + c)).expect("manager insert");
	}
	m
}

/// Create the full worst-case multichain product `PID` (used to set up the mutators).
fn setup_full_product<T: Config>() {
	Pallet::<T>::create_product(
		admin_origin::<T>(),
		PID,
		valuation(),
		full_tranches::<T>(),
		full_multichain_adapters::<T>(),
		full_managers(),
	)
	.expect("create_product setup");
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn create_product() {
		let v = valuation();
		#[extrinsic_call]
		_(
			admin_origin::<T>() as T::RuntimeOrigin,
			PID,
			v,
			full_tranches::<T>(),
			full_multichain_adapters::<T>(),
			full_managers(),
		);
		assert!(Products::<T>::contains_key(PID));
	}

	#[benchmark]
	fn create_single_chain_product() {
		let mut seed = 5_000u64;
		let tranches =
			BoundedVec::try_from(chain_tranches::<T>(7, MAX_TRANCHES_PER_CHAIN, &mut seed))
				.expect("MAX_TRANCHES_PER_CHAIN");
		let mut adapters: BoundedBTreeMap<
			H160,
			AdapterInfo<T::AccountId>,
			ConstU32<MAX_ADAPTERS_PER_SINGLE_CHAIN_PRODUCT>,
		> = BoundedBTreeMap::new();
		let n = MAX_ADAPTERS_PER_SINGLE_CHAIN_PRODUCT;
		for j in 0..n {
			adapters
				.try_insert(
					h160(0x4_000_000 + j as u64),
					AdapterInfo {
						source_type: SourceType::OnchainSource,
						weight_bps: (10_000 / n) as u16
							+ if j == 0 { (10_000 % n) as u16 } else { 0 },
					},
				)
				.expect("sc adapter insert");
		}
		let val = SingleChainValuationInfo {
			base_asset: h160(0xba5e),
			valuation_address: h160(0x7a1),
			settlement_mode: SettlementMode::Async {
				settlement_start_timestamp: 4_000_000_000,
				settlement_length_secs: 86_400,
				settlement_offset_secs: 3_600,
			},
		};

		#[extrinsic_call]
		_(
			admin_origin::<T>() as T::RuntimeOrigin,
			PID,
			7u64,
			val,
			tranches,
			h160(0x6ed_0),
			adapters,
			h160(0x_1ed6e7),
		);

		assert!(Products::<T>::contains_key(PID));
	}

	/// Worst case: `Add` a tranche onto a chain that is one short of full,
	/// forcing the insert-and-shift over `MAX_TRANCHES_PER_CHAIN - 1` entries.
	#[benchmark]
	fn set_tranche() {
		// Build a product whose chain 1 has room for exactly one more tranche.
		let mut seed = 8_000u64;
		let mut all = Vec::new();
		all.extend(chain_tranches::<T>(1, MAX_TRANCHES_PER_CHAIN - 1, &mut seed));
		for c in 2..=MAX_TRANCHE_CHAINS as u64 {
			all.extend(chain_tranches::<T>(c, MAX_TRANCHES_PER_CHAIN, &mut seed));
		}
		Pallet::<T>::create_product(
			admin_origin::<T>(),
			PID,
			valuation(),
			BoundedVec::try_from(all).expect("inputs"),
			full_multichain_adapters::<T>(),
			full_managers(),
		)
		.expect("setup");

		let new_vault = VaultId { chain_id: 1, vault_address: h160(0x9_999_999) };

		#[extrinsic_call]
		_(
			admin_origin::<T>() as T::RuntimeOrigin,
			PID,
			CrudAction::Add,
			new_vault.clone(),
			TrancheType::Senior { apr: U256::from(42) },
			h160(0xa55e7),
			h160(0x_5_a4e5),
			0u8,
		);

		assert!(Vaults::<T>::contains_key(&new_vault));
	}

	/// Worst case: full-replace one MultichainAdapter's nested adapter set
	/// (`MAX_ADAPTERS_PER_MULTICHAIN_ADAPTER` out, same many in).
	#[benchmark]
	fn set_adapters() {
		setup_full_product::<T>();
		let mut replacement: BoundedBTreeMap<
			H160,
			AdapterInfo<T::AccountId>,
			ConstU32<MAX_ADAPTERS_PER_MULTICHAIN_ADAPTER>,
		> = BoundedBTreeMap::new();
		let n = MAX_ADAPTERS_PER_MULTICHAIN_ADAPTER;
		for j in 0..n {
			replacement
				.try_insert(
					h160(0x5_000_000 + j as u64),
					AdapterInfo {
						source_type: SourceType::OnchainSource,
						weight_bps: (10_000 / n) as u16
							+ if j == 0 { (10_000 % n) as u16 } else { 0 },
					},
				)
				.expect("insert");
		}

		#[extrinsic_call]
		_(admin_origin::<T>() as T::RuntimeOrigin, PID, h160(0x9_000_000), 100u64, replacement);
	}

	/// Worst case: full-replace the whole MultichainAdapter routing table.
	#[benchmark]
	fn set_multichain_adapters() {
		setup_full_product::<T>();
		// A fresh full table on different chains/addresses so every index row
		// is removed and re-inserted.
		let mut m: BoundedBTreeMap<
			AdapterKey,
			MultichainAdapterInfo<T::AccountId>,
			ConstU32<MAX_MULTICHAIN_ADAPTERS>,
		> = BoundedBTreeMap::new();
		let n = MAX_MULTICHAIN_ADAPTERS;
		for i in 0..n as u64 {
			let mut nested: BoundedBTreeMap<
				H160,
				AdapterInfo<T::AccountId>,
				ConstU32<MAX_ADAPTERS_PER_MULTICHAIN_ADAPTER>,
			> = BoundedBTreeMap::new();
			let k = MAX_ADAPTERS_PER_MULTICHAIN_ADAPTER;
			for j in 0..k {
				nested
					.try_insert(
						h160(0x6_000_000 + i * 100 + j as u64),
						AdapterInfo {
							source_type: SourceType::OnchainSource,
							weight_bps: (10_000 / k) as u16
								+ if j == 0 { (10_000 % k) as u16 } else { 0 },
						},
					)
					.expect("insert");
			}
			m.try_insert(
				AdapterKey { address: h160(0x8_000_000 + i), chain_id: 200 + i },
				MultichainAdapterInfo {
					weight_bps: (10_000 / n) as u16 + if i == 0 { (10_000 % n) as u16 } else { 0 },
					adapters: nested,
				},
			)
			.expect("insert");
		}

		#[extrinsic_call]
		_(admin_origin::<T>() as T::RuntimeOrigin, PID, m);
	}

	#[benchmark]
	fn set_orchestrator_address() {
		#[extrinsic_call]
		_(RawOrigin::Root, h160(0x0_c_4e_5));
		assert_eq!(OrchestratorAddress::<T>::get(), h160(0x0_c_4e_5));
	}

	#[benchmark]
	fn set_multichain_tranche_managers() {
		setup_full_product::<T>();
		let mut m: BoundedBTreeMap<u64, H160, ConstU32<MAX_TRANCHE_MANAGERS>> =
			BoundedBTreeMap::new();
		for c in 1..=MAX_TRANCHE_MANAGERS as u64 {
			m.try_insert(c, h160(0x7_000_000 + c)).expect("insert");
		}

		#[extrinsic_call]
		_(admin_origin::<T>() as T::RuntimeOrigin, PID, m);
	}

	/// Stubbed extrinsic — rejects immediately. Benchmark the reject path so
	/// the generated `WeightInfo` stays complete.
	#[benchmark]
	fn set_request_flow_version() {
		let origin = admin_origin::<T>();
		#[block]
		{
			let _ = Pallet::<T>::set_request_flow_version(origin, PID, crate::FlowVersion::V2);
		}
	}

	#[benchmark]
	fn set_settlement_flow_version() {
		let origin = admin_origin::<T>();
		#[block]
		{
			let _ = Pallet::<T>::set_settlement_flow_version(origin, PID, crate::FlowVersion::V2);
		}
	}
}
