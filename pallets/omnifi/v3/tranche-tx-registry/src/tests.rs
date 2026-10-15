use crate::{
	defaults::*, mock::*, pallet::pallet::*, slug, CloseOutcome, FlowId, InstanceKey,
	InstanceStatus, Lane, Part, ProductModel, SlotDef, Template, TemplateId, TrackDef, TxHash,
	MAX_ATTEMPTS, MAX_INSTANCE_LANES, MAX_LINKS,
};
use frame_support::{
	assert_noop, assert_ok,
	traits::{BeforeAllRuntimeMigrations, GetStorageVersion, OnRuntimeUpgrade, StorageVersion},
	BoundedVec,
};
use sp_core::H256;
use sp_runtime::{BuildStorage, DispatchError};

const PID: u64 = 0x0000_0004_0000_0001;
const HUB: u64 = 49088;
const A: u64 = 11155111;
const B: u64 = 84532;
const C: u64 = 421614;

// slot ids of the default templates (docs/tranche-tx-registry-v3/default-templates.md)
mod dep {
	pub const REQUESTED: u8 = 0;
	pub const BRIDGED: u8 = 1;
	pub const QUEUED: u8 = 2;
	pub const BATCH_ROUTED: u8 = 3;
	pub const APPROVED: u8 = 4;
	pub const SETTLE_APPLIED: u8 = 5;
}
mod red {
	pub const REQUESTED: u8 = 0;
	pub const QUEUED: u8 = 2;
	pub const CARRIED: u8 = 3;
	pub const HELD: u8 = 4;
	pub const APPROVED: u8 = 5;
	pub const SETTLE_APPLIED: u8 = 6;
}
mod set {
	pub const STARTED: u8 = 0;
	pub const ABORTED: u8 = 1;
	pub const NAV_REPORTED: u8 = 4;
	pub const RESPONSE_BRIDGED: u8 = 5;
	pub const NAV_RECEIVED: u8 = 7;
	pub const FINALIZE_STARTED: u8 = 8;
	pub const SETTLED: u8 = 9;
	pub const SETTLE_APPLIED: u8 = 11;
	pub const PAYOUT_ROUTE_RECEIVED: u8 = 13;
	pub const PAYOUT_CCTP_RECEIVED: u8 = 14;
	pub const HOLD_ROUTE_RECEIVED: u8 = 16;
	pub const HOLD_CCTP_RECEIVED: u8 = 17;
	pub const WITHDRAW_REQUESTED: u8 = 19;
	// tracks
	pub const COLLECT: u8 = 0;
	pub const FINALIZE: u8 = 1;
	pub const PAYOUT_MOVE: u8 = 2;
	pub const PAYOUT_MOVE_CCTP: u8 = 3;
	pub const HOLD: u8 = 4;
	pub const HOLD_CCTP: u8 = 5;
	pub const PULL: u8 = 6;
}

fn tx(n: u8) -> TxHash {
	BoundedVec::truncate_from(vec![n; 32])
}

fn key(n: u8) -> InstanceKey {
	H256::repeat_byte(n)
}

fn investor() -> H256 {
	H256::repeat_byte(0xaa)
}

fn vault() -> H256 {
	H256::repeat_byte(0xbb)
}

fn recorder() -> frame_system::pallet_prelude::OriginFor<Test> {
	RuntimeOrigin::signed(RECORDER)
}

/// `record_tx` with the common defaults filled in.
struct Rec {
	flow: FlowId,
	key: InstanceKey,
	lane: Option<u64>,
	slot: u8,
	chain: u64,
	tx: TxHash,
	completes: bool,
	part: Option<Part>,
	investor: Option<H256>,
	scope: Option<H256>,
	declare: Vec<(u8, u64)>,
	link: Option<(FlowId, InstanceKey)>,
}

fn rec(flow: FlowId, key: InstanceKey, slot: u8, chain: u64, tx: TxHash) -> Rec {
	Rec {
		flow,
		key,
		lane: None,
		slot,
		chain,
		tx,
		completes: true,
		part: None,
		investor: None,
		scope: None,
		declare: vec![],
		link: None,
	}
}

impl Rec {
	fn lane(mut self, chain: u64) -> Self {
		self.lane = Some(chain);
		self
	}
	fn failed(mut self) -> Self {
		self.completes = false;
		self
	}
	fn part(mut self, index: u16, count: u16) -> Self {
		self.part = Some((index, count));
		self
	}
	fn investor(mut self) -> Self {
		self.investor = Some(investor());
		self
	}
	fn scope(mut self, scope: H256) -> Self {
		self.scope = Some(scope);
		self
	}
	fn declare(mut self, lanes: Vec<(u8, u64)>) -> Self {
		self.declare = lanes;
		self
	}
	fn link(mut self, target: (FlowId, InstanceKey)) -> Self {
		self.link = Some(target);
		self
	}
	fn send(self) -> frame_support::dispatch::DispatchResult {
		Registry::record_tx(
			recorder(),
			PID,
			self.flow,
			self.key,
			self.lane,
			self.slot,
			self.chain,
			self.tx,
			self.completes,
			self.part,
			None,
			self.investor,
			None,
			self.scope,
			BoundedVec::truncate_from(self.declare),
			self.link,
		)
	}
}

fn batch(
	flow: FlowId,
	keys: Vec<InstanceKey>,
	slot: u8,
	chain: u64,
	t: TxHash,
	link: Option<(FlowId, InstanceKey)>,
) -> frame_support::dispatch::DispatchResult {
	Registry::record_tx_batch(
		recorder(),
		PID,
		flow,
		BoundedVec::truncate_from(keys),
		None,
		slot,
		chain,
		t,
		true,
		None,
		None,
		link,
	)
}

fn instance(flow: FlowId, k: InstanceKey) -> crate::Instance<u64> {
	Instances::<Test>::get((PID, flow, k)).expect("instance exists")
}

fn slot(flow: FlowId, k: InstanceKey, lane: Lane, id: u8) -> crate::SlotRecord<u64> {
	Slots::<Test>::get((PID, flow, k, lane, id))
}

fn sub(track: u8, chain: u64) -> Lane {
	Lane::Sub { track, chain }
}

