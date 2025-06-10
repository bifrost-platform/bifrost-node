use super::*;

pub mod v3 {
	use core::marker::PhantomData;

	use super::*;
	use frame_support::{
		traits::{Get, GetStorageVersion, OnRuntimeUpgrade},
		weights::Weight,
	};

	pub struct V3<T>(PhantomData<T>);

	impl<T: Config> OnRuntimeUpgrade for V3<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();

			let current = Pallet::<T>::in_code_storage_version();
			let onchain = Pallet::<T>::on_chain_storage_version();

			if current == 3 && onchain == 2 {
				// clear finalized requests
				let _ = <FinalizedRequests<T>>::clear(u32::MAX, None);

				current.put::<Pallet<T>>();
				weight = weight.saturating_add(T::DbWeight::get().reads_writes(2, 2));
				log!(info, "btc-socket-queue storage migration passes v3 update ✅");
			} else {
				log!(warn, "Skipping btc-socket-queue storage v3 💤");
				weight = weight.saturating_add(T::DbWeight::get().reads(2));
			}
			weight
		}
	}
}

pub mod init_v2 {
	use super::*;
	use core::marker::PhantomData;
	use frame_support::{
		traits::{Get, GetStorageVersion, OnRuntimeUpgrade},
		weights::Weight,
	};

	pub struct InitV2<T>(PhantomData<T>);

	impl<T: Config> OnRuntimeUpgrade for InitV2<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();

			let current = Pallet::<T>::in_code_storage_version();
			let onchain = Pallet::<T>::on_chain_storage_version();

			if current == 2 && onchain == 0 {
				current.put::<Pallet<T>>();

				weight = weight.saturating_add(T::DbWeight::get().reads_writes(2, 1));
				log!(info, "btc-socket-queue storage migration passes init::v2 update ✅");
			} else {
				log!(warn, "Skipping btc-socket-queue storage init::v2 💤");
				weight = weight.saturating_add(T::DbWeight::get().reads(2));
			}
			weight
		}
	}
}

pub mod v2 {
	use super::*;
	use core::marker::PhantomData;
	use frame_support::{
		traits::{Get, GetStorageVersion, OnRuntimeUpgrade},
		weights::Weight,
	};

	pub struct V2<T>(PhantomData<T>);

	impl<T> OnRuntimeUpgrade for V2<T>
	where
		T: Config,
		T::AccountId: Into<H160>,
		H160: Into<T::AccountId>,
	{
		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();

			let current = Pallet::<T>::in_code_storage_version();
			let onchain = Pallet::<T>::on_chain_storage_version();

			weight = weight.saturating_add(T::DbWeight::get().reads(2));

			let mut count: u32 = 0;

			if current == 2 && onchain == 1 {
				<SocketMessages<T>>::translate(|_, old: SocketMessage| {
					weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));

					Some((H256::zero(), old))
				});

				log!(
					info,
					"btc-socket-queue current socket messages count: {:?} ✅",
					<SocketMessages<T>>::iter_keys().count()
				);

				let mut insert_txid =
					|raw_msg: UnboundedBytes, txid: H256, mut weight: Weight| -> Weight {
						match Pallet::<T>::try_decode_socket_message(&raw_msg) {
							Ok(msg) => {
								if let Some(translated) =
									<SocketMessages<T>>::get(&msg.req_id.sequence)
								{
									<SocketMessages<T>>::insert(
										msg.req_id.sequence,
										(txid, translated.1),
									);
									weight = weight
										.saturating_add(T::DbWeight::get().reads_writes(1, 1));

									count = count.saturating_add(1);
								} else {
									log!(warn, "not found: {:?}", &msg.req_id.sequence);
								}
							},
							Err(_) => {
								log!(warn, "decode failed");
							},
						}
						weight
					};

				for request in <PendingRequests<T>>::iter() {
					weight = weight.saturating_add(T::DbWeight::get().reads(1));

					for raw_msg in request.1.socket_messages {
						weight = insert_txid(raw_msg, request.0, weight);
					}
				}
				for request in <FinalizedRequests<T>>::iter() {
					weight = weight.saturating_add(T::DbWeight::get().reads(1));

					for raw_msg in request.1.socket_messages {
						weight = insert_txid(raw_msg, request.0, weight);
					}
				}
				for request in <ExecutedRequests<T>>::iter() {
					weight = weight.saturating_add(T::DbWeight::get().reads(1));

					for raw_msg in request.1.socket_messages {
						weight = insert_txid(raw_msg, request.0, weight);
					}
				}
				current.put::<Pallet<T>>();
				log!(info, "btc-socket-queue translated socket messages count: {:?} ✅", count);
				log!(info, "btc-socket-queue storage migration passes v2 update ✅");
				weight = weight.saturating_add(T::DbWeight::get().writes(1));
			} else {
				log!(warn, "Skipping btc-socket-queue storage v2 💤");
			}
			weight
		}
	}
}

pub mod init_v1 {
	use super::*;
	use core::marker::PhantomData;
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
				current.put::<Pallet<T>>();

				weight = weight.saturating_add(T::DbWeight::get().reads_writes(2, 1));
				log!(info, "btc-socket-queue storage migration passes init::v1 update ✅");
			} else {
				log!(warn, "Skipping btc-socket-queue storage init::v1 💤");
				weight = weight.saturating_add(T::DbWeight::get().reads(2));
			}
			weight
		}
	}
}
