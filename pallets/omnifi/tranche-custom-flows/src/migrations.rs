//! Storage migrations for `pallet-tranche-custom-flows`.
//!
//! Only the latest migration is kept: every live chain running this pallet is
//! already at the version the previous one targeted (v1).

use crate::{
	Attempt, Config, FlowId, FlowInstance, HistoryPage, InstanceKey, Lane, Pallet, ProductId,
	SlotId, SlotRecord, MAX_ATTEMPTS, MAX_ATTEMPT_METADATA, MAX_SLOT_METADATA,
};

use bp_tranche::legacy::EvmTxRecord;
use frame_support::{
	migrations::VersionedMigration, pallet_prelude::*, storage_alias,
	traits::UncheckedOnRuntimeUpgrade, weights::Weight, Blake2_128Concat,
};
use sp_core::{ConstU32, H160};
use sp_runtime::BoundedVec;
use sp_std::{marker::PhantomData, vec::Vec};

pub(crate) const LOG_TARGET: &str = "runtime::tranche-custom-flows";

macro_rules! log {
	($level:tt, $patter:expr $(, $values:expr)* $(,)?) => {
		log::$level!(target: LOG_TARGET, $patter $(, $values)*)
	};
}

/// v1 -> v2: non-EVM support. Investors widen from `H160` to a 32-byte
/// `ChainAddress` (left-padded) and recorded tx hashes from `H256` to a
/// variable-length `TxHash`. Rewrites `FlowInstances`/`FlowSlots` values and
/// re-keys the investor-keyed `InvestorActiveFlows`/`InvestorFlowHistoryLen`/
/// `InvestorFlowHistoryPage`. No entry is added or dropped.
pub mod v2 {
	use super::*;

	pub mod old {
		use super::*;

		#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug)]
		pub struct Attempt<BlockNumber> {
			pub tx: EvmTxRecord<BlockNumber>,
			pub success: bool,
			pub metadata: BoundedVec<u8, ConstU32<MAX_ATTEMPT_METADATA>>,
		}

		#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug)]
		pub struct SlotRecord<BlockNumber> {
			pub satisfied: bool,
			pub metadata: BoundedVec<u8, ConstU32<MAX_SLOT_METADATA>>,
			pub attempts: BoundedVec<Attempt<BlockNumber>, ConstU32<MAX_ATTEMPTS>>,
		}

		#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug)]
		pub struct FlowInstance<BlockNumber> {
			pub opened_at: BlockNumber,
			pub investor: Option<H160>,
			pub pending_lanes: u16,
			pub closed: bool,
		}
	}

	type BlockNumberOf<T> = frame_system::pallet_prelude::BlockNumberFor<T>;

	#[storage_alias]
	pub type FlowInstances<T: Config> = StorageNMap<
		Pallet<T>,
		(
			NMapKey<Blake2_128Concat, ProductId>,
			NMapKey<Blake2_128Concat, FlowId>,
			NMapKey<Blake2_128Concat, InstanceKey>,
		),
		old::FlowInstance<BlockNumberOf<T>>,
	>;

	#[storage_alias]
	pub type FlowSlots<T: Config> = StorageNMap<
		Pallet<T>,
		(
			NMapKey<Blake2_128Concat, ProductId>,
			NMapKey<Blake2_128Concat, FlowId>,
			NMapKey<Blake2_128Concat, InstanceKey>,
			NMapKey<Blake2_128Concat, Lane>,
			NMapKey<Blake2_128Concat, SlotId>,
		),
		old::SlotRecord<BlockNumberOf<T>>,
	>;

	#[storage_alias]
	pub type InvestorActiveFlows<T: Config> =
		StorageMap<Pallet<T>, Blake2_128Concat, H160, Vec<(ProductId, FlowId, InstanceKey)>>;

	#[storage_alias]
	pub type InvestorFlowHistoryLen<T: Config> = StorageNMap<
		Pallet<T>,
		(
			NMapKey<Blake2_128Concat, H160>,
			NMapKey<Blake2_128Concat, ProductId>,
			NMapKey<Blake2_128Concat, FlowId>,
		),
		u32,
	>;

	#[storage_alias]
	pub type InvestorFlowHistoryPage<T: Config> = StorageNMap<
		Pallet<T>,
		(
			NMapKey<Blake2_128Concat, H160>,
			NMapKey<Blake2_128Concat, ProductId>,
			NMapKey<Blake2_128Concat, FlowId>,
			NMapKey<Blake2_128Concat, u32>,
		),
		HistoryPage<InstanceKey>,
	>;

