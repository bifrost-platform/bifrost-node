use crate::{EpochId, PoolId, PoolInspect, TrancheId, TrancheMutate};
use frame_support::traits::UnixTime;
use sp_core::{H160, U256};
use sp_runtime::DispatchError;

use super::pallet::*;

impl<T: Config> Pallet<T> {
	/// Current block number as u32.
	pub fn current_block() -> u32 {
		frame_system::Pallet::<T>::block_number().try_into().unwrap_or(u32::MAX)
	}
}

impl<T: Config> PoolInspect for Pallet<T> {
	fn pool_exists(pool_id: PoolId) -> bool {
		Pools::<T>::contains_key(pool_id)
	}

	fn tranche_exists(pool_id: PoolId, tranche_id: TrancheId) -> bool {
		Pools::<T>::get(pool_id)
			.map(|pool| pool.tranches.contains_key(&tranche_id))
			.unwrap_or(false)
	}

	fn in_settlement_window(pool_id: PoolId) -> bool {
		let now_secs = T::Time::now().as_secs();
		Pools::<T>::get(pool_id)
			.map(|pool| pool.epoch.in_settlement_window(now_secs))
			.unwrap_or(false)
	}

	fn deposit_cap_exceeded(pool_id: PoolId, tranche_id: TrancheId, amount: U256) -> bool {
		Pools::<T>::get(pool_id)
			.and_then(|pool| pool.tranches.get(&tranche_id).cloned())
			.map(|tranche| {
				let Some(cap) = tranche.max_deposits else {
					return false;
				};
				let current = tranche
					.reserve
					.saturating_add(tranche.borrowed)
					.saturating_add(tranche.pending_orders.deposit);
				current.saturating_add(amount) > cap
			})
			.unwrap_or(false)
	}

	fn reserve(pool_id: PoolId, tranche_id: TrancheId) -> U256 {
		Pools::<T>::get(pool_id)
			.and_then(|pool| pool.tranches.get(&tranche_id).cloned())
			.map(|tranche| tranche.reserve)
			.unwrap_or_default()
	}

	fn epoch_price(pool_id: PoolId, tranche_id: TrancheId) -> Option<U256> {
		Pools::<T>::get(pool_id)
			.and_then(|pool| pool.tranches.get(&tranche_id).cloned())
			.and_then(|tranche| tranche.epoch_price)
	}

	fn gateway_address() -> H160 {
		GatewayAddress::<T>::get()
	}

	fn current_epoch(pool_id: PoolId) -> Option<EpochId> {
		Pools::<T>::get(pool_id).map(|pool| pool.epoch.current_epoch)
	}

	fn deposit_settlement_mode(pool_id: PoolId) -> Option<crate::SettlementMode> {
		Pools::<T>::get(pool_id).map(|pool| pool.deposit_settlement)
	}

	fn redeem_settlement_mode(pool_id: PoolId) -> Option<crate::SettlementMode> {
		Pools::<T>::get(pool_id).map(|pool| pool.redeem_settlement)
	}
}

impl<T: Config> TrancheMutate<U256> for Pallet<T> {
	fn add_pending_deposit(
		pool_id: PoolId,
		tranche_id: TrancheId,
		amount: U256,
	) -> frame_support::dispatch::DispatchResult {
		Pools::<T>::try_mutate(pool_id, |maybe_pool| -> Result<(), DispatchError> {
			let pool = maybe_pool.as_mut().ok_or(Error::<T>::PoolNotFound)?;
			let tranche = pool.tranches.get_mut(&tranche_id).ok_or(Error::<T>::TrancheNotFound)?;
			tranche.pending_orders.deposit = tranche.pending_orders.deposit.saturating_add(amount);
			Ok(())
		})
	}

