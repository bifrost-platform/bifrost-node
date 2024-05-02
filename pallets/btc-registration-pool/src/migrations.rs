use super::*;

pub mod v2 {
	use core::marker::PhantomData;

	use frame_support::{
		pallet_prelude::ValueQuery, storage_alias, traits::GetStorageVersion,
		traits::OnRuntimeUpgrade, weights::Weight,
	};
	use sp_core::Get;
	use sp_runtime::Percent;

	use super::*;

	#[storage_alias]
	pub type RequiredM<T: Config> = StorageValue<Pallet<T>, u8, ValueQuery>;

	#[storage_alias]
	pub type RequiredN<T: Config> = StorageValue<Pallet<T>, u8, ValueQuery>;

	pub struct MigrateToV2<T>(PhantomData<T>);

	impl<T: Config> OnRuntimeUpgrade for MigrateToV2<T> {
		fn on_runtime_upgrade() -> frame_support::weights::Weight {
			let mut weight = Weight::zero();

			let current = Pallet::<T>::current_storage_version();
			let onchain = Pallet::<T>::on_chain_storage_version();

			if current == 2 && onchain == 1 {
				RequiredM::<T>::kill();
				RequiredN::<T>::kill();

				<MultiSigRatio<T>>::put(Percent::from_percent(100));

				// translate `BondedRefund` to vector.
				<BondedRefund<T>>::translate(|_, old: T::AccountId| Some(vec![old]));

				current.put::<Pallet<T>>();

				weight = weight.saturating_add(T::DbWeight::get().reads_writes(0, 4));
				log!(info, "btc-registration-pool storage migration passes v2 update ✅");
			} else {
				log!(warn, "Skipping btc-registration-pool storage migration v2 💤");
				weight = weight.saturating_add(T::DbWeight::get().reads(1));
			}

			weight
		}
	}
}
