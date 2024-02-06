use super::pallet::*;

use crate::{pallet::*, IdentificationTuple, Relayer, RelayerMetadata, UnresponsivenessOffence};

use bp_staking::{traits::RelayManager, RoundIndex};
use frame_support::{
	pallet_prelude::*,
	traits::{ValidatorSet, ValidatorSetWithIdentification},
	BoundedBTreeSet,
};
use sp_runtime::traits::Convert;
use sp_staking::offence::ReportOffence;
use sp_std::{vec, vec::Vec};

impl<T: Config> RelayManager<T::AccountId> for Pallet<T>
where
	<T as frame_system::Config>::AccountId: From<
		<<T as Config>::ValidatorSet as ValidatorSet<
			<T as frame_system::Config>::AccountId,
		>>::ValidatorId,
	>,
{
	fn join_relayers(relayer: T::AccountId, controller: T::AccountId) -> DispatchResult {
		Self::verify_relayer_existence(&relayer, &controller)?;
		Self::add_to_relayer_pool(relayer.clone(), controller.clone())?;
		<RelayerState<T>>::insert(&relayer, RelayerMetadata::new(controller.clone()));
		<BondedController<T>>::insert(&controller, relayer.clone());
		Self::deposit_event(Event::JoinedRelayers { relayer, controller });
		Ok(())
	}

	fn refresh_round(now: RoundIndex) {
		<Round<T>>::put(now);
	}

	fn refresh_relayer_pool() {
		let pool = Self::relayer_pool();
		pool.iter().for_each(|r| {
			let mut relayer_state =
				Self::relayer_state(&r.relayer).expect("RelayerState must exist");
			if !relayer_state.is_kicked_out() {
				relayer_state.go_offline();
			}
			<RelayerState<T>>::insert(&r.relayer, relayer_state);
		});
	}

	fn refresh_selected_relayers(round: RoundIndex, selected_candidates: Vec<T::AccountId>) {
		let mut selected_relayers: BoundedBTreeSet<T::AccountId, ConstU32<MAX_AUTHORITIES>> = Default::default();
		Self::refresh_relayer_pool();

		for controller in selected_candidates {
			if let Some(relayer) = Self::bonded_controller(&controller) {
				selected_relayers.try_insert(relayer.clone()).expect(<Error<T>>::TooManySelectedRelayers.as_str());
				let mut relayer_state =
					Self::relayer_state(&relayer).expect("RelayerState must exist");
				relayer_state.go_online();
				<RelayerState<T>>::insert(&relayer, relayer_state);
				Self::deposit_event(Event::RelayerChosen {
					round,
					relayer,
					controller,
				});
			}
		}
		<SelectedRelayers<T>>::put(selected_relayers.clone());
		<InitialSelectedRelayers<T>>::put(selected_relayers.clone());
		Self::refresh_cached_selected_relayers(round, selected_relayers);
	}

	fn refresh_cached_selected_relayers(round: RoundIndex, relayers: BoundedBTreeSet<T::AccountId, ConstU32<MAX_AUTHORITIES>>) {
		let mut cached_selected_relayers = Self::cached_selected_relayers();
		let mut cached_initial_selected_relayers = Self::cached_initial_selected_relayers();
		if Self::storage_cache_lifetime() <= cached_selected_relayers.len() as u32 {
			cached_selected_relayers.pop_first();
		}
		if Self::storage_cache_lifetime() <= cached_initial_selected_relayers.len() as u32 {
			cached_initial_selected_relayers.pop_first();
		}
		cached_selected_relayers.insert(round, relayers.clone());
		cached_initial_selected_relayers.insert(round, relayers.clone());
		<CachedSelectedRelayers<T>>::put(cached_selected_relayers);
		<CachedInitialSelectedRelayers<T>>::put(cached_initial_selected_relayers);
	}

	fn refresh_majority(round: RoundIndex) {
		let mut cached_majority = Self::cached_majority();
		let mut cached_initial_majority = Self::cached_initial_majority();
		if Self::storage_cache_lifetime() <= cached_majority.len() as u32 {
			cached_majority.pop_first();
		}
		if Self::storage_cache_lifetime() <= cached_initial_majority.len() as u32 {
			cached_initial_majority.pop_first();
		}
		let majority: u32 = Self::compute_majority();
		cached_majority.insert(round, majority);
		cached_initial_majority.insert(round, majority);
		<Majority<T>>::put(majority);
		<InitialMajority<T>>::put(majority);
		<CachedMajority<T>>::put(cached_majority);
		<CachedInitialMajority<T>>::put(cached_initial_majority);
	}

	fn replace_bonded_controller(old: T::AccountId, new: T::AccountId) {
		if let Some(relayer) = <BondedController<T>>::take(&old) {
			<BondedController<T>>::insert(&new, relayer.clone());
			let mut relayer_state =
				Self::relayer_state(&relayer).expect("RelayerState must exist");
			relayer_state.set_controller(new.clone());
			<RelayerState<T>>::insert(&relayer, relayer_state);
			Self::remove_from_relayer_pool(&new, false);
			Self::add_to_relayer_pool(relayer.clone(), new.clone()).expect(<Error<T>>::TooManyRelayers.as_str());
		}
	}

	fn leave_relayers(controller: &T::AccountId) {
		if let Some(relayer) = <BondedController<T>>::take(controller) {
			Self::remove_from_relayer_pool(&relayer, true);
			<RelayerState<T>>::remove(&relayer);
		}
	}

	fn kickout_relayer(controller: &T::AccountId) {
		if let Some(relayer) = Self::bonded_controller(controller) {
			let mut relayer_state =
				Self::relayer_state(&relayer).expect("RelayerState must exist");
			relayer_state.kick_out();
			<RelayerState<T>>::insert(&relayer, relayer_state);

			// refresh selected relayers
			if Self::remove_from_selected_relayers(&relayer) {
				Self::refresh_latest_cached_relayers();
				// refresh majority
				let majority: u32 = Self::compute_majority();
				<Majority<T>>::put(majority);
				Self::refresh_latest_cached_majority();
			}
		}
	}

	fn collect_heartbeats() {
		let current_validators = T::ValidatorSet::validators();
		let session_index = T::ValidatorSet::session_index();
		let offenders = current_validators
			.clone()
			.into_iter()
			.enumerate()
			.filter(|(_, id)| {
				let controller: T::AccountId = id.clone().into();
				if let Some(relayer) = Self::bonded_controller(&controller) {
					!Self::is_heartbeat_pulsed(&relayer)
				} else {
					false
				}
			})
			.filter_map(|(_, id)| {
				let controller: T::AccountId = id.clone().into();
				let relayer =
					Self::bonded_controller(&controller).expect("BondedController must exist");
				let mut relayer_state =
					Self::relayer_state(&relayer).expect("RelayerState must exist");
				relayer_state.go_offline();
				<RelayerState<T>>::insert(&relayer, relayer_state);
				<T::ValidatorSet as ValidatorSetWithIdentification<T::AccountId>>::IdentificationOf::convert(
					id.clone()
				).map(|full_id| (id, full_id))
			})
			.collect::<Vec<IdentificationTuple<T>>>();

		// Remove all received heartbeats from the current session, they have already been processed
		// and won't be needed anymore.
		#[allow(deprecated)]
		ReceivedHeartbeats::<T>::remove_prefix(&session_index, None);

		if offenders.is_empty() {
			Self::deposit_event(Event::<T>::AllGood);
		} else {
			if Self::is_heartbeat_offence_active() {
				let validator_set_count = current_validators.len() as u32;
				let offence = UnresponsivenessOffence {
					session_index,
					validator_set_count,
					offenders: offenders.clone(),
					phantom: PhantomData,
				};
				if let Err(e) = T::ReportUnresponsiveness::report_offence(vec![], offence) {
					sp_runtime::print(e);
				}
			}
			Self::deposit_event(Event::<T>::SomeOffline { offline: offenders.clone() });
		}
	}

	fn handle_delayed_relayer_sets(now: RoundIndex) {
		let delayed_round = now - 1;
		let relayer_sets = <DelayedRelayerSets<T>>::take(delayed_round);
		relayer_sets.into_iter().for_each(|r| {
			Self::replace_bonded_relayer(&r.old, &r.new).expect("Replacement must success");
		});
	}
}

