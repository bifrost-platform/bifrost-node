use crate::{Config, OrderType, Pallet, RequestEntry, RequestId, TxRecord};
use pallet_tranche_system::{ProductId, VaultId};

use frame_support::{
	migrations::VersionedMigration, pallet_prelude::*, storage_alias,
	traits::UncheckedOnRuntimeUpgrade, weights::Weight, Blake2_128Concat,
};
use frame_system::pallet_prelude::BlockNumberFor;
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::{H160, U256};
use sp_runtime::RuntimeDebug;
use sp_std::marker::PhantomData;

pub(crate) const LOG_TARGET: &str = "runtime::tranche-tx-registry";

// syntactic sugar for logging.
macro_rules! log {
	($level:tt, $patter:expr $(, $values:expr)* $(,)?) => {
		log::$level!(
			target: LOG_TARGET,
			$patter $(, $values)*
		)
	};
}

/// v0 -> v1: `RequestEntry` gained two fields, `settlement_id: Option<SettlementId>` and
/// `approved_tx: Option<TxRecord<BlockNumber>>`, written by the new
/// `RequestStep::SettlementApproved` step — this pallet's own copy of the
/// request<->settlement linkage that used to be queried cross-pallet from
/// pallet-tranche-investments (via the now-removed `RequestSettlementInspect` trait; see
/// `SettlementRequests`' doc comment). Every pre-existing entry backfills both as `None` —
/// genuinely correct, not merely a placeholder: no entry written before this upgrade could
/// ever have recorded a `SettlementApproved` step (it didn't exist yet), so `None` is the
/// exact right value here, unlike the `recorded_at`/`timestamp` backfills elsewhere in this
/// pallet family, which stamp an approximate "as of this upgrade" value in place of a
/// genuinely unrecoverable original.
pub mod v1 {
	use super::*;

	/// `RequestEntry` as it existed under `STORAGE_VERSION::new(0)`, before
	/// `settlement_id`/`approved_tx` existed.
	#[derive(
		Clone,
		Encode,
		Decode,
		DecodeWithMemTracking,
		PartialEq,
		Eq,
		RuntimeDebug,
		TypeInfo,
		MaxEncodedLen,
	)]
	pub struct RequestEntryV0<BlockNumber> {
		pub product_id: ProductId,
		pub vault: VaultId,
		pub investor: H160,
		pub amount: U256,
		pub order_type: OrderType,
		pub request_tx: Option<TxRecord<BlockNumber>>,
		pub bridge_tx: Option<TxRecord<BlockNumber>>,
		pub queued_tx: Option<TxRecord<BlockNumber>>,
	}

	#[storage_alias]
	type RequestEntries<T: Config> = StorageDoubleMap<
		Pallet<T>,
		Blake2_128Concat,
		ProductId,
		Blake2_128Concat,
		RequestId,
		RequestEntryV0<BlockNumberFor<T>>,
	>;

	pub struct MigrateV0ToV1<T>(PhantomData<T>);

	impl<T: Config> UncheckedOnRuntimeUpgrade for MigrateV0ToV1<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();

			let entries = RequestEntries::<T>::drain().collect::<sp_std::vec::Vec<_>>();
			weight = weight.saturating_add(
				T::DbWeight::get().reads_writes(entries.len() as u64, entries.len() as u64),
			);
			let entries_count = entries.len();
			for (product_id, request_id, old) in entries {
				crate::RequestEntries::<T>::insert(
					product_id,
					request_id,
					RequestEntry {
						product_id: old.product_id,
						vault: old.vault,
						investor: old.investor,
						amount: old.amount,
						order_type: old.order_type,
						request_tx: old.request_tx,
						bridge_tx: old.bridge_tx,
						queued_tx: old.queued_tx,
						settlement_id: None,
						approved_tx: None,
					},
				);
			}
			weight = weight.saturating_add(T::DbWeight::get().writes(entries_count as u64));

			log!(
				info,
				"tranche-tx-registry v0->v1: backfilled settlement_id/approved_tx as None for {} RequestEntries entries ✅",
				entries_count,
			);

			weight
		}
	}

	/// Gated `on_chain == 0 && in_code == 1`, and bumps the on-chain version itself — wire
	/// this (not `MigrateV0ToV1` directly) into `Pallet::on_runtime_upgrade`.
	pub type MigrateToV1<T> = VersionedMigration<
		0,
		1,
		MigrateV0ToV1<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;
}
