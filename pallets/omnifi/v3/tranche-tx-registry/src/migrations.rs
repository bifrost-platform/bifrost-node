//! Storage migrations for `pallet-tranche-tx-registry-v3`.

use crate::{defaults, Config, DefaultFlows, Pallet, ProductFlows, ProductModel, Templates};
use frame_support::{
	migrations::VersionedMigration,
	traits::{Get, OnRuntimeUpgrade, UncheckedOnRuntimeUpgrade},
	weights::Weight,
};
use sp_std::{marker::PhantomData, vec::Vec};

/// Seeds the built-in default templates and default flow maps on a live chain
/// the pallet is newly added to (genesis does the same through
/// `GenesisConfig::seed_default_templates`). Products registered before this
/// upgrade get no assignment — governance runs `assign_default_flows` for the
/// ones that should move to v3.
pub mod v1 {
	use super::*;

	/// Runs on every runtime upgrade and seeds only while no template is
	/// registered. Templates can't be removed, so this seeds exactly once and
	/// never brings back a default flow governance later removed.
	///
	/// Not a `VersionedMigration<0, 1>`: FRAME's `before_all_runtime_migrations`
	/// sets a newly added pallet's on-chain storage version to its in-code
	/// version before any migration runs, so a 0 → 1 migration never fires for
	/// a pallet added by an upgrade.
	pub struct SeedDefaultTemplates<T>(PhantomData<T>);

	impl<T: Config> OnRuntimeUpgrade for SeedDefaultTemplates<T> {
		fn on_runtime_upgrade() -> Weight {
			let read = T::DbWeight::get().reads(1);
			if Templates::<T>::iter_keys().next().is_some() {
				return read;
			}
			let (weight, registered) = defaults::seed::<T>();
			log::info!(
				target: "runtime::tranche-tx-registry-v3",
				"registered {} of {} default templates",
				registered,
				defaults::templates().len()
			);
			read.saturating_add(weight)
		}
	}
}

/// v1 → v2: `mc-deposit` / `mc-redeem` v2 (an optional `RequestClaimed` slot for
/// the claim tx, from the vault spoke's `RequestClaimed` event, 2026-10-19).
///
/// - registers every built-in template not registered yet (the two v2s);
/// - moves the Multichain default deposit / redeem flow from v1 to v2 — only
///   where it is still the built-in v1, so a governance override stays;
/// - moves every product flow still assigned the built-in v1 to v2. Instances
///   already open keep the template they opened with (v1) — only requests
///   recorded after the upgrade get the claim slot.
///
/// Gated on the storage version (1 → 2). A chain the pallet is newly added to
/// is set straight to 2 before migrations run, so this is skipped there and
/// `v1::SeedDefaultTemplates` seeds the v2 defaults directly.
pub mod v2 {
	use super::*;

	/// Built-in v1 → v2 swaps, per flow.
	const SWAPS: [(crate::FlowId, crate::TemplateId, crate::TemplateId); 2] = [
		(defaults::FLOW_DEPOSIT, defaults::MC_DEPOSIT_V1, defaults::MC_DEPOSIT_V2),
		(defaults::FLOW_REDEEM, defaults::MC_REDEEM_V1, defaults::MC_REDEEM_V2),
	];

	fn swap(flow_id: &crate::FlowId, template: &crate::TemplateId) -> Option<crate::TemplateId> {
		SWAPS
			.iter()
			.find(|(flow, old, _)| flow == flow_id && old == template)
			.map(|(_, _, new)| *new)
	}

	pub struct UncheckedAdoptClaimSlots<T>(PhantomData<T>);

	impl<T: Config> UncheckedOnRuntimeUpgrade for UncheckedAdoptClaimSlots<T> {
		fn on_runtime_upgrade() -> Weight {
			let db = T::DbWeight::get();
			let (mut weight, registered) = defaults::register_templates::<T>();

			weight = weight.saturating_add(db.reads_writes(1, 1));
			DefaultFlows::<T>::mutate(ProductModel::Multichain, |flows| {
				for (flow_id, template) in flows.iter_mut() {
					if let Some(new) = swap(flow_id, template) {
						*template = new;
					}
				}
			});

			let mut scanned = 0u64;
			let moved: Vec<_> = ProductFlows::<T>::iter()
				.filter_map(|(product_id, flow_id, template)| {
					scanned = scanned.saturating_add(1);
					swap(&flow_id, &template).map(|new| (product_id, flow_id, new))
				})
				.collect();
			weight = weight.saturating_add(db.reads(scanned));
			for (product_id, flow_id, template) in moved.iter() {
				ProductFlows::<T>::insert(product_id, flow_id, template);
			}
			weight = weight.saturating_add(db.writes(moved.len() as u64));

			log::info!(
				target: "runtime::tranche-tx-registry-v3",
				"v2: registered {} template(s), moved {} of {} product flow(s) to v2",
				registered,
				moved.len(),
				scanned
			);
			weight
		}
	}

	pub type AdoptClaimSlots<T> = VersionedMigration<
		1,
		2,
		UncheckedAdoptClaimSlots<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;
}