fn active(flow: FlowId) -> (Vec<InstanceKey>, u32) {
	Registry::read_active(investor(), PID, flow, 0, 100)
}

fn assign_multichain() {
	assert_ok!(Registry::assign_default_flows(
		RuntimeOrigin::root(),
		PID,
		ProductModel::Multichain
	));
}

fn closed_events() -> Vec<(InstanceKey, CloseOutcome)> {
	System::events()
		.into_iter()
		.filter_map(|r| match r.event {
			RuntimeEvent::Registry(Event::InstanceClosed { instance_key, outcome, .. }) =>
				Some((instance_key, outcome)),
			_ => None,
		})
		.collect()
}

fn slot_def(id: u8, name: &str, required: bool) -> SlotDef {
	SlotDef { id, name: BoundedVec::truncate_from(name.as_bytes().to_vec()), required }
}

// ---------------------------------------------------------------------------
// templates / assignment
// ---------------------------------------------------------------------------

#[test]
fn genesis_seeds_default_templates_and_flows() {
	new_test_ext().execute_with(|| {
		for (id, _) in templates() {
			assert!(Templates::<Test>::contains_key(id));
		}
		let mc = DefaultFlows::<Test>::get(ProductModel::Multichain);
		assert_eq!(mc.get(&FLOW_DEPOSIT), Some(&MC_DEPOSIT_V1));
		assert_eq!(mc.get(&FLOW_RECEIVE), Some(&RECEIVE_V1));
		let sc = DefaultFlows::<Test>::get(ProductModel::SingleChain);
		assert_eq!(sc.get(&FLOW_SETTLEMENT), Some(&SC_SETTLEMENT_V1));

		// derived counts
		let deposit = Templates::<Test>::get(MC_DEPOSIT_V1).unwrap();
		assert_eq!(deposit.main_required_count, 4);
		assert!(deposit.tracks.is_empty());
		let redeem = Templates::<Test>::get(MC_REDEEM_V1).unwrap();
		assert_eq!(redeem.main_required_count, 4);
		assert!(redeem.tracks.is_empty());
		let settlement = Templates::<Test>::get(MC_SETTLEMENT_V1).unwrap();
		assert_eq!(settlement.main_required_count, 2);
		assert_eq!(
			settlement.tracks.iter().map(|t| t.required_count).collect::<Vec<_>>(),
			vec![2, 1, 1, 1, 1, 1, 1]
		);
		assert_eq!(
			settlement
				.tracks
				.iter()
				.map(|t| String::from_utf8(t.name.to_vec()).unwrap())
				.collect::<Vec<_>>(),
			vec![
				"collect-response",
				"finalize",
				"payout-move",
				"payout-move-cctp",
				"hold",
				"hold-cctp",
				"pull"
			]
		);
		let whitelist = Templates::<Test>::get(MC_WHITELIST_V1).unwrap();
		assert!(whitelist.tracks.is_empty());
		assert_eq!(whitelist.main_required_count, 2);
		let names = |id| {
			Templates::<Test>::get(id)
				.unwrap()
				.main
				.iter()
				.map(|s| (s.id, String::from_utf8(s.name.to_vec()).unwrap(), s.required))
				.collect::<Vec<_>>()
		};
		assert_eq!(
			names(SC_DEPOSIT_V1),
			vec![(0, "DepositRequested".into(), true), (1, "DepositsApproved".into(), true)]
		);
		assert_eq!(
			names(SC_REDEEM_V1),
			vec![
				(0, "RedeemRequested".into(), true),
				(1, "RedeemInitiated".into(), false),
				(2, "RedeemsApproved".into(), true)
			]
		);
		assert_eq!(names(SC_SETTLEMENT_V1), vec![(0, "Settled".into(), true)]);
	});
}

#[test]
fn assign_default_flows_fills_missing_only() {
	new_test_ext().execute_with(|| {
		let custom = TemplateId { name: slug("mc-deposit"), version: 9 };
		assert_ok!(Registry::register_template(RuntimeOrigin::root(), custom, mc_deposit()));
		assert_ok!(Registry::assign_flow(RuntimeOrigin::root(), PID, FLOW_DEPOSIT, Some(custom)));

		assert_eq!(Registry::do_assign_default_flows(PID, ProductModel::Multichain), 4);
		assert_eq!(ProductFlows::<Test>::get(PID, FLOW_DEPOSIT), Some(custom));
		assert_eq!(ProductFlows::<Test>::get(PID, FLOW_SETTLEMENT), Some(MC_SETTLEMENT_V1));
		assert_eq!(Registry::do_assign_default_flows(PID, ProductModel::Multichain), 0);
	});
}

#[test]
fn reassigned_template_only_affects_new_instances() {
	new_test_ext().execute_with(|| {
		assign_multichain();
		assert_ok!(rec(FLOW_RECEIVE, key(1), 0, A, tx(1)).investor().failed().send());

		// v2: an extra required step
		let v2 = TemplateId { name: slug("receive"), version: 2 };
		let mut t = receive();
		t.main.try_push(slot_def(1, "Confirmed", true)).unwrap();
		assert_ok!(Registry::register_template(RuntimeOrigin::root(), v2, t));
		assert_ok!(Registry::assign_flow(RuntimeOrigin::root(), PID, FLOW_RECEIVE, Some(v2)));

		// the open instance stays on v1 and completes on slot 0 alone
		assert_ok!(rec(FLOW_RECEIVE, key(1), 0, A, tx(2)).send());
		assert_eq!(instance(FLOW_RECEIVE, key(1)).template, RECEIVE_V1);
		assert_eq!(instance(FLOW_RECEIVE, key(1)).status, InstanceStatus::Completed);

		// a new one opens on v2 and needs both
		assert_ok!(rec(FLOW_RECEIVE, key(2), 0, A, tx(3)).investor().send());
		assert_eq!(instance(FLOW_RECEIVE, key(2)).template, v2);
		assert_eq!(instance(FLOW_RECEIVE, key(2)).status, InstanceStatus::Open);
		assert_ok!(rec(FLOW_RECEIVE, key(2), 1, A, tx(4)).send());
		assert_eq!(instance(FLOW_RECEIVE, key(2)).status, InstanceStatus::Completed);

		// unassigning: the existing instance still records, new keys can't open
		assert_ok!(Registry::assign_flow(RuntimeOrigin::root(), PID, FLOW_RECEIVE, None));
		assert_ok!(rec(FLOW_RECEIVE, key(2), 1, A, tx(5)).send());
		assert_noop!(
			rec(FLOW_RECEIVE, key(3), 0, A, tx(6)).investor().send(),
			Error::<Test>::FlowNotAssigned
		);
	});
}

