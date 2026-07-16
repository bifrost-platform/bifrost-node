use crate::{EpochId, PnlEntry, PoolId, WeightInfo};

use pallet_rwa_pools::{PermissionInspect, PoolInspect, PoolNAV};
use sp_core::U256;
use sp_runtime::DispatchError;

use frame_support::{pallet_prelude::*, traits::StorageVersion};
use frame_system::pallet_prelude::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

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
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
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
	}

	// -----------------------------------------------------------------------
	// Events
	// -----------------------------------------------------------------------

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An oracle feeder submitted a cumulative P&L snapshot for a pool epoch.
		PnlSubmitted {
			pool_id: PoolId,
			epoch_id: EpochId,
			feeder: T::AccountId,
			cumulative_pnl: U256,
			is_loss: bool,
		},
	}

	// -----------------------------------------------------------------------
	// Storage
	// -----------------------------------------------------------------------

	/// Net P&L snapshots keyed by (pool_id, epoch_id).
	///
	/// Each epoch's entry records the feeder's latest submission for that epoch — freely
	/// overwritable while the epoch is current, and immutable once the epoch advances.
	///
	/// Only the current and previous epoch entries are kept per pool; entries
	/// older than `current_epoch − 1` are pruned on each new submission.
	#[pallet::storage]
	pub type PoolEarnings<T: Config> =
		StorageDoubleMap<_, Blake2_128Concat, PoolId, Blake2_128Concat, EpochId, PnlEntry>;

	// -----------------------------------------------------------------------
	// Extrinsics
	// -----------------------------------------------------------------------

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Submit a cumulative net P&L (profit and loss) snapshot for the current pool epoch.
		///
		/// The caller must hold the `OracleFeeder` role for `pool_id`
		/// (granted by the pool admin via `pallet-permissions`).
		///
		/// `epoch_id` must match the pool's current epoch — submissions for past
		/// or future epochs are rejected with `InvalidEpochId`.
		///
		/// `cumulative_pnl` is a magnitude; `is_loss` signs it — `false` for a net gain,
		/// `true` for a net loss (e.g. a borrower default or penalty fees exceeding income).
		/// A magnitude of zero is always normalized to `is_loss = false`.
		///
		/// Within an epoch the feeder may submit multiple times; each submission
		/// overwrites the previous value — corrections (including downward adjustments,
		/// or flipping sign) are permitted with no additional constraint, both within an
		/// epoch and across epoch boundaries. A pool's true net position is not required
		/// to move in any particular direction between epochs — a prior gain can be
		/// followed by a real loss, since a loan can go from performing to defaulted.
		/// `OracleFeeder` is a permissioned role (granted by the pool admin); nothing beyond
		/// that role check gates what value is reported. Past epochs are still immutable
		/// regardless — submissions are only ever accepted for the current epoch
		/// (`InvalidEpochId` otherwise), so an already-settled epoch's locked `epoch_price`
		/// can never be rewritten by a later submission.
		///
		/// `pallet-pools` computes the final oracle NAV as:
		///   `oracle_nav = total_borrowed ± cumulative_pnl − repaid_earnings`, floored at 0
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn submit_pnl(
			origin: OriginFor<T>,
			pool_id: PoolId,
			epoch_id: EpochId,
			cumulative_pnl: U256,
			is_loss: bool,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			ensure!(T::Pools::pool_exists(pool_id), Error::<T>::PoolNotFound);
			ensure!(T::Permissions::is_oracle_feeder(pool_id, &who), Error::<T>::Unauthorized);

			let current_epoch = T::Pools::current_epoch(pool_id).ok_or(Error::<T>::PoolNotFound)?;
			ensure!(epoch_id == current_epoch, Error::<T>::InvalidEpochId);

			// Zero is never signed — canonicalize so storage never sees a spurious
			// `(0, true)` vs `(0, false)` distinction.
			let is_loss = is_loss && !cumulative_pnl.is_zero();

			let block: u32 = frame_system::Pallet::<T>::block_number().try_into().unwrap_or(0);

			PoolEarnings::<T>::insert(
				pool_id,
				epoch_id,
				PnlEntry { cumulative_pnl, is_loss, updated_block: block },
			);

			// Prune entries older than epoch_id − 1 to bound storage growth.
			if let Some(prev_epoch_id) = epoch_id.checked_sub(1) {
				if let Some(old_epoch_id) = prev_epoch_id.checked_sub(1) {
					PoolEarnings::<T>::remove(pool_id, old_epoch_id);
				}
			}

			Self::deposit_event(Event::PnlSubmitted {
				pool_id,
				epoch_id,
				feeder: who,
				cumulative_pnl,
				is_loss,
			});

			Ok(())
		}
	}

	// -----------------------------------------------------------------------
	// PoolNAV implementation
	// -----------------------------------------------------------------------

	impl<T: Config> PoolNAV<PoolId, U256> for Pallet<T> {
		/// Returns `(cumulative_pnl, is_loss, updated_block)` for the pool's current epoch.
		///
		/// Returns `None` if no feeder has submitted for the current epoch yet.
		/// pallet-pools treats `None` as `cumulative_pnl = 0`, so
		/// `oracle_nav` falls back to `total_borrowed` alone until the first submission.
		fn nav(pool_id: PoolId) -> Option<(U256, bool, u32)> {
			let current_epoch = T::Pools::current_epoch(pool_id)?;
			PoolEarnings::<T>::get(pool_id, current_epoch)
				.map(|e| (e.cumulative_pnl, e.is_loss, e.updated_block))
		}

		/// Returns the current epoch's `(cumulative_pnl, is_loss)`.
		///
		/// Returns `Ok((U256::zero(), false))` if no submission exists for the current epoch,
		/// matching the `unwrap_or_default()` semantics pallet-pools applies to `nav()`.
		fn update_nav(pool_id: PoolId) -> Result<(U256, bool), DispatchError> {
			Ok(Self::nav(pool_id).map(|(v, l, _)| (v, l)).unwrap_or((U256::zero(), false)))
		}
	}
}