	fn sub_pending_deposit(
		pool_id: PoolId,
		tranche_id: TrancheId,
		amount: U256,
	) -> frame_support::dispatch::DispatchResult {
		Pools::<T>::try_mutate(pool_id, |maybe_pool| -> Result<(), DispatchError> {
			let pool = maybe_pool.as_mut().ok_or(Error::<T>::PoolNotFound)?;
			let tranche = pool.tranches.get_mut(&tranche_id).ok_or(Error::<T>::TrancheNotFound)?;
			tranche.pending_orders.deposit = tranche.pending_orders.deposit.saturating_sub(amount);
			Ok(())
		})
	}

	fn add_pending_redeem(
		pool_id: PoolId,
		tranche_id: TrancheId,
		amount: U256,
	) -> frame_support::dispatch::DispatchResult {
		Pools::<T>::try_mutate(pool_id, |maybe_pool| -> Result<(), DispatchError> {
			let pool = maybe_pool.as_mut().ok_or(Error::<T>::PoolNotFound)?;
			let tranche = pool.tranches.get_mut(&tranche_id).ok_or(Error::<T>::TrancheNotFound)?;
			tranche.pending_orders.redeem = tranche.pending_orders.redeem.saturating_add(amount);
			Ok(())
		})
	}

	fn sub_pending_redeem(
		pool_id: PoolId,
		tranche_id: TrancheId,
		amount: U256,
	) -> frame_support::dispatch::DispatchResult {
		Pools::<T>::try_mutate(pool_id, |maybe_pool| -> Result<(), DispatchError> {
			let pool = maybe_pool.as_mut().ok_or(Error::<T>::PoolNotFound)?;
			let tranche = pool.tranches.get_mut(&tranche_id).ok_or(Error::<T>::TrancheNotFound)?;
			tranche.pending_orders.redeem = tranche.pending_orders.redeem.saturating_sub(amount);
			Ok(())
		})
	}

	fn sub_token_supply(
		pool_id: PoolId,
		tranche_id: TrancheId,
		amount: U256,
	) -> frame_support::dispatch::DispatchResult {
		Pools::<T>::try_mutate(pool_id, |maybe_pool| -> Result<(), DispatchError> {
			let pool = maybe_pool.as_mut().ok_or(Error::<T>::PoolNotFound)?;
			let tranche = pool.tranches.get_mut(&tranche_id).ok_or(Error::<T>::TrancheNotFound)?;
			tranche.token_supply = tranche.token_supply.saturating_sub(amount);
			Ok(())
		})
	}

	fn add_token_supply(
		pool_id: PoolId,
		tranche_id: TrancheId,
		amount: U256,
	) -> frame_support::dispatch::DispatchResult {
		Pools::<T>::try_mutate(pool_id, |maybe_pool| -> Result<(), DispatchError> {
			let pool = maybe_pool.as_mut().ok_or(Error::<T>::PoolNotFound)?;
			let tranche = pool.tranches.get_mut(&tranche_id).ok_or(Error::<T>::TrancheNotFound)?;
			tranche.token_supply = tranche.token_supply.saturating_add(amount);
			Ok(())
		})
	}

	fn add_reserve(
		pool_id: PoolId,
		tranche_id: TrancheId,
		amount: U256,
	) -> frame_support::dispatch::DispatchResult {
		Pools::<T>::try_mutate(pool_id, |maybe_pool| -> Result<(), DispatchError> {
			let pool = maybe_pool.as_mut().ok_or(Error::<T>::PoolNotFound)?;
			let tranche = pool.tranches.get_mut(&tranche_id).ok_or(Error::<T>::TrancheNotFound)?;
			tranche.reserve = tranche.reserve.saturating_add(amount);
			Ok(())
		})
	}

	fn sub_reserve(
		pool_id: PoolId,
		tranche_id: TrancheId,
		amount: U256,
	) -> frame_support::dispatch::DispatchResult {
		Pools::<T>::try_mutate(pool_id, |maybe_pool| -> Result<(), DispatchError> {
			let pool = maybe_pool.as_mut().ok_or(Error::<T>::PoolNotFound)?;
			let tranche = pool.tranches.get_mut(&tranche_id).ok_or(Error::<T>::TrancheNotFound)?;
			tranche.reserve = tranche.reserve.saturating_sub(amount);
			Ok(())
		})
	}
}
