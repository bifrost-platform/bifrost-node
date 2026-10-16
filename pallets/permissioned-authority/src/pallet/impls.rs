use super::pallet::*;

use crate::log;

use bp_staking::traits::{RelayManager, RelayerSetManager};
use frame_support::{pallet_prelude::*, traits::Get};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_runtime::Perbill;
use sp_staking::{
	offence::{OffenceDetails, OnOffenceHandler},
	SessionIndex,
};
use sp_std::prelude::*;

impl<T: Config> Pallet<T> {
	/// Whether the next session rotation must apply a new round: the requested authority set
	/// differs from the active one, or a relayer address change is queued.
	pub fn is_round_change_pending() -> bool {
		<Authorities<T>>::get() != <ActiveAuthorities<T>>::get()
			|| T::RelayerSets::has_pending_relayer_sets()
	}

	/// Start a new round with the requested authority set. Mirrors the `RelayManager` hook
	/// sequence of `pallet-bfc-staking`'s round transition.
	fn apply_new_round(now: BlockNumberFor<T>) -> Vec<T::AccountId> {
		let mut round = <Round<T>>::get();
		round.update_round(now);
		let round_index = round.current_round_index;
		<Round<T>>::put(round);

		let requested = <Authorities<T>>::get();
		let active = <ActiveAuthorities<T>>::get();

		// apply queued relayer address changes (requested in the previous round) first, so the
		// initial and current relayer states match at the start of the new round
		T::RelayManager::refresh_round(round_index);
		T::RelayManager::handle_delayed_relayer_sets(round_index);
		// unbond the relayers of removed authorities
		active
			.iter()
			.filter(|v| !requested.contains(v))
			.for_each(|v| T::RelayManager::leave_relayers(v));
		T::RelayManager::refresh_selected_relayers(round_index, requested.to_vec());
		T::RelayManager::refresh_majority(round_index);

		<ActiveAuthorities<T>>::put(&requested);

		let validators = requested.into_inner();
		log!(info, "new round #{} with {} authorities", round_index, validators.len());
		Self::deposit_event(Event::NewRound { round: round_index, validators: validators.clone() });
		validators
	}
}

impl<T: Config> pallet_session::SessionManager<T::AccountId> for Pallet<T> {
	fn new_session(new_index: SessionIndex) -> Option<Vec<T::AccountId>> {
		let now = <frame_system::Pallet<T>>::block_number();

		<Round<T>>::mutate(|round| round.update_session(now, new_index.saturating_sub(1)));

		// genesis: `pallet-session` builds before this pallet, so the active set may still be
		// empty — fall back to the session genesis keys.
		if new_index <= 1 {
			let active = <ActiveAuthorities<T>>::get();
			return if active.is_empty() { None } else { Some(active.into_inner()) };
		}

		if Self::is_round_change_pending() {
			Some(Self::apply_new_round(now))
		} else {
			None
		}
	}

	fn end_session(_end_index: SessionIndex) {
		T::RelayManager::collect_heartbeats();
	}

	fn start_session(_start_index: SessionIndex) {}
}

/// Offences (e.g. `pallet-im-online` unresponsiveness) are only reported as events: authorities
/// are removed by root alone, so no offence can shrink — or empty — the validator set.
impl<T: Config, FullIdentification>
	OnOffenceHandler<T::AccountId, (T::AccountId, FullIdentification), Weight> for Pallet<T>
{
	fn on_offence(
		offenders: &[OffenceDetails<T::AccountId, (T::AccountId, FullIdentification)>],
		slash_fraction: &[Perbill],
		session: SessionIndex,
	) -> Weight {
		for (details, slash_fraction) in offenders.iter().zip(slash_fraction) {
			let offender = details.offender.0.clone();
			log!(warn, "offence reported for {:?} (session {})", offender, session);
			Self::deposit_event(Event::OffenceReported {
				offender,
				slash_fraction: *slash_fraction,
				session,
			});
		}
		T::DbWeight::get().writes(offenders.len() as u64)
	}
}
