use crate::{mock::*, ActiveAuthorities, Authorities, Error, Event, Round};

use frame_support::{assert_noop, assert_ok};
use pallet_session::SessionManager;
use sp_runtime::{DispatchError, Perbill};
use sp_staking::offence::{OffenceDetails, OnOffenceHandler};

type PA = PermissionedAuthority;

/// Rotate to session `index` at block `block`, returning the new validator set (if any).
fn rotate(index: u32, block: u64) -> Option<Vec<AccountId>> {
	System::set_block_number(block);
	<PA as SessionManager<AccountId>>::end_session(index - 1);
	<PA as SessionManager<AccountId>>::new_session(index)
}

#[test]
fn genesis_starts_round_one_with_relayers_selected() {
	new_test_ext().execute_with(|| {
		assert_eq!(Authorities::<Test>::get().into_inner(), vec![10]);
		assert_eq!(ActiveAuthorities::<Test>::get().into_inner(), vec![10]);
		let round = Round::<Test>::get();
		assert_eq!(round.current_round_index, 1);
		assert_eq!(round.round_length, 14_400);
		assert_eq!(round.session_length, 300);
		assert_eq!(bonded_relayer(10), Some(110));
		assert_eq!(
			relay_calls(),
			vec![
				RelayCall::Join(110, 10),
				RelayCall::RefreshRound(1),
				RelayCall::SelectRelayers(1, vec![10]),
				RelayCall::RefreshMajority(1),
			]
		);
	});
}

#[test]
fn genesis_sessions_return_active_set() {
	new_test_ext().execute_with(|| {
		assert_eq!(<PA as SessionManager<AccountId>>::new_session(0), Some(vec![10]));
		assert_eq!(<PA as SessionManager<AccountId>>::new_session(1), Some(vec![10]));
		assert_eq!(Round::<Test>::get().current_round_index, 1);
	});
}

#[test]
fn unchanged_set_never_starts_a_new_round() {
	new_test_ext().execute_with(|| {
		clear_relay_calls();
		for (i, session) in (2..20u32).enumerate() {
			assert_eq!(rotate(session, 300 * (i as u64 + 1)), None);
		}
		let round = Round::<Test>::get();
		assert_eq!(round.current_round_index, 1);
		// sessions still advance and heartbeats are still collected
		assert_eq!(round.current_session_index, 18);
		assert!(relay_calls().iter().all(|c| *c == RelayCall::CollectHeartbeats));
	});
}

#[test]
fn on_initialize_tracks_current_block() {
	new_test_ext().execute_with(|| {
		use frame_support::traits::Hooks;
		PA::on_initialize(42);
		assert_eq!(Round::<Test>::get().current_block, 42);
	});
}

#[test]
fn add_authority_is_root_only_and_needs_session_keys() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			PA::add_authority(RuntimeOrigin::signed(10), 20, 120),
			DispatchError::BadOrigin
		);
		assert_noop!(
			PA::add_authority(RuntimeOrigin::root(), 20, 120),
			Error::<Test>::SessionKeysNotSet
		);
		assert_noop!(
			PA::add_authority(RuntimeOrigin::root(), 10, 120),
			Error::<Test>::AlreadyAuthority
		);
		set_session_keys(20);
		// relayer already bonded to another validator
		assert!(PA::add_authority(RuntimeOrigin::root(), 20, 110).is_err());
		assert_eq!(Authorities::<Test>::get().into_inner(), vec![10]);
	});
}

#[test]
fn add_authority_applies_at_next_rotation_with_one_new_round() {
	new_test_ext().execute_with(|| {
		set_session_keys(20);
		assert_ok!(PA::add_authority(RuntimeOrigin::root(), 20, 120));
		System::assert_last_event(Event::AuthorityAdded { validator: 20, relayer: 120 }.into());
		// requested only — the active set and round are untouched until rotation
		assert_eq!(ActiveAuthorities::<Test>::get().into_inner(), vec![10]);
		assert_eq!(Round::<Test>::get().current_round_index, 1);
		assert_eq!(bonded_relayer(20), Some(120));

		clear_relay_calls();
		assert_eq!(rotate(2, 300), Some(vec![10, 20]));
		let round = Round::<Test>::get();
		assert_eq!(round.current_round_index, 2);
		assert_eq!(round.first_round_block, 300);
		assert_eq!(round.first_session_index, 1);
		assert_eq!(ActiveAuthorities::<Test>::get().into_inner(), vec![10, 20]);
		assert_eq!(
			relay_calls(),
			vec![
				RelayCall::CollectHeartbeats,
				RelayCall::RefreshRound(2),
				RelayCall::HandleDelayed(2),
				RelayCall::SelectRelayers(2, vec![10, 20]),
				RelayCall::RefreshMajority(2),
			]
		);
		System::assert_last_event(Event::NewRound { round: 2, validators: vec![10, 20] }.into());

		// applied once; the following rotations change nothing
		assert_eq!(rotate(3, 600), None);
		assert_eq!(Round::<Test>::get().current_round_index, 2);
	});
}

#[test]
fn cannot_remove_last_authority() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			PA::remove_authority(RuntimeOrigin::root(), 10),
			Error::<Test>::CannotRemoveLastAuthority
		);
		assert_noop!(PA::remove_authority(RuntimeOrigin::root(), 99), Error::<Test>::NotAuthority);
		assert_noop!(PA::remove_authority(RuntimeOrigin::signed(10), 10), DispatchError::BadOrigin);
	});
}

