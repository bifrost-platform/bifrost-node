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
		FullMaximumOffenceCount::<T>::put(10u32);
		BasicMaximumOffenceCount::<T>::put(5u32);
		ValidatorOffences::<T>::remove_all(None);
		StorageVersion::<T>::put(Releases::V2_0_0);
		log::info!("bfc-offences storage migration passes Releases::V2_0_0 update ✅");
		T::BlockWeights::get().max_block
	}
}

pub mod v1 {
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
		OffenceExpirationInSessions::<T>::put(5u32);
		// MaximumOffenceCount::<T>::put(5u32);
		IsOffenceActive::<T>::put(true);
		IsSlashActive::<T>::put(true);
		log::info!("bfc-offences storage migration passes Releases::V1_0_0 update ✅");
		T::BlockWeights::get().max_block
	}
}
