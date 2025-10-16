use super::*;

pub mod init_v1 {
	use core::marker::PhantomData;

	use super::*;
	use frame_support::{
		traits::{Get, GetStorageVersion, OnRuntimeUpgrade},
		weights::Weight,
	};

	pub struct InitV1<T>(PhantomData<T>);

	impl<T: Config> OnRuntimeUpgrade for InitV1<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();

			let current = Pallet::<T>::in_code_storage_version();
			let onchain = Pallet::<T>::on_chain_storage_version();

			if current == 1 && onchain == 0 {
				IsActivated::<T>::put(false);
				ToleranceCounter::<T>::put(0);

				current.put::<Pallet<T>>();
				weight = weight.saturating_add(T::DbWeight::get().reads_writes(2, 2));
				log!(info, "blaze storage migration passes init::v1 update ✅");
			} else {
				log!(warn, "Skipping blaze storage init::v1 💤");
				weight = weight.saturating_add(T::DbWeight::get().reads(2));
			}
			weight
		}
	}
}
