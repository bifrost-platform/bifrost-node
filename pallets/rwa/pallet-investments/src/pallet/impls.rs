use pallet_pools::{DepositSettlement, PoolId, TrancheId, TrancheMutate};
use sp_core::{H160, U256};
use sp_runtime::{FixedPointNumber, FixedU128};

use super::pallet::*;

impl<T: Config> Pallet<T> {
	/// Current block number as u32.
	pub fn current_block() -> u32 {
		frame_system::Pallet::<T>::block_number().try_into().unwrap_or(u32::MAX)
	}

	/// Converts a deposit asset amount to tranche shares at the given epoch price.
	/// shares = assets * accuracy / price_inner
	///
	/// `FixedU128::accuracy()` = 10^18; `price.into_inner()` = price * 10^18.
	/// All arithmetic in U256 to avoid overflow.
	pub(crate) fn assets_to_shares(assets: U256, price: FixedU128) -> U256 {
		let price_inner = U256::from(price.into_inner());
		if price_inner.is_zero() {
			return assets;
		}
		assets.saturating_mul(U256::from(FixedU128::accuracy())) / price_inner
	}

	/// Converts a tranche share amount to deposit assets at the given epoch price.
	/// assets = shares * price_inner / accuracy
	///
	/// `FixedU128::accuracy()` = 10^18; `price.into_inner()` = price * 10^18.
	/// All arithmetic in U256 to avoid overflow.
	pub(crate) fn shares_to_assets(shares: U256, price: FixedU128) -> U256 {
		shares.saturating_mul(U256::from(price.into_inner())) / U256::from(FixedU128::accuracy())
	}
}

impl<T: Config> DepositSettlement<PoolId, TrancheId, U256> for Pallet<T> {
	/// Pro-rata confirm pending deposit orders for a tranche up to `max_amount` USDC.
	///
	/// - Full fill  (total pending <= max_amount): every investor is confirmed in full.
	/// - Partial fill (total pending >  max_amount): each investor's order is scaled by
	///   `max_amount / total`; the remainder stays in `PendingDepositOrders`.
	///
	/// Converts confirmed USDC amounts to tokens-to-mint using `epoch_price` and stores
	/// tokens in `ApprovedDepositOrders`.
	///
	/// Emits `DepositOrderConfirmed` per investor. Returns actual USDC confirmed
	/// (for `tranche.invested` accounting in pallet-pools).
	fn settle_deposit_orders(
		pool_id: PoolId,
		tranche_id: TrancheId,
		max_amount: U256,
		epoch_price: FixedU128,
	) -> U256 {
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
		let mut tokens_total = U256::zero();

		if max_amount >= total {
			// Full fill — confirm every investor's order as-is.
			for (investor_id, amount) in &entries {
				let tokens_to_mint = Self::assets_to_shares(*amount, epoch_price);
				ApprovedDepositOrders::<T>::mutate(tranche_id.clone(), investor_id, |e| {
					*e = Some(e.unwrap_or_default().saturating_add(tokens_to_mint));
				});
				Self::deposit_event(Event::DepositOrderApproved {
					pool_id,
					tranche_id: tranche_id.clone(),
					investor_id: *investor_id,
					usdc_amount: *amount,
					tokens_to_mint,
				});
				confirmed_total = confirmed_total.saturating_add(*amount);
				tokens_total = tokens_total.saturating_add(tokens_to_mint);
			}
		} else {
			// Partial fill — scale each order by fill_ratio = max_amount / total.
			let max_u128: u128 = max_amount.try_into().unwrap_or(u128::MAX);
			let total_u128: u128 = total.try_into().unwrap_or(u128::MAX);
			let fill_ratio = FixedU128::from_rational(max_u128, total_u128);

			for (investor_id, pending) in &entries {
				let pending_u128: u128 = (*pending).try_into().unwrap_or(u128::MAX);
				let confirmed = U256::from(fill_ratio.saturating_mul_int(pending_u128));
				let remainder = pending.saturating_sub(confirmed);

				if !confirmed.is_zero() {
					let tokens_to_mint = Self::assets_to_shares(confirmed, epoch_price);
					ApprovedDepositOrders::<T>::mutate(tranche_id.clone(), investor_id, |e| {
						*e = Some(e.unwrap_or_default().saturating_add(tokens_to_mint));
					});
					Self::deposit_event(Event::DepositOrderApproved {
						pool_id,
						tranche_id: tranche_id.clone(),
						investor_id: *investor_id,
						usdc_amount: confirmed,
						tokens_to_mint,
					});
					confirmed_total = confirmed_total.saturating_add(confirmed);
					tokens_total = tokens_total.saturating_add(tokens_to_mint);
				}

				// Re-insert unconfirmed remainder for the next epoch.
				if !remainder.is_zero() {
					PendingDepositOrders::<T>::insert(tranche_id.clone(), investor_id, remainder);
				}
			}
		}

		if !tokens_total.is_zero() {
			let _ = T::Pools::add_token_supply(pool_id, tranche_id, tokens_total);
		}

		confirmed_total
	}
}
