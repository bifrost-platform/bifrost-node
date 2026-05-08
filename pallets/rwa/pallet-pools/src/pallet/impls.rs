use frame_support::ensure;
use sp_core::U256;
use sp_runtime::DispatchError;

use crate::{PoolId, PoolInspect, PoolReserve, TrancheId, TrancheIndex};

use super::pallet::*;

impl<T: Config> Pallet<T> {
	/// Current block number as u32.
	pub fn current_block() -> u32 {
		frame_system::Pallet::<T>::block_number().try_into().unwrap_or(u32::MAX)
	}

	/// Look up the index of a tranche within a pool by its TrancheId.
	pub fn tranche_index_by_id(pool_id: PoolId, tranche_id: TrancheId) -> Option<TrancheIndex> {
		let pool = Pool::<T>::get(pool_id)?;
		pool.tranches
			.iter()
			.position(|t| t.tranche_id == tranche_id)
			.map(|i| i as TrancheIndex)
	}
}

impl<T: Config> PoolInspect for Pallet<T> {
	fn tranche_exists(pool_id: PoolId, tranche_id: TrancheId) -> bool {
		Pool::<T>::get(pool_id)
			.map(|p| p.tranches.iter().any(|t| t.tranche_id == tranche_id))
			.unwrap_or(false)
	}
}

impl<T: Config> PoolReserve<U256> for Pallet<T> {
	fn withdraw(pool_id: PoolId, amount: U256) -> frame_support::dispatch::DispatchResult {
		Pool::<T>::try_mutate(pool_id, |maybe_pool| -> Result<(), DispatchError> {
			let pool = maybe_pool.as_mut().ok_or(Error::<T>::PoolNotFound)?;
			ensure!(pool.reserve.withdraw(amount), Error::<T>::InsufficientReserve);
			Self::deposit_event(Event::ReserveUpdated {
				pool_id,
				total: pool.reserve.total,
				available: pool.reserve.available,
			});
			Ok(())
		})
	}

	fn deposit(pool_id: PoolId, amount: U256) -> frame_support::dispatch::DispatchResult {
		Pool::<T>::try_mutate(pool_id, |maybe_pool| -> Result<(), DispatchError> {
			let pool = maybe_pool.as_mut().ok_or(Error::<T>::PoolNotFound)?;
			pool.reserve.deposit(amount);
			Self::deposit_event(Event::ReserveUpdated {
				pool_id,
				total: pool.reserve.total,
				available: pool.reserve.available,
			});
			Ok(())
		})
	}

	fn available_reserve(pool_id: PoolId) -> U256 {
		Pool::<T>::get(pool_id).map(|p| p.reserve.available).unwrap_or_default()
	}
}
