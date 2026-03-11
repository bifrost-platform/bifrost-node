use super::*;

pub mod v4 {
	use core::marker::PhantomData;

	use super::*;
	use frame_support::{
		traits::{Get, GetStorageVersion, OnRuntimeUpgrade},
		weights::Weight,
	};
	use sp_runtime::traits::Zero;

	/// Migration V4: Clear OnFlightTransfers and PendingTransfers, reset all AssetCaps.on_flight_cap to zero.
	///
	/// This migration cleans up in-flight state to ensure a consistent starting point.
	pub struct V4<T>(PhantomData<T>);

	impl<T: Config> OnRuntimeUpgrade for V4<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();

			let current = Pallet::<T>::in_code_storage_version();
			let onchain = Pallet::<T>::on_chain_storage_version();

			weight = weight.saturating_add(T::DbWeight::get().reads(2));

			if current == 4 && onchain == 3 {
				// Count existing entries for weight and logging
				let on_flight_count = OnFlightTransfers::<T>::iter().count() as u64;
				let pending_count = PendingTransfers::<T>::iter().count() as u64;
				let asset_caps_count = AssetCaps::<T>::iter().count() as u64;

				weight = weight.saturating_add(
					T::DbWeight::get().reads(on_flight_count + pending_count + asset_caps_count),
				);

				// Clear all OnFlightTransfers
				let _ = OnFlightTransfers::<T>::clear(u32::MAX, None);
				weight = weight.saturating_add(T::DbWeight::get().writes(on_flight_count));

				// Clear all PendingTransfers
				let _ = PendingTransfers::<T>::clear(u32::MAX, None);
				weight = weight.saturating_add(T::DbWeight::get().writes(pending_count));

				// Reset on_flight_cap to zero for all AssetCaps entries
				AssetCaps::<T>::translate_values(|mut cap: AssetCapInfo<BalanceOf<T>>| {
					cap.on_flight_cap = Zero::zero();
					Some(cap)
				});
				weight = weight.saturating_add(T::DbWeight::get().writes(asset_caps_count));

				current.put::<Pallet<T>>();
				weight = weight.saturating_add(T::DbWeight::get().writes(1));

				log!(
					info,
					"cccp-relay-queue v4: cleared {} OnFlightTransfers, {} PendingTransfers, reset {} AssetCaps.on_flight_cap to zero ✅",
					on_flight_count,
					pending_count,
					asset_caps_count
				);
			} else {
				log!(warn, "Skipping cccp-relay-queue storage v4 💤");
			}

			weight
		}
	}
}
