use crate::{ClaimableDepositOrder, ClaimableRedeemOrder, PendingDepositOrder, PendingRedeemOrder};

use pallet_pools::{PoolId, Settlement, TrancheId, TrancheMutate};
use sp_core::{H160, U256};
use sp_runtime::{DispatchError, FixedPointNumber, FixedU128};
use sp_std::vec::Vec;

use super::pallet::*;

impl<T: Config> Pallet<T> {
	/// Current block number as u32.
	pub fn current_block() -> u32 {
		frame_system::Pallet::<T>::block_number().try_into().unwrap_or(u32::MAX)
	}

	/// Converts a deposit asset amount to tranche shares at the given epoch price.
	/// shares = assets * accuracy / price_inner
	pub(crate) fn assets_to_shares(assets: U256, price: FixedU128) -> U256 {
		let price_inner = U256::from(price.into_inner());
		if price_inner.is_zero() {
			return assets;
		}
		assets.saturating_mul(U256::from(FixedU128::accuracy())) / price_inner
	}

	/// Converts a tranche share amount to deposit assets at the given epoch price.
	/// assets = shares * price_inner / accuracy
	pub(crate) fn shares_to_assets(shares: U256, price: FixedU128) -> U256 {
		shares.saturating_mul(U256::from(price.into_inner())) / U256::from(FixedU128::accuracy())
	}
}

impl<T: Config> Settlement<PoolId, TrancheId, U256> for Pallet<T> {
	/// Settle all pending deposit orders for a tranche at the given epoch price.
	///
	/// Settled orders move to `ClaimableDepositOrders`. Token supply is incremented
	/// immediately so `token_price()` stays accurate for subsequent epochs.
	///
	/// Returns the total amount settled (for `tranche.invested` accounting).
	fn settle_deposit_orders(
		pool_id: PoolId,
		tranche_id: TrancheId,
		epoch_price: FixedU128,
	) -> Result<U256, DispatchError> {
		let entries: Vec<(H160, PendingDepositOrder)> =
			PendingDepositOrders::<T>::iter_prefix(&tranche_id).collect();

		if entries.is_empty() {
			return Ok(U256::zero());
		}

		let total = entries.iter().fold(U256::zero(), |acc, (_, o)| acc.saturating_add(o.amount));

		if total.is_zero() {
			return Ok(U256::zero());
		}

		let _ = PendingDepositOrders::<T>::clear_prefix(&tranche_id, entries.len() as u32, None);

		let now = Self::current_block();
		let epoch_id = <T::Pools as crate::PoolInspect<T::AccountId>>::current_epoch(pool_id)
			.unwrap_or_default();

		let mut shares_total = U256::zero();

		for (investor_id, order) in &entries {
			let shares_to_mint = Self::assets_to_shares(order.amount, epoch_price);

			ClaimableDepositOrders::<T>::mutate(
				tranche_id.clone(),
				investor_id,
				|entry| match entry {
					Some(existing) => {
						existing.amount = existing.amount.saturating_add(order.amount);
						existing.shares_to_mint =
							existing.shares_to_mint.saturating_add(shares_to_mint);
						existing.epoch_id = epoch_id;
						existing.settled_at = now;
					},
					None => {
						*entry = Some(ClaimableDepositOrder {
							amount: order.amount,
							shares_to_mint,
							epoch_id,
							settled_at: now,
						});
					},
				},
			);

			Self::deposit_event(Event::DepositOrderSettled {
				pool_id,
				tranche_id: tranche_id.clone(),
				investor_id: *investor_id,
				amount: order.amount,
				shares_to_mint,
			});
			shares_total = shares_total.saturating_add(shares_to_mint);
		}

		if !shares_total.is_zero() {
			T::Pools::add_token_supply(pool_id, tranche_id, shares_total)?;
		}

		Ok(total)
	}

