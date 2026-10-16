//! Benchmarks for `pallet-tranche-system`.
//!
//! Every product extrinsic is linear in the product's size: `t` = tranche count,
//! `a` = adapter reverse-index entries (for a multichain product: top-level
//! MultichainAdapters plus all their nested adapters — each costs one index
//! read + write; for a single-chain product: its flat adapters). The replacing
//! extrinsics add the incoming set's size `n` (and `set_multichain_adapters` the
//! replaced table's size `o`). Everything else in the input (collaterals,
//! TrancheManager table) is held at its maximum. The mutators charge the maximum
//! up front and refund to the stored product's actual size after dispatch.

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::ChainAddress;
use crate::{
	AdapterInfo, AdapterKey, CrudAction, MultichainAdapterInfo, SettlementMode,
	SingleChainValuationInfo, SourceType, TrancheInput, TrancheType, ValuationInfo, VaultId,
	MAX_ADAPTERS_PER_MULTICHAIN_ADAPTER, MAX_ADAPTERS_PER_SINGLE_CHAIN_PRODUCT,
	MAX_ADAPTER_INDEX_ENTRIES, MAX_COLLATERALS, MAX_MULTICHAIN_ADAPTERS, MAX_TRANCHES_PER_CHAIN,
	MAX_TRANCHE_MANAGERS,
};
use frame_benchmarking::v2::*;
use frame_support::{traits::EnsureOrigin, BoundedBTreeMap, BoundedVec};
use frame_system::RawOrigin;
use sp_core::{H160, U256};
use sp_std::{vec, vec::Vec};

const PID: ProductId = 1;

fn admin_origin<T: Config>() -> T::RuntimeOrigin {
	T::ProductAdminOrigin::try_successful_origin().expect("ProductAdminOrigin benchmark helper")
}

fn factory_origin<T: Config>() -> T::RuntimeOrigin {
	T::ProductFactoryOrigin::try_successful_origin().expect("ProductFactoryOrigin benchmark helper")
}

/// `seq` 1 under the permissionless prefix `prefix`.
fn permissionless_pid(prefix: u32) -> ProductId {
	((prefix as u64) << 32) | 1
}

fn h160(n: u64) -> H160 {
	H160::from_low_u64_be(n)
}