impl<T: Config> Pallet<T> {
	/// Verifies if the given account is a (candidate) relayer
	pub fn is_relayer(relayer: &T::AccountId) -> bool {
		if Self::relayer_state(relayer).is_some() {
			return true;
		}
		false
	}

	/// Verifies if the given account is a selected relayer for the current round or was selected at
	/// the beginning of the current round
	pub fn is_selected_relayer(relayer: &T::AccountId, is_initial: bool) -> bool {
		if is_initial {
			Self::initial_selected_relayers().contains(relayer)
		} else {
			Self::selected_relayers().contains(relayer)
		}
	}

	/// Verifies if the given account has already requested for relayer account update
	pub fn is_relayer_set_requested(relayer: T::AccountId) -> bool {
		let round = Self::round();
		let relayer_sets = Self::delayed_relayer_sets(round);
		relayer_sets.into_iter().any(|r| r.old == relayer)
	}

	/// Compute majority based on the current selected relayers
	fn compute_majority() -> u32 {
		((Self::selected_relayers().len() as u32) / 2) + 1
	}

	/// Verifies the existence of the given relayer and controller account. If it is both not bonded
	/// yet, it will return an `Ok`, if not an `Error` will be returned.
	fn verify_relayer_existence(
		relayer: &T::AccountId,
		controller: &T::AccountId,
	) -> Result<(), DispatchError> {
		ensure!(!Self::is_relayer(relayer), Error::<T>::RelayerAlreadyJoined);
		ensure!(!<BondedController<T>>::contains_key(controller), Error::<T>::RelayerAlreadyBonded);
		Ok(().into())
	}

