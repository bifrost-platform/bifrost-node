use super::*;

pub mod init_v3 {
	use super::*;
	use core::marker::PhantomData;
	use frame_support::{
		traits::{Get, GetStorageVersion, OnRuntimeUpgrade},
		weights::Weight,
	};

	pub struct InitV3<T>(PhantomData<T>);

	impl<T: Config> OnRuntimeUpgrade for InitV3<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();

			let current = Pallet::<T>::in_code_storage_version();
			let onchain = Pallet::<T>::on_chain_storage_version();

			if current == 3 && onchain == 2 {
				weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));

				current.put::<Pallet<T>>();

				// cleanup OnFlightTransfers
				for (src_chain_id, sequence_id, _transfer) in <OnFlightTransfers<T>>::iter() {
					<OnFlightTransfers<T>>::remove(src_chain_id, sequence_id);
				}

				// cleanup FinalizedTransfers
				for (src_chain_id, sequence_id, _transfer) in <FinalizedTransfers<T>>::iter() {
					<FinalizedTransfers<T>>::remove(src_chain_id, sequence_id);
				}

				// reset AssetCaps
				for (asset_id, asset_cap) in <AssetCaps<T>>::iter() {
					<AssetCaps<T>>::insert(
						asset_id,
						AssetCapInfo {
							max_on_flight_cap: asset_cap.max_on_flight_cap,
							on_flight_cap: Default::default(),
						},
					);
				}

				log!(info, "cccp-relay-queue storage migration passes init::v3 update ✅");
			} else {
				log!(warn, "Skipping cccp-relay-queue storage init::v3 💤");
				weight = weight.saturating_add(T::DbWeight::get().reads(1));
			}

			weight
		}
	}
}

pub mod init_v2 {
	use super::*;
	use crate::pallet::pallet as current_pallet;
	use core::marker::PhantomData;
	use frame_support::{
		storage::migration::storage_key_iter,
		traits::{Get, GetStorageVersion, OnRuntimeUpgrade},
		weights::Weight,
		Twox64Concat,
	};
	use sp_core::H256;

