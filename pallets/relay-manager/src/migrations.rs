use super::*;

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
		RelayerPool::<T>::get().clone().into_iter().for_each(|r| {
			RelayerState::<T>::insert(
				&r.relayer,
				RelayerMetadata { controller: r.controller, status: RelayerStatus::Active },
			);
		});
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