#[test]
fn template_validation() {
	new_test_ext().execute_with(|| {
		let id = TemplateId { name: slug("x"), version: 1 };
		let mk = |main: Vec<SlotDef>, tracks: Vec<TrackDef>| Template {
			investor_scoped: false,
			main: BoundedVec::truncate_from(main),
			main_required_count: 0,
			tracks: BoundedVec::truncate_from(tracks),
		};
		let track = |slots: Vec<SlotDef>| TrackDef {
			name: BoundedVec::truncate_from(b"t".to_vec()),
			slots: BoundedVec::truncate_from(slots),
			required_count: 0,
		};
		let root = || RuntimeOrigin::root();

		assert_noop!(
			Registry::register_template(root(), id, mk(vec![slot_def(0, "a", false)], vec![])),
			Error::<Test>::NoRequiredMainSlot
		);
		assert_noop!(
			Registry::register_template(
				root(),
				id,
				mk(vec![slot_def(1, "a", true), slot_def(0, "b", true)], vec![])
			),
			Error::<Test>::SlotOrderNotAscending
		);
		assert_noop!(
			Registry::register_template(
				root(),
				id,
				mk(vec![slot_def(0, "a", true)], vec![track(vec![slot_def(0, "b", true)])])
			),
			Error::<Test>::DuplicateSlotId
		);
		assert_noop!(
			Registry::register_template(
				root(),
				id,
				mk(vec![slot_def(0, "a", true)], vec![track(vec![slot_def(1, "b", false)])])
			),
			Error::<Test>::TrackWithoutRequiredSlot
		);
		assert_noop!(
			Registry::register_template(root(), id, mk(vec![slot_def(0, "", true)], vec![])),
			Error::<Test>::EmptySlotName
		);
		assert_noop!(
			Registry::register_template(root(), MC_DEPOSIT_V1, mc_deposit()),
			Error::<Test>::TemplateExists
		);
		assert_noop!(
			Registry::register_template(RuntimeOrigin::signed(OTHER), id, mc_deposit()),
			DispatchError::BadOrigin
		);
		assert_noop!(
			Registry::assign_flow(root(), PID, FLOW_DEPOSIT, Some(id)),
			Error::<Test>::UnknownTemplate
		);
		assert_noop!(
			Registry::set_default_flow(root(), ProductModel::Multichain, FLOW_DEPOSIT, Some(id)),
			Error::<Test>::UnknownTemplate
		);
	});
}

/// The pallet added to a live chain by a runtime upgrade: FRAME first sets the
/// new pallet's on-chain storage version to the in-code one, then runs the
/// pallet's `on_runtime_upgrade` — which must still seed.
#[test]
fn upgrade_adding_the_pallet_seeds_once_and_never_resurrects_removed_defaults() {
	// no pallet genesis → no pallet keys at all, nothing seeded
	let storage = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	sp_io::TestExternalities::new(storage).execute_with(|| {
		assert_eq!(Registry::on_chain_storage_version(), StorageVersion::new(0));
		assert!(!Templates::<Test>::contains_key(MC_DEPOSIT_V1));

		let upgrade = || {
			<Registry as BeforeAllRuntimeMigrations>::before_all_runtime_migrations();
			<Registry as OnRuntimeUpgrade>::on_runtime_upgrade();
		};
		upgrade();
		assert_eq!(Registry::on_chain_storage_version(), StorageVersion::new(1));
		for (id, _) in templates() {
			assert!(Templates::<Test>::contains_key(id));
		}
		assert_eq!(DefaultFlows::<Test>::get(ProductModel::Multichain).len(), 5);
		assert_eq!(DefaultFlows::<Test>::get(ProductModel::SingleChain).len(), 5);

		// governance removes a default; a later upgrade must not bring it back
		assert_ok!(Registry::set_default_flow(
			RuntimeOrigin::root(),
			ProductModel::Multichain,
			FLOW_RECEIVE,
			None
		));
		upgrade();
		assert!(!DefaultFlows::<Test>::get(ProductModel::Multichain).contains_key(&FLOW_RECEIVE));
	});
}

// ---------------------------------------------------------------------------
// recording rules
// ---------------------------------------------------------------------------

