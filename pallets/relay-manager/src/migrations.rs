use super::*;
use frame_support::{pallet_prelude::*, storage_alias, traits::OnRuntimeUpgrade};

#[storage_alias]
pub type StorageVersion<T: Config> = StorageValue<Pallet<T>, Releases, ValueQuery>;

pub mod v4 {
	use super::*;
	use bp_staking::{RoundIndex, MAX_AUTHORITIES};
	use frame_support::BoundedBTreeSet;
	use sp_std::collections::{btree_map::BTreeMap, btree_set::BTreeSet};

	#[cfg(feature = "try-runtime")]
	use sp_runtime::TryRuntimeError;

	pub struct MigrateToV4<T>(PhantomData<T>);

	impl<T: Config> OnRuntimeUpgrade for MigrateToV4<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();

			let current = Pallet::<T>::current_storage_version();
			let onchain = StorageVersion::<T>::get();

			if current == 4 && onchain == Releases::V3_0_0 {
				// closure for translate old selected relayers format to new selected relayers format
				let old_selected_to_new =
					|old: Option<BoundedVec<T::AccountId, ConstU32<MAX_AUTHORITIES>>>| {
						let new: BoundedBTreeSet<T::AccountId, ConstU32<MAX_AUTHORITIES>> = old
							.expect("")
							.into_iter()
							.collect::<BTreeSet<T::AccountId>>()
							.try_into()
							.expect("");
						Some(new)
					};
				<SelectedRelayers<T>>::translate::<
					BoundedVec<T::AccountId, ConstU32<MAX_AUTHORITIES>>,
					_,
				>(old_selected_to_new)
				.expect("");
				<InitialSelectedRelayers<T>>::translate::<
					BoundedVec<T::AccountId, ConstU32<MAX_AUTHORITIES>>,
					_,
				>(old_selected_to_new)
				.expect("");
				weight = weight.saturating_add(T::DbWeight::get().reads_writes(2, 2));

				// closure for translate old Cached*SelectedRelayers format to new Cached*SelectedRelayers format
				let old_cache_to_new = |old: Option<Vec<(RoundIndex, Vec<T::AccountId>)>>| {
					let new: BTreeMap<
						RoundIndex,
						BoundedBTreeSet<T::AccountId, ConstU32<MAX_AUTHORITIES>>,
					> = old.expect("")
						.into_iter()
						.map(|(round_index, vec_ids)| {
							let set_ids: BoundedBTreeSet<T::AccountId, ConstU32<MAX_AUTHORITIES>> =
								vec_ids
									.into_iter()
									.collect::<BTreeSet<T::AccountId>>()
									.try_into()
									.expect("");
							(round_index, set_ids)
						})
						.collect();

					Some(new)
				};
				<CachedSelectedRelayers<T>>::translate::<Vec<(RoundIndex, Vec<T::AccountId>)>, _>(
					old_cache_to_new,
				)
				.expect("");
				<CachedInitialSelectedRelayers<T>>::translate::<
					Vec<(RoundIndex, Vec<T::AccountId>)>,
					_,
				>(old_cache_to_new)
				.expect("");
				weight = weight.saturating_add(T::DbWeight::get().reads_writes(2, 2));

				// closure for translate old Cached*Majority format to new Cached*Majority format
				let old_cache_to_new =
					|old: Option<Vec<(RoundIndex, u32)>>| -> Option<BTreeMap<RoundIndex, u32>> {
						Some(old.expect("").into_iter().collect::<BTreeMap<RoundIndex, u32>>())
					};
				<CachedMajority<T>>::translate::<Vec<(RoundIndex, u32)>, _>(old_cache_to_new)
					.expect("");
				<CachedInitialMajority<T>>::translate::<Vec<(RoundIndex, u32)>, _>(
					old_cache_to_new,
				)
				.expect("");
				weight = weight.saturating_add(T::DbWeight::get().reads_writes(2, 2));

				// migrate to new standard storage version
				StorageVersion::<T>::kill();
				current.put::<Pallet<T>>();

				log!(info, "relay-manager storage migration passes v4 update ✅");
				weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 2));
			} else {
				log!(warn, "Skipping v4, should be removed");
				weight = weight.saturating_add(T::DbWeight::get().reads(1));
			}

			weight
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
			ensure!(
				StorageVersion::<T>::get() == Releases::V3_0_0,
				"Required v3_0_0 before upgrading to v4"
			);

			Ok(Default::default())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(_state: Vec<u8>) -> Result<(), TryRuntimeError> {
			ensure!(Pallet::<T>::on_chain_storage_version() == 4, "v4 not applied");

			ensure!(!StorageVersion::<T>::exists(), "Storage version not migrated correctly");

			Ok(())
		}
	}
}