	/// The old TransferInfo structure (V1) with src_tx_id instead of sequence_id.
	#[derive(
		parity_scale_codec::Decode,
		parity_scale_codec::Encode,
		scale_info::TypeInfo,
		Clone,
		PartialEq,
		Eq,
		sp_core::RuntimeDebug,
	)]
	pub struct OldTransferInfo<Balance, AccountId> {
		pub amount: Balance,
		pub src_tx_id: H256,
		pub src_chain_id: ChainId,
		pub dst_chain_id: ChainId,
		pub asset_index_hash: AssetIndexHash,
		pub option: TransferOption,
		pub status: TransferStatus,
		pub socket_message: bp_cccp::UnboundedBytes,
		pub on_flight_voters: BoundedVec<AccountId, ConstU32<{ bp_staking::MAX_AUTHORITIES }>>,
		pub finalization_voters: BoundedVec<AccountId, ConstU32<{ bp_staking::MAX_AUTHORITIES }>>,
	}

	/// Pallet prefix for storage.
	/// This must match the pallet name in construct_runtime! (CCCPRelayQueue).
	const PALLET_NAME: &[u8] = b"CCCPRelayQueue";

	pub struct InitV2<T>(PhantomData<T>);

	impl<T: Config> OnRuntimeUpgrade for InitV2<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();

			let current = Pallet::<T>::in_code_storage_version();
			let onchain = Pallet::<T>::on_chain_storage_version();

			if current == 2 && onchain == 1 {
				log!(info, "Starting cccp-relay-queue storage migration v1 → v2");

				// Migrate OnFlightTransfers
				let on_flight_count = migrate_on_flight_transfers::<T>(&mut weight);
				log!(info, "Migrated {} OnFlightTransfers entries", on_flight_count);

				// Migrate FinalizedTransfers
				let finalized_count = migrate_finalized_transfers::<T>(&mut weight);
				log!(info, "Migrated {} FinalizedTransfers entries", finalized_count);

				// Update storage version
				current.put::<Pallet<T>>();
				weight = weight.saturating_add(T::DbWeight::get().writes(1));

				log!(info, "cccp-relay-queue storage migration passes init::v2 update ✅");
			} else {
				log!(warn, "Skipping cccp-relay-queue storage init::v2 💤");
				weight = weight.saturating_add(T::DbWeight::get().reads(2));
			}
			weight
		}
	}

	/// Migrate OnFlightTransfers from (ChainId, H256) to (ChainId, U256) key structure
	fn migrate_on_flight_transfers<T: Config>(weight: &mut Weight) -> u32 {
		let mut count = 0u32;

		for ((src_chain_id, _src_tx_id), old_transfer) in storage_key_iter::<
			(ChainId, H256),
			OldTransferInfo<BalanceOf<T>, T::AccountId>,
			Twox64Concat,
		>(PALLET_NAME, b"OnFlightTransfers")
		.drain()
		{
			*weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));

			if let Some(sequence_id) =
				extract_sequence_id_from_socket_message(&old_transfer.socket_message)
			{
				let new_transfer = TransferInfo {
					amount: old_transfer.amount,
					sequence_id,
					src_tx_id: old_transfer.src_tx_id,
					src_chain_id: old_transfer.src_chain_id,
					dst_chain_id: old_transfer.dst_chain_id,
					asset_index_hash: old_transfer.asset_index_hash,
					option: old_transfer.option,
					status: old_transfer.status,
					socket_message: old_transfer.socket_message,
					on_flight_voters: old_transfer.on_flight_voters,
					finalization_voters: old_transfer.finalization_voters,
				};

				current_pallet::OnFlightTransfers::<T>::insert(
					src_chain_id,
					sequence_id,
					new_transfer,
				);
				*weight = weight.saturating_add(T::DbWeight::get().writes(1));
				count += 1;
			} else {
				log!(
					warn,
					"Failed to parse socket message for OnFlight transfer src_chain_id={}, skipping",
					src_chain_id
				);
			}
		}

		count
	}

	/// Migrate FinalizedTransfers from (ChainId, H256) to (ChainId, U256) key structure
	fn migrate_finalized_transfers<T: Config>(weight: &mut Weight) -> u32 {
		let mut count = 0u32;

		for ((src_chain_id, _src_tx_id), old_transfer) in storage_key_iter::<
			(ChainId, H256),
			OldTransferInfo<BalanceOf<T>, T::AccountId>,
			Twox64Concat,
		>(PALLET_NAME, b"FinalizedTransfers")
		.drain()
		{
			*weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));

			if let Some(sequence_id) =
				extract_sequence_id_from_socket_message(&old_transfer.socket_message)
			{
				let new_transfer = TransferInfo {
					amount: old_transfer.amount,
					sequence_id,
					src_tx_id: old_transfer.src_tx_id,
					src_chain_id: old_transfer.src_chain_id,
					dst_chain_id: old_transfer.dst_chain_id,
					asset_index_hash: old_transfer.asset_index_hash,
					option: old_transfer.option,
					status: old_transfer.status,
					socket_message: old_transfer.socket_message,
					on_flight_voters: old_transfer.on_flight_voters,
					finalization_voters: old_transfer.finalization_voters,
				};

				current_pallet::FinalizedTransfers::<T>::insert(
					src_chain_id,
					sequence_id,
					new_transfer,
				);
				*weight = weight.saturating_add(T::DbWeight::get().writes(1));
				count += 1;
			} else {
				log!(
					warn,
					"Failed to parse socket message for Finalized transfer src_chain_id={}, skipping",
					src_chain_id
				);
			}
		}

		count
	}

	/// Extract sequence_id from the socket message bytes.
	fn extract_sequence_id_from_socket_message(
		message: &bp_cccp::UnboundedBytes,
	) -> Option<sp_core::U256> {
		use bp_cccp::SocketMessage;

		let socket_msg = SocketMessage::try_from(message.clone()).ok()?;
		Some(socket_msg.req_id.sequence)
	}
}