#[test]
fn opening_rules() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			rec(FLOW_DEPOSIT, key(1), dep::REQUESTED, A, tx(1)).investor().send(),
			Error::<Test>::FlowNotAssigned
		);
		assign_multichain();
		// a track slot can't open an instance
		assert_noop!(
			rec(FLOW_SETTLEMENT, key(2), set::WITHDRAW_REQUESTED, A, tx(1)).lane(A).send(),
			Error::<Test>::InstanceNotOpened
		);
		assert_noop!(
			rec(FLOW_DEPOSIT, key(1), dep::REQUESTED, A, tx(1)).send(),
			Error::<Test>::MissingInvestor
		);
		// any main slot opens — order is the recorder's business
		assert_ok!(rec(FLOW_DEPOSIT, key(1), dep::QUEUED, HUB, tx(1)).investor().send());
		assert_ok!(rec(FLOW_SETTLEMENT, key(2), set::STARTED, HUB, tx(1)).send());
		assert_noop!(
			rec(FLOW_DEPOSIT, key(1), dep::REQUESTED, A, tx(2)).lane(A).send(),
			Error::<Test>::LaneMismatch
		);
		assert_noop!(
			rec(FLOW_SETTLEMENT, key(2), set::WITHDRAW_REQUESTED, A, tx(2)).send(),
			Error::<Test>::LaneMismatch
		);
		assert_noop!(rec(FLOW_DEPOSIT, key(1), 99, HUB, tx(2)).send(), Error::<Test>::UnknownSlot);
		assert_noop!(
			rec(FLOW_SETTLEMENT, key(2), set::WITHDRAW_REQUESTED, 0, tx(2)).lane(0).send(),
			Error::<Test>::InvalidLaneChain
		);
		assert_noop!(
			rec(FLOW_DEPOSIT, key(1), dep::REQUESTED, HUB, tx(2))
				.declare(vec![(0, B)])
				.send(),
			Error::<Test>::UnknownTrack
		);
		assert_noop!(
			rec(FLOW_SETTLEMENT, key(2), set::SETTLED, HUB, tx(2))
				.declare(vec![(7, B)])
				.send(),
			Error::<Test>::UnknownTrack
		);
		assert_noop!(
			rec(FLOW_DEPOSIT, key(1), dep::REQUESTED, HUB, TxHash::default()).send(),
			Error::<Test>::ZeroTxHash
		);
		assert_noop!(
			Registry::record_tx(
				RuntimeOrigin::signed(OTHER),
				PID,
				FLOW_DEPOSIT,
				key(1),
				None,
				0,
				HUB,
				tx(9),
				true,
				None,
				None,
				None,
				None,
				None,
				Default::default(),
				None
			),
			DispatchError::BadOrigin
		);
		assert_noop!(
			Registry::record_tx_batch(
				RuntimeOrigin::signed(OTHER),
				PID,
				FLOW_DEPOSIT,
				BoundedVec::truncate_from(vec![key(1)]),
				None,
				0,
				HUB,
				tx(9),
				true,
				None,
				None,
				None
			),
			DispatchError::BadOrigin
		);
		assert_noop!(
			Registry::close_instance(
				RuntimeOrigin::signed(OTHER),
				PID,
				FLOW_DEPOSIT,
				key(1),
				CloseOutcome::Failed
			),
			DispatchError::BadOrigin
		);
		// an investor given on a non-investor-scoped flow is ignored
		assert_ok!(rec(FLOW_SETTLEMENT, key(3), set::STARTED, HUB, tx(3)).investor().send());
		assert_eq!(instance(FLOW_SETTLEMENT, key(3)).investor, None);
	});
}

#[test]
fn reverted_bridge_attempts_and_duplicates() {
	new_test_ext().execute_with(|| {
		assign_multichain();
		let r = key(1);
		assert_ok!(rec(FLOW_DEPOSIT, r, dep::REQUESTED, A, tx(1)).investor().send());
		assert_noop!(
			rec(FLOW_DEPOSIT, r, dep::REQUESTED, A, tx(1)).send(),
			Error::<Test>::DuplicateAttestation
		);
		// same hash on another chain is a different tx
		assert_ok!(rec(FLOW_DEPOSIT, r, dep::REQUESTED, B, tx(1)).send());

		// a Reverted bridge tx is kept but doesn't complete the slot; the retry does
		assert_ok!(rec(FLOW_DEPOSIT, r, dep::BRIDGED, HUB, tx(2)).failed().send());
		assert!(!slot(FLOW_DEPOSIT, r, Lane::Main, dep::BRIDGED).completed);
		assert_ok!(rec(FLOW_DEPOSIT, r, dep::BRIDGED, HUB, tx(3)).send());
		let bridged = slot(FLOW_DEPOSIT, r, Lane::Main, dep::BRIDGED);
		assert!(bridged.completed);
		assert_eq!(bridged.attempts.len(), 2);
		assert!(!bridged.attempts[0].completes);
	});
}

#[test]
fn multi_part_slots_complete_on_every_distinct_index() {
	new_test_ext().execute_with(|| {
		assign_multichain();
		let s = key(7);
		let lane = sub(set::COLLECT, A);
		assert_ok!(rec(FLOW_SETTLEMENT, s, set::STARTED, HUB, tx(1)).declare(vec![(0, A)]).send());

		// out of order, a duplicate index, and a reverted chunk
		assert_ok!(rec(FLOW_SETTLEMENT, s, set::RESPONSE_BRIDGED, HUB, tx(2))
			.lane(A)
			.part(1, 3)
			.send());
		assert_ok!(rec(FLOW_SETTLEMENT, s, set::RESPONSE_BRIDGED, HUB, tx(3))
			.lane(A)
			.part(0, 3)
			.send());
		assert_ok!(rec(FLOW_SETTLEMENT, s, set::RESPONSE_BRIDGED, HUB, tx(4))
			.lane(A)
			.part(1, 3)
			.send());
		assert_ok!(rec(FLOW_SETTLEMENT, s, set::RESPONSE_BRIDGED, HUB, tx(5))
			.lane(A)
			.part(2, 3)
			.failed()
			.send());
		let record = slot(FLOW_SETTLEMENT, s, lane, set::RESPONSE_BRIDGED);
		assert!(!record.completed);
		assert_eq!((record.part_count, record.parts_done), (Some(3), 0b011));

		// malformed parts
		assert_noop!(
			rec(FLOW_SETTLEMENT, s, set::RESPONSE_BRIDGED, HUB, tx(6))
				.lane(A)
				.part(0, 2)
				.send(),
			Error::<Test>::PartCountMismatch
		);
		assert_noop!(
			rec(FLOW_SETTLEMENT, s, set::RESPONSE_BRIDGED, HUB, tx(6)).lane(A).send(),
			Error::<Test>::PartCountMismatch
		);
		assert_noop!(
			rec(FLOW_SETTLEMENT, s, set::RESPONSE_BRIDGED, HUB, tx(6))
				.lane(A)
				.part(3, 3)
				.send(),
			Error::<Test>::InvalidPart
		);
		assert_noop!(
			rec(FLOW_SETTLEMENT, s, set::RESPONSE_BRIDGED, HUB, tx(6))
				.lane(A)
				.part(0, 0)
				.send(),
			Error::<Test>::InvalidPart
		);
		assert_noop!(
			rec(FLOW_SETTLEMENT, s, set::RESPONSE_BRIDGED, HUB, tx(6))
				.lane(A)
				.part(0, 65)
				.send(),
			Error::<Test>::InvalidPart
		);
		// a single-part slot rejects a part once it has a plain attempt
		assert_ok!(rec(FLOW_SETTLEMENT, s, set::NAV_REPORTED, A, tx(7)).lane(A).send());
		assert_noop!(
			rec(FLOW_SETTLEMENT, s, set::NAV_REPORTED, A, tx(8)).lane(A).part(0, 1).send(),
			Error::<Test>::PartCountMismatch
		);

		// the retried last chunk completes the slot
		assert_ok!(rec(FLOW_SETTLEMENT, s, set::RESPONSE_BRIDGED, HUB, tx(9))
			.lane(A)
			.part(2, 3)
			.send());
		let record = slot(FLOW_SETTLEMENT, s, lane, set::RESPONSE_BRIDGED);
		assert!(record.completed);
		assert_eq!(record.attempts.len(), 5);
	});
}

