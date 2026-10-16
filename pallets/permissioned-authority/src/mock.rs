use crate as pallet_permissioned_authority;

use bp_staking::{
	traits::{RelayManager, RelayerSetManager},
	RoundIndex, MAX_AUTHORITIES,
};
use frame_support::{
	construct_runtime, derive_impl,
	traits::{ConstU32, Contains},
	BoundedBTreeSet,
};
use sp_runtime::{BuildStorage, DispatchError};
use std::{
	cell::RefCell,
	collections::{BTreeMap, BTreeSet},
};

pub type AccountId = u64;

construct_runtime!(
	pub enum Test {
		System: frame_system,
		PermissionedAuthority: pallet_permissioned_authority,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = frame_system::mocking::MockBlock<Test>;
	type AccountId = AccountId;
	type Lookup = sp_runtime::traits::IdentityLookup<AccountId>;
}

thread_local! {
	/// Every `RelayManager` hook call, in order.
	pub static RELAY_CALLS: RefCell<Vec<RelayCall>> = RefCell::new(vec![]);
	/// controller -> relayer
	pub static BONDED: RefCell<BTreeMap<AccountId, AccountId>> = RefCell::new(BTreeMap::new());
	pub static SESSION_KEYS: RefCell<BTreeSet<AccountId>> = RefCell::new(BTreeSet::new());
	/// controller -> requested new relayer (relay-manager's `DelayedRelayerSets`)
	pub static PENDING_RELAYER_SETS: RefCell<BTreeMap<AccountId, AccountId>> = RefCell::new(BTreeMap::new());
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelayCall {
	Join(AccountId, AccountId),
	Leave(AccountId),
	RefreshRound(RoundIndex),
	HandleDelayed(RoundIndex),
	SelectRelayers(RoundIndex, Vec<AccountId>),
	RefreshMajority(RoundIndex),
	CollectHeartbeats,
}

fn record(call: RelayCall) {
	RELAY_CALLS.with(|c| c.borrow_mut().push(call));
}

pub fn relay_calls() -> Vec<RelayCall> {
	RELAY_CALLS.with(|c| c.borrow().clone())
}

pub fn clear_relay_calls() {
	RELAY_CALLS.with(|c| c.borrow_mut().clear());
}

pub fn bonded_relayer(controller: AccountId) -> Option<AccountId> {
	BONDED.with(|b| b.borrow().get(&controller).copied())
}

pub fn set_session_keys(who: AccountId) {
	SESSION_KEYS.with(|k| k.borrow_mut().insert(who));
}

pub fn pending_relayer_set(controller: AccountId) -> Option<AccountId> {
	PENDING_RELAYER_SETS.with(|p| p.borrow().get(&controller).copied())
}

pub struct MockRelayManager;
impl RelayManager<AccountId> for MockRelayManager {
	fn join_relayers(relayer: AccountId, controller: AccountId) -> Result<(), DispatchError> {
		let exists = BONDED.with(|b| {
			let b = b.borrow();
			b.contains_key(&controller) || b.values().any(|r| *r == relayer)
		});
		if exists {
			return Err(DispatchError::Other("RelayerAlreadyJoined"));
		}
		BONDED.with(|b| b.borrow_mut().insert(controller, relayer));
		record(RelayCall::Join(relayer, controller));
		Ok(())
	}
	fn refresh_round(now: RoundIndex) {
		record(RelayCall::RefreshRound(now));
	}
	fn refresh_relayer_pool() {}
	fn refresh_selected_relayers(round: RoundIndex, selected_candidates: Vec<AccountId>) {
		record(RelayCall::SelectRelayers(round, selected_candidates));
	}
	fn refresh_cached_selected_relayers(
		_round: RoundIndex,
		_relayers: BoundedBTreeSet<AccountId, ConstU32<MAX_AUTHORITIES>>,
	) {
	}
	fn refresh_majority(round: RoundIndex) {
		record(RelayCall::RefreshMajority(round));
	}
	fn replace_bonded_controller(_old: AccountId, _new: AccountId) {}
	fn leave_relayers(controller: &AccountId) {
		BONDED.with(|b| b.borrow_mut().remove(controller));
		record(RelayCall::Leave(*controller));
	}
	fn kickout_relayer(_controller: &AccountId) {}
	fn collect_heartbeats() {
		record(RelayCall::CollectHeartbeats);
	}
	fn handle_delayed_relayer_sets(now: RoundIndex) {
		// apply (and consume) every pending replacement, like relay-manager does
		let pending = PENDING_RELAYER_SETS.with(|p| std::mem::take(&mut *p.borrow_mut()));
		BONDED.with(|b| {
			let mut b = b.borrow_mut();
			for (controller, new) in pending {
				b.insert(controller, new);
			}
		});
		record(RelayCall::HandleDelayed(now));
	}
}

pub struct MockRelayerSets;
impl RelayerSetManager<AccountId> for MockRelayerSets {
	fn request_relayer_set(controller: &AccountId, new: AccountId) -> Result<(), DispatchError> {
		if bonded_relayer(*controller).is_none() {
			return Err(DispatchError::Other("ControllerDNE"));
		}
		if BONDED.with(|b| b.borrow().values().any(|r| *r == new)) {
			return Err(DispatchError::Other("RelayerAlreadyJoined"));
		}
		if pending_relayer_set(*controller).is_some() {
			return Err(DispatchError::Other("AlreadyRelayerSetRequested"));
		}
		PENDING_RELAYER_SETS.with(|p| p.borrow_mut().insert(*controller, new));
		Ok(())
	}
	fn cancel_relayer_set(controller: &AccountId) -> Result<(), DispatchError> {
		PENDING_RELAYER_SETS
			.with(|p| p.borrow_mut().remove(controller))
			.map(|_| ())
			.ok_or(DispatchError::Other("RelayerSetDNE"))
	}
	fn has_pending_relayer_sets() -> bool {
		PENDING_RELAYER_SETS.with(|p| !p.borrow().is_empty())
	}
}

pub struct MockSessionKeys;
impl Contains<AccountId> for MockSessionKeys {
	fn contains(who: &AccountId) -> bool {
		SESSION_KEYS.with(|k| k.borrow().contains(who))
	}
}

impl pallet_permissioned_authority::Config for Test {
	type RelayManager = MockRelayManager;
	type HasSessionKeys = MockSessionKeys;
	type RelayerSets = MockRelayerSets;
	type MaxAuthorities = ConstU32<4>;
	type NominalRoundLength = ConstU32<14_400>;
	type SessionLength = ConstU32<300>;
	type WeightInfo = ();
}

/// Validator `10` bonded to relayer `110`, with session keys.
pub fn new_test_ext() -> sp_io::TestExternalities {
	RELAY_CALLS.with(|c| c.borrow_mut().clear());
	BONDED.with(|b| b.borrow_mut().clear());
	SESSION_KEYS.with(|k| *k.borrow_mut() = BTreeSet::from([10]));
	PENDING_RELAYER_SETS.with(|p| p.borrow_mut().clear());

	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_permissioned_authority::GenesisConfig::<Test> { authorities: vec![(10, 110)] }
		.assimilate_storage(&mut t)
		.unwrap();
	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}
