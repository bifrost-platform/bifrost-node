//! Benchmarks for `pallet-tranche-investments`.
//!
//! `T::Vaults`/`T::Adapters` (pallet-tranche-system) are seeded through
//! `T::BenchmarkHelper` so each extrinsic reaches its full body. Array inputs
//! are filled to their bounds; `record_investment_approvals` carries a linear
//! `n` component (its `WeightInfo` signature already does).

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::{
	AdapterValuation, Allocation, AssetPosition, BenchmarkHelper, InvestmentApprovalInput,
	OrderType, TrancheSettle, MAX_ADAPTER_VALUATIONS, MAX_ALLOCATIONS, MAX_ASSET_POSITIONS,
	MAX_SETTLEMENT_REQUESTS,
};
use frame_benchmarking::v2::*;
use frame_support::{traits::EnsureOrigin, BoundedVec};
use pallet_tranche_system::ChainAddress;
use pallet_tranche_system::{AdapterKey, VaultId, MAX_TRANCHE_INPUTS};
use sp_core::{H160, H256, U256};
use sp_std::{vec, vec::Vec};

const PID: ProductId = 1;
const SID_LOW: u128 = 7;

fn origin<T: Config>() -> T::RuntimeOrigin {
	T::ValuationOrigin::try_successful_origin().expect("ValuationOrigin benchmark helper")
}
fn h160(n: u64) -> ChainAddress {
	H160::from_low_u64_be(n).into()
}
fn sid(n: u128) -> SettlementId {
	U256::from(n)
}
fn rid(n: u64) -> RequestId {
	H256::from_low_u64_be(n)
}

fn vault(seed: u64) -> VaultId {
	VaultId { chain_id: 1 + seed % 10, vault_address: h160(0x10_000 + seed) }
}
fn mc_adapter(i: u64) -> AdapterKey {
	AdapterKey { address: h160(0x90_000 + i), chain_id: 100 + i }
}
fn adapter(i: u64) -> AdapterKey {
	AdapterKey { address: h160(0x1_00_000 + i), chain_id: 100 + i % 10 }
}

/// Register `count` mc-adapters and return `count` allocations summing to `total`.
fn allocations<T: Config>(
	count: u32,
	total: U256,
) -> BoundedVec<Allocation, ConstU32<MAX_ALLOCATIONS>> {
	let each = total / U256::from(count.max(1));
	let mut v = Vec::new();
	let mut acc = U256::zero();
	for i in 0..count {
		T::BenchmarkHelper::register_multichain_adapter(PID, mc_adapter(i as u64));
		let amt = if i == count - 1 { total - acc } else { each };
		acc += amt;
		v.push(Allocation { adapter: mc_adapter(i as u64), amount: amt });
	}
	BoundedVec::try_from(v).expect("MAX_ALLOCATIONS")
}

fn seed_request<T: Config>(request_id: RequestId, amount: U256) {
	let v = vault(request_id.to_low_u64_be());
	T::BenchmarkHelper::register_vault(PID, v.clone());
	Pallet::<T>::record_investment_request(
		origin::<T>(),
		PID,
		request_id,
		sid(SID_LOW),
		v.chain_id,
		v.vault_address,
		h160(0x1_2_3),
		amount,
		OrderType::Deposit,
	)
	.expect("seed request");
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn record_investment_request() {
		let v = vault(1);
		T::BenchmarkHelper::register_vault(PID, v.clone());

		#[extrinsic_call]
		_(
			origin::<T>() as T::RuntimeOrigin,
			PID,
			rid(1),
			sid(SID_LOW),
			v.chain_id,
			v.vault_address,
			h160(0x1_2_3),
			U256::from(1_000_000u64),
			OrderType::Deposit,
		);
		assert!(RequestedInvestments::<T>::contains_key(PID, rid(1)));
	}

	#[benchmark]
	fn record_investment_approval() {
		let amount = U256::from(1_000_000u64);
		seed_request::<T>(rid(1), amount);
		let allocs = allocations::<T>(MAX_ALLOCATIONS, amount);

		#[extrinsic_call]
		_(origin::<T>() as T::RuntimeOrigin, PID, rid(1), sid(SID_LOW), allocs, amount);

		assert!(ApprovedInvestments::<T>::contains_key(PID, rid(1)));
	}

	#[benchmark]
	fn record_investment_approvals(n: Linear<1, { MAX_SETTLEMENT_REQUESTS }>) {
		let amount = U256::from(1_000_000u64);
		T::BenchmarkHelper::register_multichain_adapter(PID, mc_adapter(0));
		let mut inputs = Vec::new();
		for i in 0..n {
			let r = rid(1000 + i as u64);
			seed_request::<T>(r, amount);
			inputs.push(InvestmentApprovalInput {
				request_id: r,
				allocations: BoundedVec::try_from(vec![Allocation {
					adapter: mc_adapter(0),
					amount,
				}])
				.unwrap(),
				receivable_amount: amount,
			});
		}
		let inputs = BoundedVec::try_from(inputs).expect("MAX_SETTLEMENT_REQUESTS");

		#[extrinsic_call]
		_(origin::<T>() as T::RuntimeOrigin, PID, sid(SID_LOW), inputs);

		assert!(ApprovedInvestments::<T>::contains_key(PID, rid(1000)));
	}

	#[benchmark]
	fn record_adapter_valuations() {
		let mut v = Vec::new();
		for i in 0..MAX_ADAPTER_VALUATIONS as u64 {
			let a = adapter(i);
			T::BenchmarkHelper::register_adapter(PID, a.clone());
			let positions = BoundedVec::try_from(
				(0..MAX_ASSET_POSITIONS)
					.map(|k| AssetPosition {
						asset: h160(0x5_00_000 + i * 100 + k as u64),
						amount: U256::from(k + 1),
						price_usd: U256::from(1_000_000u64),
						usd_value: U256::from(k + 1),
						counted: true,
					})
					.collect::<Vec<_>>(),
			)
			.expect("MAX_ASSET_POSITIONS");
			v.push(AdapterValuation {
				chain_id: a.chain_id,
				adapter: a.address,
				epoch_id: U256::from(i),
				valuation_cutoff: 1_700_000_000 + i,
				principal: U256::from(1_000u64),
				positions,
			});
		}
		let valuations = BoundedVec::try_from(v).expect("MAX_ADAPTER_VALUATIONS");

		#[extrinsic_call]
		_(origin::<T>() as T::RuntimeOrigin, PID, sid(SID_LOW), valuations);

		assert!(AdapterValuations::<T>::contains_key(PID, sid(SID_LOW)));
	}

	#[benchmark]
	fn record_settlement() {
		let mut v = Vec::new();
		for i in 0..MAX_TRANCHE_INPUTS as u64 {
			let vlt = vault(0x1000 + i);
			T::BenchmarkHelper::register_vault(PID, vlt.clone());
			v.push(TrancheSettle {
				vault: vlt,
				tranche_nav: U256::from(1_000u64),
				share_price: U256::from(1_000_000u64),
				units_outstanding: U256::from(500u64),
				principal: U256::from(100u64),
			});
		}
		let tranches = BoundedVec::try_from(v).expect("MAX_TRANCHE_INPUTS");

		#[extrinsic_call]
		_(
			origin::<T>() as T::RuntimeOrigin,
			PID,
			sid(SID_LOW),
			tranches,
			U256::from(1u64),
			U256::from(1u64),
		);

		assert!(Settlements::<T>::contains_key(PID, sid(SID_LOW)));
	}
}