#[test]
fn limits() {
	new_test_ext().execute_with(|| {
		assign_multichain();
		let r = key(1);
		assert_ok!(rec(FLOW_DEPOSIT, r, dep::REQUESTED, A, tx(1)).investor().send());
		for i in 1..MAX_ATTEMPTS {
			assert_ok!(rec(FLOW_DEPOSIT, r, dep::REQUESTED, A, tx(1 + i as u8)).send());
		}
		assert_noop!(
			rec(FLOW_DEPOSIT, r, dep::REQUESTED, A, tx(200)).send(),
			Error::<Test>::TooManyAttempts
		);

		for i in 0..MAX_LINKS {
			assert_ok!(rec(FLOW_DEPOSIT, r, dep::BRIDGED, HUB, tx(1 + i as u8))
				.link((FLOW_SETTLEMENT, key(100 + i as u8)))
				.send());
		}
		// the same link again is a no-op, a ninth one fails
		assert_ok!(rec(FLOW_DEPOSIT, r, dep::BRIDGED, HUB, tx(50))
			.link((FLOW_SETTLEMENT, key(100)))
			.send());
		assert_eq!(Registry::read_linked_from(PID, FLOW_SETTLEMENT, key(100), 0, 10).1, 1);
		assert_noop!(
			rec(FLOW_DEPOSIT, r, dep::BRIDGED, HUB, tx(51))
				.link((FLOW_SETTLEMENT, key(99)))
				.send(),
			Error::<Test>::TooManyLinks
		);

		let s = key(2);
		assert_ok!(rec(FLOW_SETTLEMENT, s, set::STARTED, HUB, tx(1)).send());
		for i in 0..MAX_INSTANCE_LANES {
			assert_ok!(rec(FLOW_SETTLEMENT, s, set::HOLD_ROUTE_RECEIVED, B, tx(60))
				.lane(1_000 + i as u64)
				.send());
		}
		assert_noop!(
			rec(FLOW_SETTLEMENT, s, set::HOLD_ROUTE_RECEIVED, B, tx(60)).lane(5_000).send(),
			Error::<Test>::TooManyLanes
		);
		// re-declaring existing lanes (and the attested one) counts each once
		assert_eq!(instance(FLOW_SETTLEMENT, s).lane_count, MAX_INSTANCE_LANES);
	});
}

// ---------------------------------------------------------------------------
// flows
// ---------------------------------------------------------------------------

/// B vault, 100 USDC: queued during a NAV round (placed at its close, settlement
/// 0x50), approved in the next settle round (0x51).
#[test]
fn deposit_links_its_placement_round_and_completes_on_settle_applied() {
	new_test_ext().execute_with(|| {
		assign_multichain();
		let r = key(1);
		let (placed, approved) = (key(0x50), key(0x51));
		assert_ok!(rec(FLOW_DEPOSIT, r, dep::REQUESTED, B, tx(1)).investor().scope(vault()).send());
		assert_eq!(active(FLOW_DEPOSIT), (vec![r], 1));
		assert_eq!(LatestByScope::<Test>::get((investor(), PID, FLOW_DEPOSIT, vault())), Some(r));
		assert_ok!(rec(FLOW_DEPOSIT, r, dep::BRIDGED, HUB, tx(2)).send());
		assert_ok!(rec(FLOW_DEPOSIT, r, dep::QUEUED, HUB, tx(3)).send());
		// the first round close after queueing routes the whole hold of B
		assert_ok!(rec(FLOW_DEPOSIT, r, dep::BATCH_ROUTED, HUB, tx(4))
			.link((FLOW_SETTLEMENT, placed))
			.send());
		let i = instance(FLOW_DEPOSIT, r);
		assert_eq!((i.status, i.main_stage, i.main_done), (InstanceStatus::Open, Some(3), 2));

		assert_ok!(rec(FLOW_DEPOSIT, r, dep::APPROVED, HUB, tx(5))
			.link((FLOW_SETTLEMENT, approved))
			.send());
		assert_eq!(instance(FLOW_DEPOSIT, r).status, InstanceStatus::Open);
		assert_ok!(rec(FLOW_DEPOSIT, r, dep::SETTLE_APPLIED, B, tx(6)).send());
		let i = instance(FLOW_DEPOSIT, r);
		assert_eq!(
			(i.status, i.closed_at, i.main_done, i.lane_count),
			(InstanceStatus::Completed, Some(1), 4, 0)
		);
		assert_eq!(i.links.to_vec(), vec![(FLOW_SETTLEMENT, placed), (FLOW_SETTLEMENT, approved)]);
		assert_eq!(
			Registry::read_linked_from(PID, FLOW_SETTLEMENT, placed, 0, 10),
			(vec![(FLOW_DEPOSIT, r)], 1)
		);
		assert_eq!(active(FLOW_DEPOSIT), (vec![], 0));
		assert_eq!(Registry::read_history(investor(), PID, FLOW_DEPOSIT, 0, 10), (vec![r], 1));
		assert_eq!(closed_events(), vec![(r, CloseOutcome::Completed)]);
	});
}

