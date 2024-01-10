use crate::{BalanceOf, Offence, ValidatorOffenceInfo};

use super::pallet::*;

use bp_staking::{traits::OffenceHandler, TierType};
use frame_support::traits::{OnUnbalanced, ReservableCurrency};
use sp_runtime::{traits::Zero, Perbill};
use sp_staking::SessionIndex;

impl<T: Config> OffenceHandler<T::AccountId, BalanceOf<T>> for Pallet<T> {
	fn try_handle_offence(
		who: &T::AccountId,
		stash: &T::AccountId,
		tier: TierType,
		offence: Offence<BalanceOf<T>>,
	) -> (bool, BalanceOf<T>) {
		// offence check only if activated
		if IsOffenceActive::<T>::get() {
			return Self::handle_offence(who, stash, tier, offence);
		}
		return (false, BalanceOf::<T>::zero());
	}

	fn handle_offence(
		who: &T::AccountId,
		stash: &T::AccountId,
		tier: TierType,
		offence: Offence<BalanceOf<T>>,
	) -> (bool, BalanceOf<T>) {
		let mut slash_amount = BalanceOf::<T>::zero();
		// Check if the validator had offenced before.
		if let Some(mut offences) = ValidatorOffences::<T>::get(who) {
			if Self::is_offence_count_exceeds(offences.offence_count + 1, tier) {
				// apply offence penalty to this validator
				slash_amount = Self::try_slash(
					who,
					stash,
					offences.aggregated_slash_fraction,
					offence.self_bond,
				);
				ValidatorOffences::<T>::remove(who);
				return (true, slash_amount);
			}

			// add a new offence and increase offence count to this validator
			offences.add_offence(offence);
			ValidatorOffences::<T>::insert(who, offences);

			(false, slash_amount)
		} else {
			// add the initial offence to this validator
			ValidatorOffences::<T>::insert(who, ValidatorOffenceInfo::new(offence));
			return (false, slash_amount);
		}
	}

	/// Slash validator account if IsSlashActive is active
	fn try_slash(
		who: &T::AccountId,
		stash: &T::AccountId,
		slash_fraction: Perbill,
		bond: BalanceOf<T>,
	) -> BalanceOf<T> {
		// slash bonds only if activated
		if Self::is_slash_active() {
			let slash_amount = slash_fraction * bond;
			// slash the validator's reserved self bond
			// the slashed imbalance will be reserved to the treasury
			T::Slash::on_unbalanced(T::Currency::slash_reserved(stash, slash_amount).0);
			Self::deposit_event(Event::Slashed { who: who.clone(), amount: slash_amount });
			return slash_amount;
		}
		BalanceOf::<T>::zero()
	}

	fn refresh_offences(session_index: SessionIndex) {
		<ValidatorOffences<T>>::iter().for_each(|offences| {
			if (session_index - offences.1.latest_offence_session_index)
				> Self::offence_expiration_in_sessions()
			{
				<ValidatorOffences<T>>::remove(&offences.0);
			}
		});
	}

	fn is_offence_count_exceeds(count: u32, tier: TierType) -> bool {
		// if offence count exceeds the configured limit
		return match tier {
			TierType::Full => count > Self::full_maximum_offence_count(),
			_ => count > Self::basic_maximum_offence_count(),
		};
	}
}
