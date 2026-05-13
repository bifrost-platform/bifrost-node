use frame_support::ensure;
use sp_core::U256;
use sp_runtime::DispatchError;

use crate::{PoolId, PoolInspect, PoolNAV};

use super::pallet::*;

impl<T: Config> Pallet<T> {
	/// Current block number as u32.
	pub fn current_block() -> u32 {
		frame_system::Pallet::<T>::block_number().try_into().unwrap_or(u32::MAX)
	}

	/// Recompute NAV: accrue every active loan in the pool, sum their outstanding debt,
	/// persist the result with the current block number. Returns the new NAV.
	pub fn do_update_nav(pool_id: PoolId) -> Result<U256, DispatchError> {
		ensure!(T::Pools::pool_exists(pool_id), Error::<T>::PoolNotFound);
		let now = Self::current_block();

		let mut nav = U256::zero();
		for (loan_id, mut loan) in Loans::<T>::iter_prefix(pool_id) {
			if matches!(loan.status, crate::LoanStatus::Active) {
				loan.accrue(now);
				nav = nav.saturating_add(loan.debt());
				Loans::<T>::insert(pool_id, loan_id, loan);
			}
		}

		LastNav::<T>::insert(pool_id, (nav, now));
		Ok(nav)
	}
}

impl<T: Config> PoolNAV<PoolId, U256> for Pallet<T> {
	fn nav(pool_id: PoolId) -> Option<(U256, u32)> {
		LastNav::<T>::get(pool_id)
	}

	fn update_nav(pool_id: PoolId) -> Result<U256, DispatchError> {
		Self::do_update_nav(pool_id)
	}
}
