//! Storage migrations for `pallet-tranche-tx-registry-v3`.

use crate::{defaults, Config, Pallet};
use frame_support::{
	migrations::VersionedMigration, traits::UncheckedOnRuntimeUpgrade, weights::Weight,
};
use sp_std::marker::PhantomData;

/// v0 -> v1: seeds the built-in default templates and default flow maps on a
/// live chain the pallet is newly added to (genesis does the same through
/// `GenesisConfig::seed_default_templates`). Products registered before this
/// upgrade get no assignment — governance runs `assign_default_flows` for the
/// ones that should move to v3.
pub mod v1 {
	use super::*;

	pub struct SeedDefaultTemplatesUnchecked<T>(PhantomData<T>);

	impl<T: Config> UncheckedOnRuntimeUpgrade for SeedDefaultTemplatesUnchecked<T> {
		fn on_runtime_upgrade() -> Weight {
			let (weight, registered) = defaults::seed::<T>();
			log::info!(
				target: "runtime::tranche-tx-registry-v3",
				"registered {} of {} default templates",
				registered,
				defaults::templates().len()
			);
			weight
		}
	}

	pub type SeedDefaultTemplates<T> = VersionedMigration<
		0,
		1,
		SeedDefaultTemplatesUnchecked<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;
}