	fn slot<BN>(old: old::SlotRecord<BN>) -> SlotRecord<BN> {
		SlotRecord {
			satisfied: old.satisfied,
			metadata: old.metadata,
			// Same bound on both sides (`MAX_ATTEMPTS`), so this never truncates.
			attempts: BoundedVec::truncate_from(
				old.attempts
					.into_inner()
					.into_iter()
					.map(|a| Attempt { tx: a.tx.into(), success: a.success, metadata: a.metadata })
					.collect(),
			),
		}
	}

	pub struct MigrateV1ToV2<T>(PhantomData<T>);

	impl<T: Config> UncheckedOnRuntimeUpgrade for MigrateV1ToV2<T> {
		fn on_runtime_upgrade() -> Weight {
			// Value-only rewrites: keys are unchanged, so each `insert` overwrites
			// the entry it was read from. Collected first so no entry is read after
			// being rewritten in the new shape.
			let old_instances = FlowInstances::<T>::iter().collect::<Vec<_>>();
			let instances = old_instances.len() as u64;
			for ((product_id, flow_id, key), inst) in old_instances {
				crate::FlowInstances::<T>::insert(
					(product_id, flow_id, key),
					FlowInstance {
						opened_at: inst.opened_at,
						investor: inst.investor.map(Into::into),
						pending_lanes: inst.pending_lanes,
						closed: inst.closed,
					},
				);
			}

			let old_slots = FlowSlots::<T>::iter().collect::<Vec<_>>();
			let slots = old_slots.len() as u64;
			for (key, record) in old_slots {
				crate::FlowSlots::<T>::insert(key, slot(record));
			}

			// Investor-keyed maps: keys change, so collect, clear, re-insert.
			let active = InvestorActiveFlows::<T>::drain().collect::<Vec<_>>();
			let history_len = InvestorFlowHistoryLen::<T>::drain().collect::<Vec<_>>();
			let history_pages = InvestorFlowHistoryPage::<T>::drain().collect::<Vec<_>>();
			let rekeyed = (active.len() + history_len.len() + history_pages.len()) as u64;

			for (investor, flows) in active {
				let investor: crate::ChainAddress = investor.into();
				crate::InvestorActiveFlows::<T>::insert(investor, flows);
			}
			for ((investor, product_id, flow_id), len) in history_len {
				let investor: crate::ChainAddress = investor.into();
				crate::InvestorFlowHistoryLen::<T>::insert((investor, product_id, flow_id), len);
			}
			for ((investor, product_id, flow_id, page), entries) in history_pages {
				let investor: crate::ChainAddress = investor.into();
				crate::InvestorFlowHistoryPage::<T>::insert(
					(investor, product_id, flow_id, page),
					entries,
				);
			}

			log!(
				info,
				"tranche-custom-flows v2: migrated {} instances, {} slots, re-keyed {} investor entries",
				instances,
				slots,
				rekeyed
			);

			// Value rewrites: 1 read + 1 write each; re-keys: 1 read + 2 writes each.
			T::DbWeight::get().reads_writes(
				instances + slots + rekeyed,
				instances + slots + rekeyed.saturating_mul(2),
			)
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			let counts: (u32, u32, u32, u32, u32) = (
				FlowInstances::<T>::iter_keys().count() as u32,
				FlowSlots::<T>::iter_keys().count() as u32,
				InvestorActiveFlows::<T>::iter_keys().count() as u32,
				InvestorFlowHistoryLen::<T>::iter_keys().count() as u32,
				InvestorFlowHistoryPage::<T>::iter_keys().count() as u32,
			);
			Ok(counts.encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			let (instances, slots, active, len, pages): (u32, u32, u32, u32, u32) =
				Decode::decode(&mut &state[..]).map_err(|_| "v2: bad pre_upgrade state")?;
			ensure!(crate::FlowInstances::<T>::iter().count() as u32 == instances, "v2: instances");
			ensure!(crate::FlowSlots::<T>::iter().count() as u32 == slots, "v2: slots");
			ensure!(crate::InvestorActiveFlows::<T>::iter().count() as u32 == active, "v2: active");
			ensure!(crate::InvestorFlowHistoryLen::<T>::iter().count() as u32 == len, "v2: len");
			ensure!(
				crate::InvestorFlowHistoryPage::<T>::iter().count() as u32 == pages,
				"v2: pages"
			);
			Ok(())
		}
	}

	pub type MigrateToV2<T> = VersionedMigration<
		1,
		2,
		MigrateV1ToV2<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;
}
