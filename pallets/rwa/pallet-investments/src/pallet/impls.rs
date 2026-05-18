use crate::TrancheId;
use sp_core::U256;

use super::pallet::*;

impl<T: Config> Pallet<T> {
	/// Current block number as u32.
	pub fn current_block() -> u32 {
		frame_system::Pallet::<T>::block_number().try_into().unwrap_or(u32::MAX)
	}

	/// Drain all `ConfirmedRedeemOrders` for `tranche_id`, returning the sum of
	/// all token amounts. Called by `execute_redeem_orders`.
	pub(crate) fn drain_confirmed_redeem(tranche_id: TrancheId) -> U256 {
		let entries: sp_std::vec::Vec<_> =
			ConfirmedRedeemOrders::<T>::iter_prefix(&tranche_id).collect();
		let _ = ConfirmedRedeemOrders::<T>::clear_prefix(&tranche_id, entries.len() as u32, None);

		entries
			.into_iter()
			.fold(U256::zero(), |acc, (_, tokens)| acc.saturating_add(tokens))
	}
}
