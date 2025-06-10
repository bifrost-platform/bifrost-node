use super::*;

pub mod v2 {
	use core::marker::PhantomData;

	use super::*;
	use frame_support::{
		traits::{Get, GetStorageVersion, OnRuntimeUpgrade},
		weights::Weight,
	};

	pub struct V2<T>(PhantomData<T>);

	impl<T: Config> OnRuntimeUpgrade for V2<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();

			let current = Pallet::<T>::in_code_storage_version();
			let onchain = Pallet::<T>::on_chain_storage_version();

			if current == 2 && onchain == 1 {
				// make all utxos available
				let utxos = Utxos::<T>::iter().collect::<Vec<_>>();
				for (hash, utxo) in utxos {
					if utxo.status != UtxoStatus::Available {
						Utxos::<T>::insert(hash, utxo);
					}
				}

				// clear pending txs
				let _ = <PendingTxs<T>>::clear(u32::MAX, None);

				current.put::<Pallet<T>>();
				weight = weight.saturating_add(T::DbWeight::get().reads_writes(2, 2));
				log!(info, "blaze storage migration passes v2 update ✅");
			} else {
				log!(warn, "Skipping blaze storage v2 💤");
				weight = weight.saturating_add(T::DbWeight::get().reads(2));
			}
			weight
		}
	}
}

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
