use super::*;
use frame_support::{pallet_prelude::*, storage_alias, traits::OnRuntimeUpgrade};

#[storage_alias]
pub type StorageVersion<T: Config> = StorageValue<Pallet<T>, Releases, ValueQuery>;

/// Used to match mainnet pallet version
pub mod v3_update {
	use super::*;

	pub struct MigrateToV3Update<T>(PhantomData<T>);

	impl<T: Config> OnRuntimeUpgrade for MigrateToV3Update<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();

			let current = Pallet::<T>::in_code_storage_version();
			let onchain = Pallet::<T>::on_chain_storage_version();

			if current == 3 && onchain == 0 {
				current.put::<Pallet<T>>();
				log!(info, "bfc-utility storage migration passes v3::update(2) ✅");
				weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));
			} else {
				log!(warn, "Skipping bfc-utility storage migration v3::update(2) 💤");
				weight = weight.saturating_add(T::DbWeight::get().reads(1));
			}

			weight
		}
	}
}

pub mod v3 {
	use super::*;
	#[cfg(feature = "try-runtime")]
	use sp_runtime::TryRuntimeError;

	pub struct MigrateToV3<T>(PhantomData<T>);

	impl<T: Config> OnRuntimeUpgrade for MigrateToV3<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();

			let current = Pallet::<T>::in_code_storage_version();
			// (previous) let onchain = StorageVersion::<T>::get();
			let onchain = Pallet::<T>::on_chain_storage_version();

			// (previous: if current == 3 && onchain == Releases::V2_0_0)
			if current == 3 && onchain == 2 {
				// migrate to new standard storage version
				StorageVersion::<T>::kill();
				current.put::<Pallet<T>>();

				log!(info, "bfc-utility storage migration passes v3 update ✅");
				weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 2));
			} else {
				log!(warn, "Skipping bfc-utility storage migration v3 ✅");
				weight = weight.saturating_add(T::DbWeight::get().reads(1));
			}

			weight
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
			ensure!(
				StorageVersion::<T>::get() == Releases::V2_0_0,
				"Required v2_0_0 before upgrading to v3"
			);

			Ok(Default::default())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(_state: Vec<u8>) -> Result<(), TryRuntimeError> {
			ensure!(Pallet::<T>::on_chain_storage_version() == 3, "v3 not applied");

			ensure!(!StorageVersion::<T>::exists(), "Storage version not migrated correctly");

			Ok(())
		}
	}
}

pub mod v2 {
	use super::*;
	use frame_support::traits::Get;

	#[derive(Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
	pub struct OldProposal<AccountId> {
		pub who: AccountId,
		pub proposal_hex: String,
		pub proposal_index: PropIndex,
	}

	pub fn migrate<T: Config>() -> Weight {
		let mut new_proposals: Vec<Proposal> = vec![];
		AcceptedProposals::<T>::translate(|v: Option<Vec<OldProposal<T::AccountId>>>| {
			if let Some(proposals) = v.clone() {
				for proposal in proposals {
					new_proposals.push(Proposal {
						proposal_hex: proposal.proposal_hex,
						proposal_index: proposal.proposal_index,
					});
				}
				Some(new_proposals)
			} else {
				// For Safety. Should not happen
				None
			}
		})
		.expect("BFC_UTILITY: Error while migrating");

		StorageVersion::<T>::put(Releases::V2_0_0);
		T::BlockWeights::get().max_block
	}
}
