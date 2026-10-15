//! Storage migrations for `pallet-tranche-tx-registry-v3`.

use crate::{defaults, Config, Templates};
use frame_support::{
	traits::{Get, OnRuntimeUpgrade},
	weights::Weight,
};
use sp_std::marker::PhantomData;

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