#[test]
fn deposit_batch_approval_links_dedups_and_is_atomic() {
	new_test_ext().execute_with(|| {
		assign_multichain();
		let s = key(0x50);
		let requests = [key(1), key(2), key(3)];
		for (i, r) in requests.iter().enumerate() {
			let n = 10 + 3 * i as u8;
			assert_ok!(rec(FLOW_DEPOSIT, *r, dep::REQUESTED, B, tx(n)).investor().send());
			assert_ok!(rec(FLOW_DEPOSIT, *r, dep::QUEUED, HUB, tx(n + 1)).send());
		}
		assert_eq!(active(FLOW_DEPOSIT).1, 3);

		// key(2) listed twice is attested once
		let keys = vec![key(1), key(2), key(2), key(3)];
		assert_ok!(batch(
			FLOW_DEPOSIT,
			keys,
			dep::APPROVED,
			HUB,
			tx(31),
			Some((FLOW_SETTLEMENT, s))
		));
		assert_eq!(slot(FLOW_DEPOSIT, key(2), Lane::Main, dep::APPROVED).attempts.len(), 1);
		assert_eq!(instance(FLOW_DEPOSIT, key(2)).links.to_vec(), vec![(FLOW_SETTLEMENT, s)]);
		let (linked, total) = Registry::read_linked_from(PID, FLOW_SETTLEMENT, s, 0, 10);
		assert_eq!(total, 3);
		assert_eq!(linked[0], (FLOW_DEPOSIT, key(3)));

		assert_ok!(batch(FLOW_DEPOSIT, requests.to_vec(), dep::SETTLE_APPLIED, B, tx(32), None));
		for r in requests {
			assert_eq!(instance(FLOW_DEPOSIT, r).status, InstanceStatus::Completed);
		}
		assert_eq!(active(FLOW_DEPOSIT), (vec![], 0));

		// atomic: one unopened key reverts the whole batch
		assert_noop!(
			batch(FLOW_DEPOSIT, vec![key(1), key(9)], dep::BRIDGED, HUB, tx(40), None),
			Error::<Test>::InstanceNotOpened
		);
	});
}

/// B vault JR redeem: carried in settle 1, held for coverage in 2, carried
/// again in 3, approved in 4 — one linked attempt per deferring round.
#[test]
fn redeem_links_every_deferring_round_and_completes_on_approval() {
	new_test_ext().execute_with(|| {
		assign_multichain();
		let r = key(1);
		let s = |n: u8| (FLOW_SETTLEMENT, key(0x50 + n));
		assert_ok!(rec(FLOW_REDEEM, r, red::REQUESTED, B, tx(1)).investor().send());
		assert_ok!(rec(FLOW_REDEEM, r, red::QUEUED, HUB, tx(2)).send());
		assert_ok!(rec(FLOW_REDEEM, r, red::CARRIED, HUB, tx(3)).link(s(1)).send());
		assert_ok!(rec(FLOW_REDEEM, r, red::HELD, HUB, tx(4)).link(s(2)).send());
		assert_ok!(rec(FLOW_REDEEM, r, red::CARRIED, HUB, tx(5)).link(s(3)).send());
		let i = instance(FLOW_REDEEM, r);
		assert_eq!((i.status, i.main_stage, i.main_done), (InstanceStatus::Open, Some(4), 2));
		assert_eq!(slot(FLOW_REDEEM, r, Lane::Main, red::CARRIED).attempts.len(), 2);

		assert_ok!(rec(FLOW_REDEEM, r, red::APPROVED, HUB, tx(6)).link(s(4)).send());
		assert_eq!(instance(FLOW_REDEEM, r).status, InstanceStatus::Open);
		assert_ok!(rec(FLOW_REDEEM, r, red::SETTLE_APPLIED, B, tx(7)).send());
		let i = instance(FLOW_REDEEM, r);
		assert_eq!((i.status, i.lane_count), (InstanceStatus::Completed, 0));
		assert_eq!(i.links.to_vec(), vec![s(1), s(2), s(3), s(4)]);
		assert_eq!(
			Registry::read_linked_from(PID, FLOW_SETTLEMENT, key(0x52), 0, 10),
			(vec![(FLOW_REDEEM, r)], 1)
		);
		assert_eq!(Registry::read_history(investor(), PID, FLOW_REDEEM, 0, 10).1, 1);
	});
}

#[test]
fn closed_instance_counters_stay_frozen() {
	new_test_ext().execute_with(|| {
		assign_multichain();
		let r = key(1);
		assert_ok!(rec(FLOW_DEPOSIT, r, dep::REQUESTED, B, tx(1)).investor().send());
		assert_ok!(rec(FLOW_DEPOSIT, r, dep::QUEUED, HUB, tx(2)).send());
		assert_ok!(Registry::close_instance(
			recorder(),
			PID,
			FLOW_DEPOSIT,
			r,
			CloseOutcome::Failed
		));
		assert_eq!(active(FLOW_DEPOSIT), (vec![], 0));
		assert_noop!(
			Registry::close_instance(recorder(), PID, FLOW_DEPOSIT, r, CloseOutcome::Completed),
			Error::<Test>::InstanceAlreadyClosed
		);
		let frozen = instance(FLOW_DEPOSIT, r);

		// late evidence on a main slot
		assert_ok!(rec(FLOW_DEPOSIT, r, dep::BATCH_ROUTED, HUB, tx(3)).send());
		let i = instance(FLOW_DEPOSIT, r);
		assert_eq!(i.status, InstanceStatus::Failed);
		assert_eq!((i.main_done, i.main_stage), (frozen.main_done, frozen.main_stage));
		assert!(slot(FLOW_DEPOSIT, r, Lane::Main, dep::BATCH_ROUTED).completed);
		assert_eq!(closed_events(), vec![(r, CloseOutcome::Failed)]);
		assert_eq!(Registry::read_history(investor(), PID, FLOW_DEPOSIT, 0, 10).1, 1);

		// late evidence on a closed settlement: a declared lane and a lane slot
		let s = key(2);
		assert_ok!(rec(FLOW_SETTLEMENT, s, set::STARTED, HUB, tx(4))
			.declare(vec![(set::COLLECT, A)])
			.send());
		assert_ok!(Registry::close_instance(
			recorder(),
			PID,
			FLOW_SETTLEMENT,
			s,
			CloseOutcome::Failed
		));
		assert_ok!(rec(FLOW_SETTLEMENT, s, set::SETTLED, HUB, tx(5))
			.declare(vec![(set::HOLD, B)])
			.send());
		assert_ok!(rec(FLOW_SETTLEMENT, s, set::HOLD_ROUTE_RECEIVED, B, tx(6)).lane(B).send());
		let i = instance(FLOW_SETTLEMENT, s);
		assert_eq!(
			(i.status, i.main_done, i.pending_lanes, i.lane_count),
			(InstanceStatus::Failed, 1, 1, 2)
		);
		assert!(slot(FLOW_SETTLEMENT, s, sub(set::HOLD, B), set::HOLD_ROUTE_RECEIVED).completed);
	});
}