	/// Sets the liveness of the requested relayer to `true`.
	pub fn pulse_heartbeat(relayer: &T::AccountId) -> bool {
		let session_index = T::ValidatorSet::session_index();
		if !Self::received_heartbeats(session_index, relayer) {
			<ReceivedHeartbeats<T>>::insert(session_index, relayer, true);
			return true;
		}
		false
	}

	/// Verifies whether the given relayer has sent a heartbeat in the current session. Returns
	/// `true` if the given relayer sent a heartbeat in the current session.
	pub fn is_heartbeat_pulsed(relayer: &T::AccountId) -> bool {
		let session_index = T::ValidatorSet::session_index();
		Self::received_heartbeats(session_index, relayer)
	}

	/// Remove the given `relayer` from the `SelectedRelayers`. Returns `true` if the relayer has
	/// been removed.
	fn remove_from_selected_relayers(relayer: &T::AccountId) -> bool {
		<SelectedRelayers<T>>::mutate(|selected_relayers| -> bool {
			selected_relayers.remove(relayer)
		})
	}

	/// Add the given `relayer` to the `SelectedRelayers`
	fn add_to_selected_relayers(relayer: T::AccountId) -> Result<bool, DispatchError> {
		<SelectedRelayers<T>>::try_mutate(|selected_relayers| -> Result<bool, DispatchError> {
			Ok(selected_relayers.try_insert(relayer).map_err(|_| <Error<T>>::TooManyRelayers)?)
		})
	}

	/// Adds a new relayer address update request. The state reflection will be applied in the next round.
	pub fn add_to_relayer_sets(old: T::AccountId, new: T::AccountId) -> DispatchResult {
		let round = Self::round();
		<DelayedRelayerSets<T>>::try_mutate(round, |relayer_sets| -> DispatchResult {
			Ok(relayer_sets
				.try_push(DelayedRelayerSet::new(old, new))
				.map_err(|_| <Error<T>>::TooManyDelayedRelayers)?)
		})
	}

	/// Remove the given `who` from the `DelayedRelayerSets` of the current round.
	pub fn remove_relayer_set(who: &T::AccountId) -> DispatchResult {
		let round = Self::round();
		<DelayedRelayerSets<T>>::mutate(round, |relayer_set| {
			relayer_set.retain(|r| r.old != *who);
		});
		Ok(())
	}

