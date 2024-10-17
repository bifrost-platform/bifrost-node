use super::*;

pub mod init_v1 {
	use core::marker::PhantomData;

	use super::*;
	use bp_btc_relay::MigrationSequence;
	use frame_support::{
		traits::{Get, GetStorageVersion, OnRuntimeUpgrade},
		weights::Weight,
	};

	pub struct InitV1<T>(PhantomData<T>);

	impl<T: Config> OnRuntimeUpgrade for InitV1<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();

			let current = Pallet::<T>::current_storage_version();
			let onchain = Pallet::<T>::on_chain_storage_version();

			if current == 1 && onchain == 0 {
				MultiSigRatio::<T>::put(T::DefaultMultiSigRatio::get());
				CurrentRound::<T>::put(1);
				ServiceState::<T>::put(MigrationSequence::Normal);
				MaxPreSubmission::<T>::put(100);

				current.put::<Pallet<T>>();

				weight = weight.saturating_add(T::DbWeight::get().reads_writes(3, 5));
				log!(info, "btc-registration-pool storage migration passes init::v1 update ✅");
			} else {
				log!(warn, "Skipping btc-registration-pool storage init::v1 💤");
				weight = weight.saturating_add(T::DbWeight::get().reads(2));
			}
			weight
		}
	}
}
