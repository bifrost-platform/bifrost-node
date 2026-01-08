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

pub mod v2 {
	use super::*;
	use bp_cccp::traits::SocketVerifier;
	use core::marker::PhantomData;
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

			weight = weight.saturating_add(T::DbWeight::get().reads(2));

			if current == 2 && onchain == 1 {
				let outbound_pool = OutboundPool::<T>::get();
				weight = weight.saturating_add(T::DbWeight::get().reads(1));

				let to_remove: Vec<UnboundedBytes> = outbound_pool
					.iter()
					.filter(|raw_msg| {
						weight = weight.saturating_add(T::DbWeight::get().reads(1));
						T::SocketQueue::verify_socket_message(raw_msg).is_err()
					})
					.cloned()
					.collect();

				if !to_remove.is_empty() {
					OutboundPool::<T>::mutate(|pool| pool.retain(|m| !to_remove.contains(m)));
					weight = weight.saturating_add(T::DbWeight::get().writes(1));
					log!(
						info,
						"blaze v2: removed {} invalid socket messages from outbound pool ✅",
						to_remove.len()
					);
				}

				current.put::<Pallet<T>>();
				weight = weight.saturating_add(T::DbWeight::get().writes(1));
				log!(info, "blaze storage migration passes v2 update ✅");
			} else {
				log!(warn, "Skipping blaze storage v2 💤");
			}
			weight
		}
	}
}
