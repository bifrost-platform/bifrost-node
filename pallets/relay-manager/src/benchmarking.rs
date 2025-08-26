#![cfg(feature = "runtime-benchmarks")]

use super::*;
use bp_staking::MAX_AUTHORITIES;
use frame_benchmarking::v2::*;
use frame_support::pallet_prelude::ConstU32;
use frame_support::BoundedBTreeSet;
use frame_system::RawOrigin;

fn set_controller<T: Config>() -> (T::AccountId, T::AccountId) {
	<Round<T>>::put(1);

	let controller = account::<T::AccountId>("controller", 0, 0);
	let relayer = account::<T::AccountId>("relayer", 0, 0);
	let relayer_metadata = RelayerMetadata {
		controller: controller.clone(),
		status: RelayerStatus::Active,
		impl_version: None,
		spec_version: None,
	};
	<RelayerState<T>>::insert(relayer.clone(), relayer_metadata);
	<BondedController<T>>::insert(controller.clone(), relayer.clone());

	(controller, relayer)
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn set_storage_cache_lifetime() {
		#[extrinsic_call]
		_(RawOrigin::Root, u32::MAX);
	}

	#[benchmark]
	fn set_heartbeat_offence_activation() {
		#[extrinsic_call]
		_(RawOrigin::Root, true);
	}

	#[benchmark]
	fn set_heartbeat_slash_fraction() {
		#[extrinsic_call]
		_(RawOrigin::Root, Perbill::from_percent(10));
	}

	#[benchmark]
	fn set_relayer() {
		let (controller, _) = set_controller::<T>();
		let new_relayer = account::<T::AccountId>("relayer", 1, 0);

		#[extrinsic_call]
		_(RawOrigin::Signed(controller), new_relayer);
	}

	#[benchmark]
	fn cancel_relayer_set() {
		let (controller, _) = set_controller::<T>();
		let new = account::<T::AccountId>("relayer", 1, 0);
		let _ = Pallet::<T>::set_relayer(RawOrigin::Signed(controller.clone()).into(), new);

		#[extrinsic_call]
		_(RawOrigin::Signed(controller));
	}

	#[benchmark]
	fn heartbeat() {
		let (_, relayer) = set_controller::<T>();
		let mut selected_relayers: BoundedBTreeSet<T::AccountId, ConstU32<MAX_AUTHORITIES>> =
			Default::default();
		selected_relayers.try_insert(relayer.clone()).unwrap();
		<SelectedRelayers<T>>::put(selected_relayers);

		#[extrinsic_call]
		_(RawOrigin::Signed(relayer));
	}

	#[benchmark]
	fn heartbeat_v2() {
		let (_, relayer) = set_controller::<T>();
		let mut selected_relayers: BoundedBTreeSet<T::AccountId, ConstU32<MAX_AUTHORITIES>> =
			Default::default();
		selected_relayers.try_insert(relayer.clone()).unwrap();
		<SelectedRelayers<T>>::put(selected_relayers);

		#[extrinsic_call]
		_(RawOrigin::Signed(relayer), 1, T::Hash::default());
	}

	impl_benchmark_test_suite!(Pallet, mock::new_test_ext(), mock::Test);
}
