//! Benchmarks for `pallet-tranche-permissions`.
//!
//! Every extrinsic is O(1) — no loops, no unbounded storage — so each is
//! benchmarked once at its worst case. `grant_permission`/`revoke_permission`
//! use `Role::OracleFeeder` (`ProductAdmin` needs a root origin and touches
//! strictly less storage); the tranche-investor calls additionally read
//! pallet-tranche-system's `Vaults` (vault ownership) and `Products`
//! (`single_chain_id`) on top of this pallet's own `TrancheInvestors` n-map.

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::{BenchmarkHelper, Role};
use frame_benchmarking::v2::*;
use frame_system::RawOrigin;
use pallet_tranche_system::{ChainAddress, ProductId, VaultId};

const PRODUCT_ID: ProductId = 7;

fn vault() -> VaultId {
	VaultId { chain_id: 1, vault_address: ChainAddress::repeat_byte(0xa1) }
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn grant_permission() {
		let admin: T::AccountId = whitelisted_caller();
		let who: T::AccountId = account::<T::AccountId>("feeder", 0, 0);
		ProductAdmins::<T>::insert(PRODUCT_ID, &admin);

		#[extrinsic_call]
		_(RawOrigin::Signed(admin), PRODUCT_ID, Role::OracleFeeder, who.clone());

		assert!(OracleFeeders::<T>::contains_key(PRODUCT_ID, &who));
	}

	#[benchmark]
	fn revoke_permission() {
		let admin: T::AccountId = whitelisted_caller();
		let who: T::AccountId = account::<T::AccountId>("feeder", 0, 0);
		ProductAdmins::<T>::insert(PRODUCT_ID, &admin);
		OracleFeeders::<T>::insert(PRODUCT_ID, &who, ());

		#[extrinsic_call]
		_(RawOrigin::Signed(admin), PRODUCT_ID, Role::OracleFeeder, who.clone());

		assert!(!OracleFeeders::<T>::contains_key(PRODUCT_ID, &who));
	}

	#[benchmark]
	fn grant_tranche_investor() {
		let admin: T::AccountId = whitelisted_caller();
		let investor = ChainAddress::repeat_byte(0xb2);
		let v = vault();

		ProductAdmins::<T>::insert(PRODUCT_ID, &admin);
		T::BenchmarkHelper::setup_multichain_vault(PRODUCT_ID, v.clone());

		#[extrinsic_call]
		_(RawOrigin::Signed(admin), PRODUCT_ID, v.clone(), investor);

		assert!(TrancheInvestors::<T>::contains_key((PRODUCT_ID, &v, &investor)));
	}

	#[benchmark]
	fn revoke_tranche_investor() {
		let admin: T::AccountId = whitelisted_caller();
		let investor = ChainAddress::repeat_byte(0xb2);
		let v = vault();

		ProductAdmins::<T>::insert(PRODUCT_ID, &admin);
		T::BenchmarkHelper::setup_multichain_vault(PRODUCT_ID, v.clone());
		TrancheInvestors::<T>::insert((PRODUCT_ID, &v, &investor), ());

		#[extrinsic_call]
		_(RawOrigin::Signed(admin), PRODUCT_ID, v.clone(), investor);

		assert!(!TrancheInvestors::<T>::contains_key((PRODUCT_ID, &v, &investor)));
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
