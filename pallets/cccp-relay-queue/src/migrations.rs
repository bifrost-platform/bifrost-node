use super::*;

pub mod init_v2 {
	use super::*;
	use core::marker::PhantomData;
	use frame_support::{
		storage_alias,
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

	/// Old storage types (key2: H256, value: OldTransferInfo)
	mod old {
		use super::*;

		#[storage_alias]
		pub type OnFlightTransfers<T: Config> = StorageDoubleMap<
			Pallet<T>,
			Twox64Concat,
			ChainId,
			Twox64Concat,
			H256,
			OldTransferInfo<BalanceOf<T>, <T as frame_system::Config>::AccountId>,
		>;

		#[storage_alias]
		pub type FinalizedTransfers<T: Config> = StorageDoubleMap<
			Pallet<T>,
			Twox64Concat,
			ChainId,
			Twox64Concat,
			H256,
			OldTransferInfo<BalanceOf<T>, <T as frame_system::Config>::AccountId>,
		>;
	}

	/// New storage types (key2: U256, value: TransferInfo)
	mod new {
		use super::*;

		#[storage_alias]
		pub type OnFlightTransfers<T: Config> = StorageDoubleMap<
			Pallet<T>,
			Twox64Concat,
			ChainId,
			Twox64Concat,
			sp_core::U256,
			TransferInfo<BalanceOf<T>, <T as frame_system::Config>::AccountId>,
		>;

		#[storage_alias]
		pub type FinalizedTransfers<T: Config> = StorageDoubleMap<
			Pallet<T>,
			Twox64Concat,
			ChainId,
			Twox64Concat,
			sp_core::U256,
			TransferInfo<BalanceOf<T>, <T as frame_system::Config>::AccountId>,
		>;
	}

	pub struct InitV2<T>(PhantomData<T>);

	impl<T: Config> OnRuntimeUpgrade for InitV2<T>
	where
		BalanceOf<T>: Into<sp_core::U256> + TryFrom<sp_core::U256>,
	{
		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();

			let current = Pallet::<T>::in_code_storage_version();
			let onchain = Pallet::<T>::on_chain_storage_version();

			if current == 2 && onchain == 1 {
				log!(info, "Starting cccp-relay-queue storage migration v1 → v2");

				// Migrate OnFlightTransfers: key2 changes from H256 (src_tx_id) to U256 (sequence_id)
				let on_flight_entries: sp_std::vec::Vec<_> =
					old::OnFlightTransfers::<T>::drain().collect();
				weight =
					weight.saturating_add(T::DbWeight::get().reads_writes(
						on_flight_entries.len() as u64,
						on_flight_entries.len() as u64,
					));

				let mut on_flight_count = 0u32;
				for (src_chain_id, _src_tx_id, old_transfer) in on_flight_entries {
					if let Some(sequence_id) =
						extract_sequence_id_from_socket_message(&old_transfer.socket_message)
					{
						let new_transfer = TransferInfo {
							amount: old_transfer.amount,
							sequence_id,
							src_chain_id: old_transfer.src_chain_id,
							dst_chain_id: old_transfer.dst_chain_id,
							asset_index_hash: old_transfer.asset_index_hash,
							option: old_transfer.option,
							status: old_transfer.status,
							socket_message: old_transfer.socket_message,
							on_flight_voters: old_transfer.on_flight_voters,
							finalization_voters: old_transfer.finalization_voters,
						};
						new::OnFlightTransfers::<T>::insert(
							src_chain_id,
							sequence_id,
							new_transfer,
						);
						on_flight_count += 1;
					} else {
						log!(
							warn,
							"Failed to parse socket message for OnFlight transfer src_chain_id={}, skipping",
							src_chain_id
						);
					}
				}
				weight = weight.saturating_add(T::DbWeight::get().writes(on_flight_count as u64));
				log!(info, "Migrated {} OnFlightTransfers entries", on_flight_count);

				// Migrate FinalizedTransfers: key2 changes from H256 (src_tx_id) to U256 (sequence_id)
				let finalized_entries: sp_std::vec::Vec<_> =
					old::FinalizedTransfers::<T>::drain().collect();
				weight =
					weight.saturating_add(T::DbWeight::get().reads_writes(
						finalized_entries.len() as u64,
						finalized_entries.len() as u64,
					));

				let mut finalized_count = 0u32;
				for (src_chain_id, _src_tx_id, old_transfer) in finalized_entries {
					if let Some(sequence_id) =
						extract_sequence_id_from_socket_message(&old_transfer.socket_message)
					{
						let new_transfer = TransferInfo {
							amount: old_transfer.amount,
							sequence_id,
							src_chain_id: old_transfer.src_chain_id,
							dst_chain_id: old_transfer.dst_chain_id,
							asset_index_hash: old_transfer.asset_index_hash,
							option: old_transfer.option,
							status: old_transfer.status,
							socket_message: old_transfer.socket_message,
							on_flight_voters: old_transfer.on_flight_voters,
							finalization_voters: old_transfer.finalization_voters,
						};
						new::FinalizedTransfers::<T>::insert(
							src_chain_id,
							sequence_id,
							new_transfer,
						);
						finalized_count += 1;
					} else {
						log!(
							warn,
							"Failed to parse socket message for Finalized transfer src_chain_id={}, skipping",
							src_chain_id
						);
					}
				}
				weight = weight.saturating_add(T::DbWeight::get().writes(finalized_count as u64));
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

	/// Extract sequence_id from the socket message bytes.
	fn extract_sequence_id_from_socket_message(
		message: &bp_cccp::UnboundedBytes,
	) -> Option<sp_core::U256> {
		use bp_cccp::SocketMessage;

		let socket_msg = SocketMessage::try_from(message.clone()).ok()?;
		Some(socket_msg.req_id.sequence)
	}
}