	/// Pro-rata settle pending redeem orders for a tranche up to `max_asset_payout`
	/// (the tranche's available treasury liquidity).
	///
	/// Settled orders move to `ClaimableRedeemOrders`.
	///
	/// Returns `(tokens_settled, asset_payout)` for `pending_orders.redeem` and
	/// `tranche.invested` accounting in pallet-pools.
	fn settle_redeem_orders(
		pool_id: PoolId,
		tranche_id: TrancheId,
		max_asset_payout: U256,
		epoch_price: FixedU128,
	) -> Result<(U256, U256), DispatchError> {
		let entries: Vec<(H160, PendingRedeemOrder)> =
			PendingRedeemOrders::<T>::iter_prefix(&tranche_id).collect();

		if entries.is_empty() {
			return Ok((U256::zero(), U256::zero()));
		}

		let total_tokens =
			entries.iter().fold(U256::zero(), |acc, (_, o)| acc.saturating_add(o.amount));

		if total_tokens.is_zero() {
			return Ok((U256::zero(), U256::zero()));
		}

		let total_asset_owed = Self::shares_to_assets(total_tokens, epoch_price);

		// Clear all pending; partial-fill remainders are re-inserted below.
		let _ = PendingRedeemOrders::<T>::clear_prefix(&tranche_id, entries.len() as u32, None);

		let now = Self::current_block();
		let epoch_id = <T::Pools as crate::PoolInspect<T::AccountId>>::current_epoch(pool_id)
			.unwrap_or_default();

		let mut tokens_settled_total = U256::zero();
		let mut asset_payout_total = U256::zero();

		if total_asset_owed <= max_asset_payout {
			// Full fill — settle every investor's order as-is.
			for (investor_id, order) in &entries {
				let payout = Self::shares_to_assets(order.amount, epoch_price);

				ClaimableRedeemOrders::<T>::mutate(tranche_id.clone(), investor_id, |entry| {
					match entry {
						Some(existing) => {
							existing.shares_redeemed =
								existing.shares_redeemed.saturating_add(order.amount);
							existing.payout = existing.payout.saturating_add(payout);
							existing.epoch_id = epoch_id;
							existing.settled_at = now;
						},
						None => {
							*entry = Some(ClaimableRedeemOrder {
								shares_redeemed: order.amount,
								payout,
								epoch_id,
								settled_at: now,
							});
						},
					}
				});

				Self::deposit_event(Event::RedeemOrderSettled {
					pool_id,
					tranche_id: tranche_id.clone(),
					investor_id: *investor_id,
					shares_redeemed: order.amount,
					payout,
				});

				tokens_settled_total = tokens_settled_total.saturating_add(order.amount);
				asset_payout_total = asset_payout_total.saturating_add(payout);
			}
		} else {
			// Partial fill — each investor receives floor(tokens * max_asset_payout / total_asset_owed).
			for (investor_id, order) in &entries {
				let tokens_confirmed =
					order.amount.saturating_mul(max_asset_payout) / total_asset_owed;
				let tokens_remainder = order.amount.saturating_sub(tokens_confirmed);

				if !tokens_confirmed.is_zero() {
					let payout = Self::shares_to_assets(tokens_confirmed, epoch_price);

					ClaimableRedeemOrders::<T>::mutate(tranche_id.clone(), investor_id, |entry| {
						match entry {
							Some(existing) => {
								existing.shares_redeemed =
									existing.shares_redeemed.saturating_add(tokens_confirmed);
								existing.payout = existing.payout.saturating_add(payout);
								existing.epoch_id = epoch_id;
								existing.settled_at = now;
							},
							None => {
								*entry = Some(ClaimableRedeemOrder {
									shares_redeemed: tokens_confirmed,
									payout,
									epoch_id,
									settled_at: now,
								});
							},
						}
					});

					Self::deposit_event(Event::RedeemOrderSettled {
						pool_id,
						tranche_id: tranche_id.clone(),
						investor_id: *investor_id,
						shares_redeemed: tokens_confirmed,
						payout,
					});

					tokens_settled_total = tokens_settled_total.saturating_add(tokens_confirmed);
					asset_payout_total = asset_payout_total.saturating_add(payout);
				}

				// Re-insert unconfirmed remainder preserving original epoch/block metadata.
				if !tokens_remainder.is_zero() {
					PendingRedeemOrders::<T>::insert(
						tranche_id.clone(),
						investor_id,
						PendingRedeemOrder {
							amount: tokens_remainder,
							epoch_id: order.epoch_id,
							submitted_at: order.submitted_at,
						},
					);
				}
			}
		}

		Ok((tokens_settled_total, asset_payout_total))
	}
}