fn addr(n: u64) -> ChainAddress {
	h160(n).into()
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
/// then one Junior (priority `n-1`); a single Senior when `n == 1`. Satisfies
/// every per-chain composition rule.
fn chain_tranches<T: Config>(chain_id: u64, n: u32, vault_seed: &mut u64) -> Vec<TrancheInput> {
	(0..n)
		.map(|i| {
			*vault_seed += 1;
			let is_junior = n > 1 && i == n - 1;
			TrancheInput {
				priority: i as u8,
				tranche_type: if is_junior {
					TrancheType::Junior
				} else {
					TrancheType::Senior { apr: U256::from(i + 1) }
				},
				vault: VaultId { chain_id, vault_address: addr(*vault_seed) },
				asset: addr(0xa55e7),
				shares: addr(0x_5_a4e5),
			}
		})
		.collect()
}

/// `t` tranches filling chains `1, 2, …` up to `MAX_TRANCHES_PER_CHAIN` each.
fn tranches_n<T: Config>(
	t: u32,
) -> BoundedVec<TrancheInput, ConstU32<{ crate::MAX_TRANCHE_INPUTS }>> {
	let mut seed = 1_000u64;
	let mut all = Vec::new();
	let (mut left, mut chain) = (t, 1u64);
	while left > 0 {
		let n = left.min(MAX_TRANCHES_PER_CHAIN);
		all.extend(chain_tranches::<T>(chain, n, &mut seed));
		left -= n;
		chain += 1;
	}
	BoundedVec::try_from(all).expect("MAX_TRANCHE_INPUTS")
}

/// `weight_bps` of entry `j` among `n`, so the set sums to exactly 10_000.
fn bps(j: u32, n: u32) -> u16 {
	(10_000 / n) as u16 + if j == 0 { (10_000 % n) as u16 } else { 0 }
}

/// A worst-case-encoding adapter: `OffchainSource` with `MAX_COLLATERALS` NFTs.
fn offchain_adapter(seed: u64, weight_bps: u16) -> AdapterInfo {
	let collaterals = BoundedVec::try_from(
		(0..MAX_COLLATERALS)
			.map(|k| crate::CollateralAsset {
				chain_id: 900 + k as u64,
				nft_contract: addr(0x2_000_000 + seed * 10 + k as u64),
				nft_token_id: U256::from(k),
			})
			.collect::<Vec<_>>(),
	)
	.expect("MAX_COLLATERALS");
	AdapterInfo {
		source_type: SourceType::OffchainSource { borrower: addr(0xb0_0000 + seed), collaterals },
		weight_bps,
	}
}

fn nested_adapters_n<T: Config>(
	parent: u64,
	n: u32,
) -> BoundedBTreeMap<ChainAddress, AdapterInfo, ConstU32<MAX_ADAPTERS_PER_MULTICHAIN_ADAPTER>> {
	let mut m = BoundedBTreeMap::new();
	for j in 0..n {
		let seed = parent * 1_000 + j as u64;
		m.try_insert(addr(0x1_000_000 + seed), offchain_adapter(seed, bps(j, n)))
			.expect("nested adapter insert");
	}
	m
}

/// Nested-adapter counts for `a` adapter-index entries: `m = ceil(a / (1 +
/// MAX_ADAPTERS_PER_MULTICHAIN_ADAPTER))` MultichainAdapters, with the other `a - m`
/// nested adapters spread evenly over them (each gets at least one and at most
/// `MAX_ADAPTERS_PER_MULTICHAIN_ADAPTER`). Requires `a >= 2`.
fn nested_counts(a: u32) -> Vec<u32> {
	let per = 1 + MAX_ADAPTERS_PER_MULTICHAIN_ADAPTER;
	let m = (a + per - 1) / per;
	let (base, extra) = ((a - m) / m, (a - m) % m);
	(0..m).map(|i| base + u32::from(i < extra)).collect()
}

/// One MultichainAdapter per entry of `nested`, holding that many worst-case
/// adapters. `seed` keeps addresses disjoint between sets built for the same
/// product (e.g. the old and the replacement table).
fn mcas<T: Config>(
	nested: &[u32],
	seed: u64,
) -> BoundedBTreeMap<AdapterKey, MultichainAdapterInfo, ConstU32<MAX_MULTICHAIN_ADAPTERS>> {
	let m = nested.len() as u32;
	let mut out = BoundedBTreeMap::new();
	for (i, n) in nested.iter().enumerate() {
		let i = i as u32;
		let key_seed = seed + i as u64;
		out.try_insert(
			AdapterKey { address: addr(0x9_000_000 + key_seed), chain_id: 100 + i as u64 },
			MultichainAdapterInfo {
				weight_bps: bps(i, m),
				adapters: nested_adapters_n::<T>(key_seed, *n),
			},
		)
		.expect("multichain adapter insert");
	}
	out
}

fn multichain_adapters_n<T: Config>(
	a: u32,
) -> BoundedBTreeMap<AdapterKey, MultichainAdapterInfo, ConstU32<MAX_MULTICHAIN_ADAPTERS>> {
	mcas::<T>(&nested_counts(a), 0)
}

/// Create multichain product `PID` with `t` tranches and the given adapter table.
fn setup_product<T: Config>(
	t: u32,
	adapters: BoundedBTreeMap<AdapterKey, MultichainAdapterInfo, ConstU32<MAX_MULTICHAIN_ADAPTERS>>,
) {
	Pallet::<T>::create_product(
		admin_origin::<T>(),
		PID,
		valuation(),
		tranches_n::<T>(t),
		adapters,
		full_managers(),
	)
	.expect("create_product setup");
}

fn full_managers() -> BoundedBTreeMap<u64, ChainAddress, ConstU32<MAX_TRANCHE_MANAGERS>> {
	let mut m = BoundedBTreeMap::new();
	for c in 1..=MAX_TRANCHE_MANAGERS as u64 {
		m.try_insert(c, addr(0x3_000_000 + c)).expect("manager insert");
	}
	m
}

/// Single-chain product inputs on chain 7 with `t` tranches and `a`
/// worst-case-encoding adapters: `(tranches, adapters, valuation)`.
fn single_chain_inputs<T: Config>(
	t: u32,
	a: u32,
) -> (
	BoundedVec<TrancheInput, ConstU32<MAX_TRANCHES_PER_CHAIN>>,
	BoundedBTreeMap<ChainAddress, AdapterInfo, ConstU32<MAX_ADAPTERS_PER_SINGLE_CHAIN_PRODUCT>>,
	SingleChainValuationInfo,
) {
	let mut seed = 5_000u64;
	let tranches =
		BoundedVec::try_from(chain_tranches::<T>(7, t, &mut seed)).expect("MAX_TRANCHES_PER_CHAIN");
	let mut adapters: BoundedBTreeMap<
		ChainAddress,
		AdapterInfo,
		ConstU32<MAX_ADAPTERS_PER_SINGLE_CHAIN_PRODUCT>,
	> = BoundedBTreeMap::new();
	for j in 0..a {
		let seed = 70_000 + j as u64;
		adapters
			.try_insert(addr(0x4_000_000 + j as u64), offchain_adapter(seed, bps(j, a)))
			.expect("sc adapter insert");
	}
	let val = SingleChainValuationInfo {
		base_asset: addr(0xba5e),
		valuation_address: addr(0x7a1),
		settlement_mode: SettlementMode::Async {
			settlement_start_timestamp: 4_000_000_000,
			settlement_length_secs: 86_400,
			settlement_offset_secs: 3_600,
		},
	};
	(tranches, adapters, val)
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn create_product(
		t: Linear<1, { crate::MAX_TRANCHE_INPUTS }>,
		a: Linear<2, { MAX_MULTICHAIN_ADAPTERS * (1 + MAX_ADAPTERS_PER_MULTICHAIN_ADAPTER) }>,
	) {
		let v = valuation();
		#[extrinsic_call]
		_(
			admin_origin::<T>() as T::RuntimeOrigin,
			PID,
			v,
			tranches_n::<T>(t),
			multichain_adapters_n::<T>(a),
			full_managers(),
		);
		assert!(Products::<T>::contains_key(PID));
	}

	#[benchmark]
	fn create_single_chain_product(
		t: Linear<1, MAX_TRANCHES_PER_CHAIN>,
		a: Linear<1, MAX_ADAPTERS_PER_SINGLE_CHAIN_PRODUCT>,
	) {
		let (tranches, adapters, val) = single_chain_inputs::<T>(t, a);

		#[extrinsic_call]
		_(
			admin_origin::<T>() as T::RuntimeOrigin,
			PID,
			7u64,
			val,
			tranches,
			addr(0x6ed_0),
			adapters,
			addr(0x_1ed6e7),
		);

		assert!(Products::<T>::contains_key(PID));
	}

	/// `Add` at priority 0 (shifting every tranche on that chain) to a product of
	/// `t` tranches and `a` adapter-index entries. `t <= MAX_TRANCHE_INPUTS - 1` so
	/// there is always room: the target chain `t / MAX_TRANCHES_PER_CHAIN + 1` is
	/// the last, partially filled one (or the next, empty one).
	#[benchmark]
	fn set_tranche(
		t: Linear<1, { crate::MAX_TRANCHE_INPUTS - 1 }>,
		a: Linear<2, MAX_ADAPTER_INDEX_ENTRIES>,
	) {
		setup_product::<T>(t, multichain_adapters_n::<T>(a));
		let chain_id = (t / MAX_TRANCHES_PER_CHAIN + 1) as u64;
		let new_vault = VaultId { chain_id, vault_address: addr(0x9_999_999) };

		#[extrinsic_call]
		_(
			admin_origin::<T>() as T::RuntimeOrigin,
			PID,
			CrudAction::Add,
			new_vault.clone(),
			TrancheType::Senior { apr: U256::from(1_000_000) },
			addr(0xa55e7),
			addr(0x_5_a4e5),
			0u8,
		);

		assert!(Vaults::<T>::contains_key(&new_vault));
	}

	/// Replace a MultichainAdapter's single nested adapter with `n` new ones, on a
	/// product of `t` tranches whose other MultichainAdapters hold `a` index
	/// entries. The replaced adapters' index removals are charged separately (one
	/// write each — see `Pallet::set_adapters_weight`).
	#[benchmark]
	fn set_adapters(
		t: Linear<1, { crate::MAX_TRANCHE_INPUTS }>,
		a: Linear<2, { MAX_ADAPTER_INDEX_ENTRIES - 1 - MAX_ADAPTERS_PER_MULTICHAIN_ADAPTER }>,
		n: Linear<1, MAX_ADAPTERS_PER_MULTICHAIN_ADAPTER>,
	) {
		// Target parent (index 0, one nested adapter) + `a` entries of filler.
		let mut counts = vec![1u32];
		counts.extend(nested_counts(a));
		setup_product::<T>(t, mcas::<T>(&counts, 0));
		let mut replacement: BoundedBTreeMap<
			ChainAddress,
			AdapterInfo,
			ConstU32<MAX_ADAPTERS_PER_MULTICHAIN_ADAPTER>,
		> = BoundedBTreeMap::new();
		for j in 0..n {
			let seed = 900_000 + j as u64;
			replacement
				.try_insert(addr(0x5_000_000 + j as u64), offchain_adapter(seed, bps(j, n)))
				.expect("insert");
		}

		#[extrinsic_call]
		_(admin_origin::<T>() as T::RuntimeOrigin, PID, addr(0x9_000_000), 100u64, replacement);
	}

	/// Replace a product's whole MultichainAdapter table (`o` index entries) with a
	/// disjoint one of `n` entries, on a product of `t` tranches.
	#[benchmark]
	fn set_multichain_adapters(
		t: Linear<1, { crate::MAX_TRANCHE_INPUTS }>,
		o: Linear<2, MAX_ADAPTER_INDEX_ENTRIES>,
		n: Linear<2, MAX_ADAPTER_INDEX_ENTRIES>,
	) {
		setup_product::<T>(t, mcas::<T>(&nested_counts(o), 0));
		let replacement = mcas::<T>(&nested_counts(n), 500);

		#[extrinsic_call]
		_(admin_origin::<T>() as T::RuntimeOrigin, PID, replacement);
	}

	#[benchmark]
	fn set_orchestrator_address() {
		#[extrinsic_call]
		_(RawOrigin::Root, h160(0x0_c_4e_5));
		assert_eq!(OrchestratorAddress::<T>::get(), h160(0x0_c_4e_5));
	}

	#[benchmark]
	fn set_multichain_tranche_managers(
		t: Linear<1, { crate::MAX_TRANCHE_INPUTS }>,
		a: Linear<2, MAX_ADAPTER_INDEX_ENTRIES>,
	) {
		setup_product::<T>(t, multichain_adapters_n::<T>(a));
		let mut m: BoundedBTreeMap<u64, ChainAddress, ConstU32<MAX_TRANCHE_MANAGERS>> =
			BoundedBTreeMap::new();
		for c in 1..=MAX_TRANCHE_MANAGERS as u64 {
			m.try_insert(c, addr(0x7_000_000 + c)).expect("insert");
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

	#[benchmark]
	fn create_product_permissionless(
		t: Linear<1, { crate::MAX_TRANCHE_INPUTS }>,
		a: Linear<2, { MAX_MULTICHAIN_ADAPTERS * (1 + MAX_ADAPTERS_PER_MULTICHAIN_ADAPTER) }>,
	) {
		let pid = permissionless_pid(crate::PRODUCT_ID_PREFIX_PERMISSIONLESS_MULTICHAIN);
		let admin: T::AccountId = frame_benchmarking::account("manager", 0, 0);
		#[extrinsic_call]
		_(
			factory_origin::<T>() as T::RuntimeOrigin,
			pid,
			admin,
			valuation(),
			tranches_n::<T>(t),
			multichain_adapters_n::<T>(a),
			full_managers(),
		);
		assert!(Products::<T>::contains_key(pid));
	}

	#[benchmark]
	fn create_single_chain_product_permissionless(
		t: Linear<1, MAX_TRANCHES_PER_CHAIN>,
		a: Linear<1, MAX_ADAPTERS_PER_SINGLE_CHAIN_PRODUCT>,
	) {
		let pid = permissionless_pid(crate::PRODUCT_ID_PREFIX_PERMISSIONLESS_SINGLE_CHAIN);
		let admin: T::AccountId = frame_benchmarking::account("manager", 0, 0);
		let (tranches, adapters, val) = single_chain_inputs::<T>(t, a);
		#[extrinsic_call]
		_(
			factory_origin::<T>() as T::RuntimeOrigin,
			pid,
			admin,
			7u64,
			val,
			tranches,
			addr(0x6ed_0),
			adapters,
			addr(0x_1ed6e7),
		);
		assert!(Products::<T>::contains_key(pid));
	}
}