pub mod v3 {
	use super::*;
	use frame_support::{pallet_prelude::Weight, traits::Get};

	#[derive(Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
	pub struct OldRelayerState<AccountId> {
		pub controller: AccountId,
		pub status: RelayerStatus,
	}

	pub fn pre_migrate<T: Config>() -> Result<(), &'static str> {
		frame_support::ensure!(
			StorageVersion::<T>::get() == Releases::V1_0_0
				|| StorageVersion::<T>::get() == Releases::V2_0_0,
			"Storage version must match to v1.0.0 or v2.0.0",
		);
		log::info!("relay-manager storage migration passes pre-migrate checks ✅");
		Ok(())
	}

	pub fn migrate<T: Config>() -> Weight {
		RelayerState::<T>::translate(|_key, old: OldRelayerState<T::AccountId>| {
			Some(RelayerMetadata {
				controller: old.controller,
				status: old.status,
				impl_version: None,
				spec_version: None,
			})
		});
		StorageVersion::<T>::put(Releases::V3_0_0);
		log::info!("relay-manager storage migration passes Releases::V3_0_0 update ✅");
		T::BlockWeights::get().max_block
	}
}

pub mod v2 {
	use super::*;
	use frame_support::{pallet_prelude::Weight, traits::Get};

	pub fn pre_migrate<T: Config>() -> Result<(), &'static str> {
		frame_support::ensure!(
			StorageVersion::<T>::get() == Releases::V1_0_0,
			"Storage version must match to v1.0.0",
		);
		Ok(())
	}

	pub fn migrate<T: Config>() -> Weight {
		// RelayerPool::<T>::get().clone().into_iter().for_each(|r| {
		// 	RelayerState::<T>::insert(
		// 		&r.relayer,
		// 		RelayerMetadata { controller: r.controller, status: RelayerStatus::Active },
		// 	);
		// });
		IsHeartbeatOffenceActive::<T>::put(false);
		HeartbeatSlashFraction::<T>::put(Perbill::from_percent(3));
		StorageVersion::<T>::put(Releases::V2_0_0);
		log::info!("relay-manager storage migration passes Releases::V2_0_0 update ✅");
		T::BlockWeights::get().max_block
	}
}

pub mod v1 {
	use super::*;
	use frame_support::{pallet_prelude::Weight, traits::Get};

	pub fn migrate<T: Config>() -> Weight {
		let selected_relayers = SelectedRelayers::<T>::get();
		InitialSelectedRelayers::<T>::put(selected_relayers);

		let cached_selected_relayers = CachedSelectedRelayers::<T>::get();
		CachedInitialSelectedRelayers::<T>::put(cached_selected_relayers);

		let majority = Majority::<T>::get();
		InitialMajority::<T>::put(majority);

		let cached_majority = CachedMajority::<T>::get();
		CachedInitialMajority::<T>::put(cached_majority);

		log::info!("relay-manager storage migration passes Releases::V1_0_0 update ✅");
		T::BlockWeights::get().max_block
	}
}
