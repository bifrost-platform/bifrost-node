//! Benchmarks for `pallet-tranche-permissions`.
//!
//! Both extrinsics are O(1) — no loops, no unbounded storage — so each is
//! benchmarked once at its worst case: `Role::TrancheInvestor(vault)`, the only
//! role whose path also reads pallet-tranche-system's `Vaults` (vault
//! ownership) and `Products` (`single_chain_id`) on top of this pallet's own
//! `TrancheInvestors` n-map read + write. `ProductAdmin` needs a root origin
//! and `OracleFeeder` touches strictly less storage.

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::{BenchmarkHelper, Role};
use frame_benchmarking::v2::*;
use frame_system::RawOrigin;
use pallet_tranche_system::{ProductId, VaultId};
use sp_core::H160;

const PRODUCT_ID: ProductId = 7;

fn vault() -> VaultId {
	VaultId { chain_id: 1, vault_address: H160::repeat_byte(0xa1) }
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn grant_permission() {
		let admin: T::AccountId = whitelisted_caller();
		let who: T::AccountId = account::<T::AccountId>("investor", 0, 0);
		let v = vault();

		ProductAdmins::<T>::insert(PRODUCT_ID, &admin);
		T::BenchmarkHelper::setup_multichain_vault(PRODUCT_ID, v.clone());

		#[extrinsic_call]
		_(RawOrigin::Signed(admin), PRODUCT_ID, Role::TrancheInvestor(v.clone()), who.clone());

		assert!(TrancheInvestors::<T>::contains_key((PRODUCT_ID, &v, &who)));
	}

	#[benchmark]
	fn revoke_permission() {
		let admin: T::AccountId = whitelisted_caller();
		let who: T::AccountId = account::<T::AccountId>("investor", 0, 0);
		let v = vault();

		ProductAdmins::<T>::insert(PRODUCT_ID, &admin);
		T::BenchmarkHelper::setup_multichain_vault(PRODUCT_ID, v.clone());
		TrancheInvestors::<T>::insert((PRODUCT_ID, &v, &who), ());

		#[extrinsic_call]
		_(RawOrigin::Signed(admin), PRODUCT_ID, Role::TrancheInvestor(v.clone()), who.clone());

		assert!(!TrancheInvestors::<T>::contains_key((PRODUCT_ID, &v, &who)));
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