#[test]
fn removing_an_unapplied_authority_unbonds_immediately_and_starts_no_round() {
	new_test_ext().execute_with(|| {
		set_session_keys(20);
		assert_ok!(PA::add_authority(RuntimeOrigin::root(), 20, 120));
		assert_ok!(PA::remove_authority(RuntimeOrigin::root(), 20));
		assert_eq!(bonded_relayer(20), None);
		// requested == active again: nothing to apply
		assert_eq!(rotate(2, 300), None);
		assert_eq!(Round::<Test>::get().current_round_index, 1);
	});
}

#[test]
fn removing_an_active_authority_unbonds_at_rotation() {
	new_test_ext().execute_with(|| {
		set_session_keys(20);
		assert_ok!(PA::add_authority(RuntimeOrigin::root(), 20, 120));
		assert_eq!(rotate(2, 300), Some(vec![10, 20]));

		assert_ok!(PA::remove_authority(RuntimeOrigin::root(), 10));
		// still bonded and active until the rotation applies the removal
		assert_eq!(bonded_relayer(10), Some(110));
		assert_noop!(
			PA::add_authority(RuntimeOrigin::root(), 10, 111),
			Error::<Test>::PendingRemoval
		);

		clear_relay_calls();
		assert_eq!(rotate(3, 600), Some(vec![20]));
		assert_eq!(bonded_relayer(10), None);
		assert_eq!(Round::<Test>::get().current_round_index, 3);
		assert_eq!(
			relay_calls(),
			vec![
				RelayCall::CollectHeartbeats,
				RelayCall::RefreshRound(3),
				RelayCall::HandleDelayed(3),
				RelayCall::Leave(10),
				RelayCall::SelectRelayers(3, vec![20]),
				RelayCall::RefreshMajority(3),
			]
		);

		// re-adding after the removal is applied works
		assert_ok!(PA::add_authority(RuntimeOrigin::root(), 10, 111));
	});
}

#[test]
fn set_relayer_is_root_only_and_starts_one_new_round() {
	new_test_ext().execute_with(|| {
		assert_noop!(PA::set_relayer(RuntimeOrigin::signed(10), 10, 111), DispatchError::BadOrigin);
		assert_noop!(PA::set_relayer(RuntimeOrigin::root(), 99, 111), Error::<Test>::NotAuthority);

		assert_ok!(PA::set_relayer(RuntimeOrigin::root(), 10, 111));
		System::assert_last_event(
			Event::RelayerSetRequested { validator: 10, new_relayer: 111 }.into(),
		);
		assert_eq!(pending_relayer_set(10), Some(111));
		// only one pending replacement per validator
		assert!(PA::set_relayer(RuntimeOrigin::root(), 10, 112).is_err());

		// same validators, but the relayer set changes: new round
		assert_eq!(rotate(2, 300), Some(vec![10]));
		assert_eq!(Round::<Test>::get().current_round_index, 2);
		assert_eq!(bonded_relayer(10), Some(111));
		assert_eq!(pending_relayer_set(10), None);

		// consumed: no further round
		assert_eq!(rotate(3, 600), None);
		assert_eq!(Round::<Test>::get().current_round_index, 2);
	});
}

#[test]
fn cancelled_relayer_set_starts_no_round() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			PA::cancel_relayer_set(RuntimeOrigin::signed(10), 10),
			DispatchError::BadOrigin
		);
		assert!(PA::cancel_relayer_set(RuntimeOrigin::root(), 10).is_err());

		assert_ok!(PA::set_relayer(RuntimeOrigin::root(), 10, 111));
		assert_ok!(PA::cancel_relayer_set(RuntimeOrigin::root(), 10));
		System::assert_last_event(Event::RelayerSetCancelled { validator: 10 }.into());

		assert_eq!(rotate(2, 300), None);
		assert_eq!(Round::<Test>::get().current_round_index, 1);
		assert_eq!(bonded_relayer(10), Some(110));
	});
}

#[test]
fn removing_an_authority_drops_its_pending_relayer_set() {
	new_test_ext().execute_with(|| {
		// unapplied validator with a pending relayer replacement, then removed: the set is back
		// to the active one, so the stale request must not trigger a round-up on its own
		set_session_keys(20);
		assert_ok!(PA::add_authority(RuntimeOrigin::root(), 20, 120));
		assert_ok!(PA::set_relayer(RuntimeOrigin::root(), 20, 121));
		assert_ok!(PA::remove_authority(RuntimeOrigin::root(), 20));
		assert_eq!(pending_relayer_set(20), None);
		assert_eq!(bonded_relayer(20), None);
		assert_eq!(rotate(2, 300), None);
		assert_eq!(Round::<Test>::get().current_round_index, 1);
	});
}

#[test]
fn too_many_authorities() {
	new_test_ext().execute_with(|| {
		for v in [20, 30, 40] {
			set_session_keys(v);
			assert_ok!(PA::add_authority(RuntimeOrigin::root(), v, v + 100));
		}
		set_session_keys(50);
		assert_noop!(
			PA::add_authority(RuntimeOrigin::root(), 50, 150),
			Error::<Test>::TooManyAuthorities
		);
	});
}

#[test]
fn offences_are_reported_but_never_enforced() {
	new_test_ext().execute_with(|| {
		let offenders = vec![OffenceDetails { offender: (10u64, ()), reporters: vec![] }];
		<PA as OnOffenceHandler<AccountId, (AccountId, ()), _>>::on_offence(
			&offenders,
			&[Perbill::from_percent(10)],
			5,
		);
		System::assert_last_event(
			Event::OffenceReported {
				offender: 10,
				slash_fraction: Perbill::from_percent(10),
				session: 5,
			}
			.into(),
		);
		assert_eq!(ActiveAuthorities::<Test>::get().into_inner(), vec![10]);
		assert_eq!(Authorities::<Test>::get().into_inner(), vec![10]);
		assert_eq!(bonded_relayer(10), Some(110));
		assert_eq!(rotate(2, 300), None);
	});
}