/// A, B spokes; B gets a 2-chunk FINALIZE; the close routes a C → B payout
/// move, B's hold (burning to A and C) and a pull on A.
#[test]
fn settlement_waits_for_finalize_chunks_routes_and_cctp_arrivals() {
	new_test_ext().execute_with(|| {
		assign_multichain();
		let s = key(0x50);
		assert_ok!(rec(FLOW_SETTLEMENT, s, set::STARTED, HUB, tx(1))
			.declare(vec![(set::COLLECT, A), (set::COLLECT, B)])
			.send());
		for (chain, n) in [(A, 10u8), (B, 20u8)] {
			assert_ok!(rec(FLOW_SETTLEMENT, s, set::NAV_REPORTED, chain, tx(n)).lane(chain).send());
			assert_ok!(rec(FLOW_SETTLEMENT, s, set::NAV_RECEIVED, HUB, tx(n + 1))
				.lane(chain)
				.send());
		}
		assert_eq!(instance(FLOW_SETTLEMENT, s).pending_lanes, 0);

		// one continueFinalize sends both FINALIZE chunks — same tx, two parts
		assert_ok!(rec(FLOW_SETTLEMENT, s, set::FINALIZE_STARTED, HUB, tx(31))
			.lane(B)
			.part(0, 2)
			.declare(vec![(set::FINALIZE, B)])
			.send());
		assert_ok!(rec(FLOW_SETTLEMENT, s, set::FINALIZE_STARTED, HUB, tx(31))
			.lane(B)
			.part(1, 2)
			.send());
		assert_noop!(
			rec(FLOW_SETTLEMENT, s, set::FINALIZE_STARTED, HUB, tx(31))
				.lane(B)
				.part(1, 2)
				.send(),
			Error::<Test>::DuplicateAttestation
		);
		assert!(slot(FLOW_SETTLEMENT, s, sub(set::FINALIZE, B), set::FINALIZE_STARTED).completed);
		// the close tx: payout move C → B (PayoutMoveOrdered), B's hold batched,
		// A's adapter pulled (BatchRouted) — every ROUTE lane declared with Settled
		assert_ok!(rec(FLOW_SETTLEMENT, s, set::SETTLED, HUB, tx(31))
			.declare(vec![
				(set::FINALIZE, B),
				(set::PAYOUT_MOVE, C),
				(set::HOLD, B),
				(set::PULL, A),
			])
			.send());
		let i = instance(FLOW_SETTLEMENT, s);
		assert_eq!((i.status, i.main_stage, i.pending_lanes), (InstanceStatus::Open, Some(9), 4));

		assert_ok!(rec(FLOW_SETTLEMENT, s, set::SETTLE_APPLIED, B, tx(40))
			.lane(B)
			.part(1, 2)
			.send());
		assert_ok!(rec(FLOW_SETTLEMENT, s, set::SETTLE_APPLIED, B, tx(41))
			.lane(B)
			.part(0, 2)
			.send());
		let done = |lane: Lane| Lanes::<Test>::get((PID, FLOW_SETTLEMENT, s, lane)).unwrap().done;
		assert!(done(sub(set::FINALIZE, B)));
		assert_ok!(rec(FLOW_SETTLEMENT, s, set::WITHDRAW_REQUESTED, A, tx(42)).lane(A).send());
		assert!(done(sub(set::PULL, A)));

		// each ROUTE hook tx that burns CCTP declares its chain's `*-cctp` lane:
		// C's move burns once (→ B), B's hold burns twice (→ A, → C)
		assert_ok!(rec(FLOW_SETTLEMENT, s, set::PAYOUT_ROUTE_RECEIVED, C, tx(43))
			.lane(C)
			.declare(vec![(set::PAYOUT_MOVE_CCTP, C)])
			.send());
		assert_ok!(rec(FLOW_SETTLEMENT, s, set::HOLD_ROUTE_RECEIVED, B, tx(44))
			.lane(B)
			.declare(vec![(set::HOLD_CCTP, B)])
			.send());
		let i = instance(FLOW_SETTLEMENT, s);
		assert_eq!((i.status, i.pending_lanes, i.lane_count), (InstanceStatus::Open, 2, 8));

		// arrivals land on the destination chain but belong to the sender's lane
		assert_ok!(rec(FLOW_SETTLEMENT, s, set::PAYOUT_CCTP_RECEIVED, B, tx(50))
			.lane(C)
			.part(0, 1)
			.send());
		assert_ok!(rec(FLOW_SETTLEMENT, s, set::HOLD_CCTP_RECEIVED, A, tx(51))
			.lane(B)
			.part(0, 2)
			.send());
		assert_eq!(instance(FLOW_SETTLEMENT, s).status, InstanceStatus::Open);
		assert_ok!(rec(FLOW_SETTLEMENT, s, set::HOLD_CCTP_RECEIVED, C, tx(52))
			.lane(B)
			.part(1, 2)
			.send());
		assert_eq!(instance(FLOW_SETTLEMENT, s).status, InstanceStatus::Completed);
		assert_eq!(
			slot(FLOW_SETTLEMENT, s, sub(set::HOLD_CCTP, B), set::HOLD_CCTP_RECEIVED).attempts[1]
				.tx
				.chain_id,
			C
		);
	});
}

#[test]
fn settlement_restart_reuses_the_instance() {
	new_test_ext().execute_with(|| {
		assign_multichain();
		let s = key(0x50);
		assert_ok!(rec(FLOW_SETTLEMENT, s, set::STARTED, HUB, tx(1))
			.declare(vec![(set::COLLECT, A)])
			.send());
		assert_ok!(rec(FLOW_SETTLEMENT, s, set::ABORTED, HUB, tx(2)).send());
		// the same settlementId starts again — another attempt, lanes re-declared idempotently
		assert_ok!(rec(FLOW_SETTLEMENT, s, set::STARTED, HUB, tx(3))
			.declare(vec![(set::COLLECT, A)])
			.send());
		let i = instance(FLOW_SETTLEMENT, s);
		assert_eq!((i.status, i.lane_count, i.main_stage), (InstanceStatus::Open, 1, Some(1)));
		let started = slot(FLOW_SETTLEMENT, s, Lane::Main, set::STARTED);
		assert_eq!(started.attempts.len(), 2);
	});
}

