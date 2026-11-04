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
use frame_support::traits::EnsureOrigin;
use frame_system::RawOrigin;
use pallet_tranche_system::{ChainAddress, ProductId, VaultId};

const PRODUCT_ID: ProductId = 7;

/// A `ProductAdminOrigin` origin and the account it carries, installed as
/// `PRODUCT_ID`'s admin.
fn product_admin<T: Config>() -> T::RuntimeOrigin {
	let origin = T::ProductAdminOrigin::try_successful_origin()
		.expect("ProductAdminOrigin benchmark helper");
	let admin = T::ProductAdminOrigin::try_origin(origin.clone())
		.ok()
		.expect("successful origin yields an account");
	ProductAdmins::<T>::insert(PRODUCT_ID, &admin);
	origin
}

fn vault() -> VaultId {
	VaultId { chain_id: 1, vault_address: ChainAddress::repeat_byte(0xa1) }
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn grant_permission() {
		let origin = product_admin::<T>();
		let who: T::AccountId = account::<T::AccountId>("feeder", 0, 0);

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, PRODUCT_ID, Role::OracleFeeder, who.clone());

		assert!(OracleFeeders::<T>::contains_key(PRODUCT_ID, &who));
	}

	#[benchmark]
	fn revoke_permission() {
		let origin = product_admin::<T>();
		let who: T::AccountId = account::<T::AccountId>("feeder", 0, 0);
		OracleFeeders::<T>::insert(PRODUCT_ID, &who, ());

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, PRODUCT_ID, Role::OracleFeeder, who.clone());

		assert!(!OracleFeeders::<T>::contains_key(PRODUCT_ID, &who));
	}

	#[benchmark]
	fn grant_tranche_investor() {
		let origin = product_admin::<T>();
		let investor = ChainAddress::repeat_byte(0xb2);
		let v = vault();

		T::BenchmarkHelper::setup_multichain_vault(PRODUCT_ID, v.clone());

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, PRODUCT_ID, v.clone(), investor);

		assert!(TrancheInvestors::<T>::contains_key((PRODUCT_ID, &v, &investor)));
	}

	#[benchmark]
	fn revoke_tranche_investor() {
		let origin = product_admin::<T>();
		let investor = ChainAddress::repeat_byte(0xb2);
		let v = vault();

		T::BenchmarkHelper::setup_multichain_vault(PRODUCT_ID, v.clone());
		TrancheInvestors::<T>::insert((PRODUCT_ID, &v, &investor), ());

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, PRODUCT_ID, v.clone(), investor);

		assert!(!TrancheInvestors::<T>::contains_key((PRODUCT_ID, &v, &investor)));
	}

	#[benchmark]
	fn set_product_factory() {
		let old: T::AccountId = account::<T::AccountId>("factory", 0, 0);
		let new: T::AccountId = account::<T::AccountId>("factory", 1, 0);
		ProductFactory::<T>::put(&old);

		#[extrinsic_call]
		_(RawOrigin::Root, Some(new.clone()));

		assert_eq!(ProductFactory::<T>::get(), Some(new));
	}

	#[benchmark]
	fn force_set_product_admin() {
		let old: T::AccountId = account::<T::AccountId>("old", 0, 0);
		let new: T::AccountId = account::<T::AccountId>("new", 0, 0);
		ProductAdmins::<T>::insert(PRODUCT_ID, &old);

		#[extrinsic_call]
		_(RawOrigin::Root, PRODUCT_ID, Some(new.clone()));

		assert_eq!(ProductAdmins::<T>::get(PRODUCT_ID), Some(new));
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
