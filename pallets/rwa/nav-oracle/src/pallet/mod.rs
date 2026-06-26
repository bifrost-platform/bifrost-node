use crate::{EarningsEntry, EpochId, PoolId};
use pallet_pools::{PermissionInspect, PoolInspect, PoolNAV};
use sp_core::U256;
use sp_runtime::DispatchError;

use frame_support::{pallet_prelude::*, traits::StorageVersion};
use frame_system::pallet_prelude::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
		/// Pool inspection — implemented by pallet-pools.
		/// Used to verify pool existence and the current epoch ID before accepting submissions.
		type Pools: PoolInspect;
		/// Permission inspection — implemented by pallet-permissions.
		/// Used to verify that the caller holds the `OracleFeeder` role for the pool.
		type Permissions: PermissionInspect<Self::AccountId>;
	}

	// -----------------------------------------------------------------------
	// Errors
	// -----------------------------------------------------------------------

	#[pallet::error]
	pub enum Error<T> {
		/// No pool exists with this ID.
		PoolNotFound,
		/// Caller does not hold the OracleFeeder role for this pool.
		Unauthorized,
		/// Submitted epoch_id does not match the pool's current epoch.
		/// Submissions are only accepted for the currently active epoch.
		InvalidEpochId,
		/// Submitted cumulative_earnings is lower than the previous epoch's finalized value.
		/// Across epochs, cumulative_earnings must be monotonically non-decreasing.
		EarningsDecreased,
	}

	// -----------------------------------------------------------------------
	// Events
	// -----------------------------------------------------------------------

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An oracle feeder submitted a cumulative earnings snapshot for a pool epoch.
		EarningsSubmitted {
			pool_id: PoolId,
			epoch_id: EpochId,
			feeder: T::AccountId,
			cumulative_earnings: U256,
		},
	}

	// -----------------------------------------------------------------------
	// Storage
	// -----------------------------------------------------------------------

	/// Cumulative earnings snapshots keyed by (pool_id, epoch_id).
	///
	/// Each epoch's entry records the feeder's latest submission for that epoch.
	/// Within an epoch the feeder may update freely (corrections allowed).
	/// The value written at epoch close becomes the immutable baseline for the
	/// next epoch's monotonicity check.
	///
	/// Only the current and previous epoch entries are kept per pool; entries
	/// older than `current_epoch − 1` are pruned on each new submission.
	#[pallet::storage]
	pub type PoolEarnings<T: Config> =
		StorageDoubleMap<_, Blake2_128Concat, PoolId, Blake2_128Concat, EpochId, EarningsEntry>;

	// -----------------------------------------------------------------------
	// Extrinsics
	// -----------------------------------------------------------------------

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Submit a cumulative earnings snapshot for the current pool epoch.
		///
		/// The caller must hold the `OracleFeeder` role for `pool_id`
		/// (granted by the pool admin via `pallet-permissions`).
		///
		/// `epoch_id` must match the pool's current epoch — submissions for past
		/// or future epochs are rejected with `InvalidEpochId`.
		///
		/// Within an epoch the feeder may submit multiple times; each submission
		/// overwrites the previous value. Intra-epoch corrections (including
		/// downward adjustments) are permitted.
		///
		/// Across epoch boundaries, `cumulative_earnings` must be monotonically
		/// non-decreasing: the first submission of epoch N must be >= the last
		/// submitted value of epoch N−1. This prevents a compromised feeder from
		/// retroactively tanking oracle NAV after an epoch has already settled.
		///
		/// `pallet-pools` computes the final oracle NAV as:
		///   `oracle_nav = total_borrowed + (cumulative_earnings − repaid_earnings)`
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::from_parts(5_000, 0))]
		pub fn submit_earnings(
			origin: OriginFor<T>,
			pool_id: PoolId,
			epoch_id: EpochId,
			cumulative_earnings: U256,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			ensure!(T::Pools::pool_exists(pool_id), Error::<T>::PoolNotFound);
			ensure!(T::Permissions::is_oracle_feeder(pool_id, &who), Error::<T>::Unauthorized);

			let current_epoch = T::Pools::current_epoch(pool_id).ok_or(Error::<T>::PoolNotFound)?;
			ensure!(epoch_id == current_epoch, Error::<T>::InvalidEpochId);

			// Cross-epoch monotonicity: new epoch's first submission must be >=
			// the previous epoch's finalized (last-submitted) value.
			if let Some(prev_epoch_id) = epoch_id.checked_sub(1) {
				if let Some(prev) = PoolEarnings::<T>::get(pool_id, prev_epoch_id) {
					ensure!(
						cumulative_earnings >= prev.cumulative_earnings,
						Error::<T>::EarningsDecreased
					);
				}
			}

			let block: u32 = frame_system::Pallet::<T>::block_number().try_into().unwrap_or(0);

			PoolEarnings::<T>::insert(
				pool_id,
				epoch_id,
				EarningsEntry { cumulative_earnings, updated_block: block },
			);

			// Prune entries older than epoch_id − 1 to bound storage growth.
			if let Some(prev_epoch_id) = epoch_id.checked_sub(1) {
				if let Some(old_epoch_id) = prev_epoch_id.checked_sub(1) {
					PoolEarnings::<T>::remove(pool_id, old_epoch_id);
				}
			}

			Self::deposit_event(Event::EarningsSubmitted {
				pool_id,
				epoch_id,
				feeder: who,
				cumulative_earnings,
			});

			Ok(())
		}
	}

	// -----------------------------------------------------------------------
	// PoolNAV implementation
	// -----------------------------------------------------------------------

	impl<T: Config> PoolNAV<PoolId, U256> for Pallet<T> {
		/// Returns `(cumulative_earnings, updated_block)` for the pool's current epoch.
		///
		/// Returns `None` if no feeder has submitted for the current epoch yet.
		/// pallet-pools treats `None` as `cumulative_earnings = 0`, so
		/// `oracle_nav` falls back to `total_borrowed` alone until the first submission.
		fn nav(pool_id: PoolId) -> Option<(U256, u32)> {
			let current_epoch = T::Pools::current_epoch(pool_id)?;
			PoolEarnings::<T>::get(pool_id, current_epoch)
				.map(|e| (e.cumulative_earnings, e.updated_block))
		}

		/// Returns the current epoch's `cumulative_earnings`.
		///
		/// Returns `Ok(U256::zero())` if no submission exists for the current epoch,
		/// matching the `unwrap_or_default()` semantics pallet-pools applies to `nav()`.
		fn update_nav(pool_id: PoolId) -> Result<U256, DispatchError> {
			Ok(Self::nav(pool_id).map(|(v, _)| v).unwrap_or_default())
		}
	}
}