#[test]
fn receive_opens_and_completes_in_one_call() {
	new_test_ext().execute_with(|| {
		assign_multichain();
		assert_ok!(rec(FLOW_RECEIVE, key(1), 0, B, tx(1)).investor().scope(vault()).send());
		assert_eq!(instance(FLOW_RECEIVE, key(1)).status, InstanceStatus::Completed);
		assert_eq!(active(FLOW_RECEIVE), (vec![], 0));
		assert_eq!(Registry::read_history(investor(), PID, FLOW_RECEIVE, 0, 10).1, 1);
		assert_eq!(closed_events(), vec![(key(1), CloseOutcome::Completed)]);
	});
}

#[test]
fn whitelist_latest_by_scope_tracks_the_newest_nonce() {
	new_test_ext().execute_with(|| {
		assign_multichain();
		let vault_a = H256::repeat_byte(0xa1);
		let vault_b = H256::repeat_byte(0xb2);
		let latest =
			|scope: H256| LatestByScope::<Test>::get((investor(), PID, FLOW_WHITELIST, scope));

		// whitelist templates are investor-scoped: the opener needs `who`
		assert_noop!(
			rec(FLOW_WHITELIST, key(1), 0, HUB, tx(1)).scope(vault_a).send(),
			Error::<Test>::MissingInvestor
		);
		assert_ok!(rec(FLOW_WHITELIST, key(1), 0, HUB, tx(1)).investor().scope(vault_a).send());
		assert_eq!(latest(vault_a), Some(key(1)));
		assert_eq!(latest(vault_b), None);

		// resending the scope on a later record is harmless
		assert_ok!(rec(FLOW_WHITELIST, key(1), 1, A, tx(2)).scope(vault_b).send());
		assert_ok!(rec(FLOW_WHITELIST, key(1), 2, A, tx(3)).send());
		assert_eq!(instance(FLOW_WHITELIST, key(1)).status, InstanceStatus::Completed);
		assert_eq!(latest(vault_b), None);

		// nonce 2 on vault A supersedes; vault B is tracked separately
		assert_ok!(rec(FLOW_WHITELIST, key(2), 0, HUB, tx(4)).investor().scope(vault_a).send());
		assert_ok!(rec(FLOW_WHITELIST, key(3), 0, HUB, tx(5)).investor().scope(vault_b).send());
		assert_eq!(latest(vault_a), Some(key(2)));
		assert_eq!(latest(vault_b), Some(key(3)));
		assert_eq!(active(FLOW_WHITELIST).1, 2);
		assert_eq!(
			Registry::read_history(investor(), PID, FLOW_WHITELIST, 0, 10),
			(vec![key(1)], 1)
		);

		// a scope on a non-investor-scoped flow is rejected
		assert_noop!(
			rec(FLOW_SETTLEMENT, key(9), 0, HUB, tx(9)).scope(vault_a).send(),
			Error::<Test>::ScopeWithoutInvestor
		);
	});
}

#[test]
fn active_index_pages_per_flow() {
	new_test_ext().execute_with(|| {
		assign_multichain();
		for n in 1..=5u8 {
			assert_ok!(rec(FLOW_DEPOSIT, key(n), dep::REQUESTED, B, tx(n)).investor().send());
		}
		assert_ok!(rec(FLOW_REDEEM, key(9), red::REQUESTED, B, tx(9)).investor().send());
		assert_eq!(active(FLOW_DEPOSIT).1, 5);
		assert_eq!(active(FLOW_REDEEM), (vec![key(9)], 1));

		let (first, total) = Registry::read_active(investor(), PID, FLOW_DEPOSIT, 0, 2);
		let (rest, _) = Registry::read_active(investor(), PID, FLOW_DEPOSIT, 2, 10);
		assert_eq!((first.len(), rest.len(), total), (2, 3, 5));

		assert_ok!(Registry::close_instance(
			recorder(),
			PID,
			FLOW_DEPOSIT,
			key(3),
			CloseOutcome::Failed
		));
		let (keys, total) = active(FLOW_DEPOSIT);
		assert_eq!(total, 4);
		assert!(!keys.contains(&key(3)));
		assert!(!InvestorActiveInstances::<Test>::contains_key((
			investor(),
			PID,
			FLOW_DEPOSIT,
			key(3)
		)));
		assert_eq!(InvestorActiveCount::<Test>::get((investor(), PID, FLOW_DEPOSIT)), 4);
	});
}

#[test]
fn single_chain_flows() {
	new_test_ext().execute_with(|| {
		assert_ok!(Registry::assign_default_flows(
			RuntimeOrigin::root(),
			PID,
			ProductModel::SingleChain
		));
		let (r, sid) = (key(1), key(0x50));
		// SYNC deposit: one tx carries the request, the approval and Settled
		assert_ok!(rec(FLOW_DEPOSIT, r, 0, A, tx(1)).investor().scope(vault()).send());
		assert_ok!(rec(FLOW_DEPOSIT, r, 1, A, tx(1)).link((FLOW_SETTLEMENT, sid)).send());
		assert_ok!(rec(FLOW_SETTLEMENT, sid, 0, A, tx(1)).send());
		assert_eq!(instance(FLOW_DEPOSIT, r).status, InstanceStatus::Completed);
		assert_eq!(instance(FLOW_SETTLEMENT, sid).status, InstanceStatus::Completed);

		// ASYNC redeem: initiated in one settle, approved in a later one
		let q = key(2);
		assert_ok!(rec(FLOW_REDEEM, q, 0, A, tx(2)).investor().send());
		assert_ok!(rec(FLOW_REDEEM, q, 1, A, tx(3)).send());
		assert_eq!(instance(FLOW_REDEEM, q).status, InstanceStatus::Open);
		assert_ok!(rec(FLOW_REDEEM, q, 2, A, tx(4)).send());
		assert_eq!(instance(FLOW_REDEEM, q).status, InstanceStatus::Completed);
	});
}
