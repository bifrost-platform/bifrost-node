use frame_support::ensure;
use sp_core::U256;
use sp_runtime::DispatchError;

use crate::{
	InvestmentSettlement, PoolDetails, PoolId, PoolInspect, PoolReserve, TrancheId, TrancheIndex,
};

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

	/// Pro-rata invest settlement: allocates available reserve capacity across all
	/// tranches in seniority order (index 0 = most senior fills first).
	///
	/// For each tranche, delegates to `T::Investments::settle_invest_orders` which
	/// handles the per-investor pro-rata distribution and writes `ConfirmedInvestOrders`.
	/// The pool reserve is credited by the actual USDC confirmed per tranche.
	///
	/// Called from `on_initialize` for Automatic pools.
	pub(crate) fn settle_invest_orders(pool_id: PoolId, pool: &mut PoolDetails<T::AccountId>) {
		let reserve_space = pool.reserve.max.saturating_sub(pool.reserve.total);
		if reserve_space.is_zero() {
			return;
		}

		let mut remaining = reserve_space;

		for tranche in pool.tranches.iter() {
			if remaining.is_zero() {
				break;
			}
			let actual =
				T::Investments::settle_invest_orders(pool_id, tranche.tranche_id.clone(), remaining);
			if !actual.is_zero() {
				pool.reserve.deposit(actual);
				remaining = remaining.saturating_sub(actual);
			}
		}
	}
}

impl<T: Config> PoolInspect<T::AccountId> for Pallet<T> {
	fn pool_exists(pool_id: PoolId) -> bool {
		Pool::<T>::contains_key(pool_id)
	}

	fn pool_admin(pool_id: PoolId) -> Option<T::AccountId> {
		Pool::<T>::get(pool_id).map(|p| p.admin)
	}

	fn tranche_exists(pool_id: PoolId, tranche_id: TrancheId) -> bool {
		Pool::<T>::get(pool_id)
			.map(|p| p.tranches.iter().any(|t| t.tranche_id == tranche_id))
			.unwrap_or(false)
	}

	fn in_settlement_window(pool_id: PoolId) -> bool {
		let now = Self::current_block();
		Pool::<T>::get(pool_id)
			.map(|p| p.epoch.in_settlement_window(now))
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