	/// Refresh the latest rounds cached selected relayers to the current state
	fn refresh_latest_cached_relayers() {
		let round = Self::round();
		<CachedSelectedRelayers<T>>::mutate(|cached_selected_relayers| {
			let selected_relayers = Self::selected_relayers();
			cached_selected_relayers
				.entry(round)
				.and_modify(|s| *s = selected_relayers.clone())
				.or_insert(selected_relayers);
		});
	}

	/// Refresh the latest rounds cached majority to the current state
	fn refresh_latest_cached_majority() {
		let round = Self::round();
		<CachedMajority<T>>::mutate(|cached_majority| {
			let majority = Self::majority();
			cached_majority.entry(round).and_modify(|m| *m = majority).or_insert(majority);
		});
	}

	/// Remove the given `acc` from the `RelayerPool`. The `is_relayer` parameter represents whether
	/// the given `acc` references to the relayer account or not. It it's not, it represents the
	/// bonded controller account. Returns `true` if the relayer has been removed.
	fn remove_from_relayer_pool(acc: &T::AccountId, is_relayer: bool) -> bool {
		let mut pool = Self::relayer_pool();
		let prev_len = pool.len();
		pool.retain(|r| if is_relayer { r.relayer != *acc } else { r.controller != *acc });
		let curr_len = pool.len();
		<RelayerPool<T>>::put(pool);
		curr_len < prev_len
	}

	/// Add the given `relayer` and `controller` pair to the `RelayerPool`
	fn add_to_relayer_pool(relayer: T::AccountId, controller: T::AccountId) -> DispatchResult {
		<RelayerPool<T>>::try_mutate(|pool| -> DispatchResult {
			Ok(pool
				.try_push(Relayer { relayer, controller })
				.map_err(|_| <Error<T>>::TooManyRelayers)?)
		})
	}

	/// Replace the `old` account that is used as a storage key for `SelectedRelayers` related
	/// values to the given `new` account.
	fn replace_selected_relayers(old: &T::AccountId, new: &T::AccountId) -> DispatchResult {
		if Self::remove_from_selected_relayers(old) {
			Self::add_to_selected_relayers(new.clone())?;
			Self::refresh_latest_cached_relayers();
			// replace pulsed heartbeats
			Self::replace_heartbeats(old, new);
		}
		Ok(())
	}

	/// Replace the `old` relayer account to the `new` relayer account from the `RelayerPool`
	fn replace_relayer_pool(
		old: &T::AccountId,
		new: &T::AccountId,
		controller: T::AccountId,
	) -> DispatchResult {
		if Self::remove_from_relayer_pool(old, true) {
			Self::add_to_relayer_pool(new.clone(), controller)?;
		}
		Ok(())
	}

	/// Replace the `ReceivedHeartbeats` mapped key from `old` to `new`
	fn replace_heartbeats(old: &T::AccountId, new: &T::AccountId) {
		let session_index = T::ValidatorSet::session_index();
		let is_pulsed = <ReceivedHeartbeats<T>>::take(session_index, old);
		<ReceivedHeartbeats<T>>::insert(session_index, new, is_pulsed);
	}

	/// Try to replace the bonded `old` relayer account to the given `new` relayer account. Returns
	/// `true` if the bonded relayer has been replaced.
	pub fn replace_bonded_relayer(
		old: &T::AccountId,
		new: &T::AccountId,
	) -> Result<bool, DispatchError> {
		if let Some(old_state) = <RelayerState<T>>::take(old) {
			let controller = old_state.clone().controller;
			// replace bonded controller
			<BondedController<T>>::insert(&controller, new);
			// replace relayer state
			<RelayerState<T>>::insert(new, old_state.clone());
			// replace relayer pool
			Self::replace_relayer_pool(&old, &new, controller.clone())?;
			// replace selected relayers
			Self::replace_selected_relayers(&old, &new)?;
			return Ok(true);
		}
		return Ok(false);
	}
}
