//! Storage migrations for `pallet-tranche-custom-flows`.
//!
//! The pallet shipped to a live chain with no `#[pallet::storage_version]`, so
//! its on-chain version is the implicit `0`. `v1` is the first migration: it
//! introduces `STORAGE_VERSION = 1` and converts the old unbounded
//! `InvestorFlowHistory` `Vec` into the paged `InvestorFlowHistoryLen` +
//! `InvestorFlowHistoryPage` storages (see `bp_tranche::history`).

use crate::{
	history::HISTORY_PAGE_SIZE, Config, FlowId, HistoryPage, InstanceKey, Pallet, ProductId,
};

use frame_support::{
	migrations::VersionedMigration, pallet_prelude::*, storage_alias,
	traits::UncheckedOnRuntimeUpgrade, weights::Weight, Blake2_128Concat,
};
use sp_core::H160;
use sp_std::{collections::btree_map::BTreeMap, marker::PhantomData, vec::Vec};

pub(crate) const LOG_TARGET: &str = "runtime::tranche-custom-flows";

macro_rules! log {
	($level:tt, $patter:expr $(, $values:expr)* $(,)?) => {
		log::$level!(target: LOG_TARGET, $patter $(, $values)*)
	};
}

/// v0 -> v1: split the single unbounded per-`(investor, product)`
/// `InvestorFlowHistory` `Vec<(FlowId, InstanceKey)>` into the paged
/// `InvestorFlowHistoryLen` + `InvestorFlowHistoryPage` storages, re-keyed by
/// `(investor, product, flow)` — one cleanly-paginated list per flow, matching
/// how `get_investor_flow_history` reads it. Entry order within each flow is
/// preserved (oldest first).
pub mod v1 {
	use super::*;

	#[storage_alias]
	type InvestorFlowHistory<T: Config> = StorageDoubleMap<
		Pallet<T>,
		Blake2_128Concat,
		H160,
		Blake2_128Concat,
		ProductId,
		Vec<(FlowId, InstanceKey)>,
		ValueQuery,
	>;

	pub struct MigrateV0ToV1<T>(PhantomData<T>);

	impl<T: Config> UncheckedOnRuntimeUpgrade for MigrateV0ToV1<T> {
		fn on_runtime_upgrade() -> Weight {
			let old_lists = InvestorFlowHistory::<T>::drain().collect::<Vec<_>>();

			let mut old_list_count = 0u64;
			let mut new_list_count = 0u64;
			let mut page_writes = 0u64;

			for (investor, product_id, entries) in old_lists {
				old_list_count = old_list_count.saturating_add(1);

				// Group by flow_id, preserving append order within each group.
				let mut by_flow: BTreeMap<FlowId, Vec<InstanceKey>> = BTreeMap::new();
				for (flow_id, instance_key) in entries {
					by_flow.entry(flow_id).or_default().push(instance_key);
				}

				for (flow_id, keys) in by_flow {
					let len = keys.len() as u32;
					for (page_idx, chunk) in keys.chunks(HISTORY_PAGE_SIZE as usize).enumerate() {
						// `chunk.len() <= HISTORY_PAGE_SIZE` — never truncates.
						crate::InvestorFlowHistoryPage::<T>::insert(
							(investor, product_id, flow_id, page_idx as u32),
							HistoryPage::truncate_from(chunk.to_vec()),
						);
						page_writes = page_writes.saturating_add(1);
					}
					crate::InvestorFlowHistoryLen::<T>::insert(
						(investor, product_id, flow_id),
						len,
					);
					new_list_count = new_list_count.saturating_add(1);
				}
			}

			log!(
				info,
				"tranche-custom-flows v0->v1: paged {} InvestorFlowHistory lists into {} per-flow lists ({} page writes) ✅",
				old_list_count,
				new_list_count,
				page_writes,
			);

			// reads: one per drained old key. writes: drained key removed + each
			// page + each len header.
			T::DbWeight::get().reads_writes(
				old_list_count,
				old_list_count.saturating_add(page_writes).saturating_add(new_list_count),
			)
		}
	}

	/// Gated `on_chain == 0 && in_code == 1`; bumps the on-chain version. Wire
	/// this (not `MigrateV0ToV1` directly) into `Pallet::on_runtime_upgrade`.
	pub type MigrateToV1<T> = VersionedMigration<
		0,
		1,
		MigrateV0ToV1<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;
}
