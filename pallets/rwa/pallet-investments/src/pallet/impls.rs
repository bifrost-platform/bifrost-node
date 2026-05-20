use pallet_pools::{DepositSettlement, PoolId, TrancheId};
use sp_core::{H160, U256};
use sp_runtime::{FixedPointNumber, FixedU128};

use super::pallet::*;

impl<T: Config> Pallet<T> {
	/// Current block number as u32.
	pub fn current_block() -> u32 {
		frame_system::Pallet::<T>::block_number().try_into().unwrap_or(u32::MAX)
	}
}

impl<T: Config> DepositSettlement<PoolId, TrancheId, U256> for Pallet<T> {
	/// Pro-rata confirm pending deposit orders for a tranche up to `max_amount` USDC.
	///
	/// - Full fill  (total pending <= max_amount): every investor is confirmed in full.
	/// - Partial fill (total pending >  max_amount): each investor's order is scaled by
	///   `max_amount / total`; the remainder stays in `PendingDepositOrders`.
	///
	/// Emits `DepositOrderConfirmed` per investor. Returns actual USDC confirmed.
	fn settle_deposit_orders(pool_id: PoolId, tranche_id: TrancheId, max_amount: U256) -> U256 {
		let entries: sp_std::vec::Vec<(H160, U256)> =
			PendingDepositOrders::<T>::iter_prefix(&tranche_id).collect();

		if entries.is_empty() {
			return U256::zero();
		}

		let total = entries.iter().fold(U256::zero(), |acc, (_, amt)| acc.saturating_add(*amt));

		if total.is_zero() {
			return U256::zero();
		}

		// Clear all pending; remainders are re-inserted below if partial fill.
		let _ = PendingDepositOrders::<T>::clear_prefix(&tranche_id, entries.len() as u32, None);

		let mut confirmed_total = U256::zero();

		if max_amount >= total {
			// Full fill — confirm every investor's order as-is.
			for (investor, amount) in &entries {
				ConfirmedDepositOrders::<T>::mutate(tranche_id.clone(), investor, |e| {
					*e = Some(e.unwrap_or_default().saturating_add(*amount));
				});
				Self::deposit_event(Event::DepositOrderConfirmed {
					pool_id,
					tranche_id: tranche_id.clone(),
					investor: *investor,
					amount: *amount,
				});
				confirmed_total = confirmed_total.saturating_add(*amount);
			}
		} else {
			// Partial fill — scale each order by fill_ratio = max_amount / total.
			let max_u128: u128 = max_amount.try_into().unwrap_or(u128::MAX);
			let total_u128: u128 = total.try_into().unwrap_or(u128::MAX);
			let fill_ratio = FixedU128::from_rational(max_u128, total_u128);

			for (investor, pending) in &entries {
				let pending_u128: u128 = (*pending).try_into().unwrap_or(u128::MAX);
				let confirmed = U256::from(fill_ratio.saturating_mul_int(pending_u128));
				let remainder = pending.saturating_sub(confirmed);

				if !confirmed.is_zero() {
					ConfirmedDepositOrders::<T>::mutate(tranche_id.clone(), investor, |e| {
						*e = Some(e.unwrap_or_default().saturating_add(confirmed));
					});
					Self::deposit_event(Event::DepositOrderConfirmed {
						pool_id,
						tranche_id: tranche_id.clone(),
						investor: *investor,
						amount: confirmed,
					});
					confirmed_total = confirmed_total.saturating_add(confirmed);
				}

				// Re-insert unconfirmed remainder for the next epoch.
				if !remainder.is_zero() {
					PendingDepositOrders::<T>::insert(tranche_id.clone(), investor, remainder);
				}
			}
		}

		confirmed_total
	}
}
