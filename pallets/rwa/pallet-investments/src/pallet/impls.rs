use crate::{ApprovedDepositOrder, PendingDepositOrder};
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
	/// them in `ApprovedDepositOrders` with the settling epoch/block metadata.
	///
	/// Emits `DepositOrderApproved` per investor. Returns actual USDC confirmed
	/// (for `tranche.invested` accounting in pallet-pools).
	fn settle_deposit_orders(
		pool_id: PoolId,
		tranche_id: TrancheId,
		max_amount: U256,
		epoch_price: FixedU128,
	) -> U256 {
		let entries: sp_std::vec::Vec<(H160, PendingDepositOrder)> =
			PendingDepositOrders::<T>::iter_prefix(&tranche_id).collect();

		if entries.is_empty() {
			return U256::zero();
		}

		let total = entries.iter().fold(U256::zero(), |acc, (_, o)| acc.saturating_add(o.amount));

		if total.is_zero() {
			return U256::zero();
		}

		// Clear all pending; partial-fill remainders are re-inserted below.
		let _ = PendingDepositOrders::<T>::clear_prefix(&tranche_id, entries.len() as u32, None);

		let now = Self::current_block();
		let epoch_id = <T::Pools as crate::PoolInspect<T::AccountId>>::current_epoch(pool_id)
			.unwrap_or_default();

		let mut confirmed_total = U256::zero();
		let mut shares_total = U256::zero();

		if max_amount >= total {
			// Full fill — confirm every investor's order as-is.
			for (investor_id, order) in &entries {
				let shares_to_mint = Self::assets_to_shares(order.amount, epoch_price);

				ApprovedDepositOrders::<T>::mutate(tranche_id.clone(), investor_id, |entry| {
					match entry {
						Some(existing) => {
							existing.amount = existing.amount.saturating_add(order.amount);
							existing.shares_to_mint =
								existing.shares_to_mint.saturating_add(shares_to_mint);
							existing.epoch_id = epoch_id;
							existing.approved_at = now;
						},
						None => {
							*entry = Some(ApprovedDepositOrder {
								amount: order.amount,
								shares_to_mint,
								epoch_id,
								approved_at: now,
							});
						},
					}
				});

				Self::deposit_event(Event::DepositOrderApproved {
					pool_id,
					tranche_id: tranche_id.clone(),
					investor_id: *investor_id,
					amount: order.amount,
					shares_to_mint,
				});
				confirmed_total = confirmed_total.saturating_add(order.amount);
				shares_total = shares_total.saturating_add(shares_to_mint);
			}
		} else {
			// Partial fill — scale each order by fill_ratio = max_amount / total.
			let max_u128: u128 = max_amount.try_into().unwrap_or(u128::MAX);
			let total_u128: u128 = total.try_into().unwrap_or(u128::MAX);
			let fill_ratio = FixedU128::from_rational(max_u128, total_u128);

			for (investor_id, order) in &entries {
				let pending_u128: u128 = order.amount.try_into().unwrap_or(u128::MAX);
				let confirmed = U256::from(fill_ratio.saturating_mul_int(pending_u128));
				let remainder = order.amount.saturating_sub(confirmed);

				if !confirmed.is_zero() {
					let shares_to_mint = Self::assets_to_shares(confirmed, epoch_price);

					ApprovedDepositOrders::<T>::mutate(tranche_id.clone(), investor_id, |entry| {
						match entry {
							Some(existing) => {
								existing.amount = existing.amount.saturating_add(confirmed);
								existing.shares_to_mint =
									existing.shares_to_mint.saturating_add(shares_to_mint);
								existing.epoch_id = epoch_id;
								existing.approved_at = now;
							},
							None => {
								*entry = Some(ApprovedDepositOrder {
									amount: confirmed,
									shares_to_mint,
									epoch_id,
									approved_at: now,
								});
							},
						}
					});

					Self::deposit_event(Event::DepositOrderApproved {
						pool_id,
						tranche_id: tranche_id.clone(),
						investor_id: *investor_id,
						amount: confirmed,
						shares_to_mint,
					});
					confirmed_total = confirmed_total.saturating_add(confirmed);
					shares_total = shares_total.saturating_add(shares_to_mint);
				}

				// Re-insert unconfirmed remainder preserving original epoch/block metadata.
				if !remainder.is_zero() {
					PendingDepositOrders::<T>::insert(
						tranche_id.clone(),
						investor_id,
						PendingDepositOrder {
							amount: remainder,
							epoch_id: order.epoch_id,
							submitted_at: order.submitted_at,
						},
					);
				}
			}
		}

		if !shares_total.is_zero() {
			let _ = T::Pools::add_token_supply(pool_id, tranche_id, shares_total);
		}

		confirmed_total
	}
}
