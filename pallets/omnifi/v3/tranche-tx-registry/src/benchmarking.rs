//! Benchmarks for `pallet-tranche-tx-registry-v3`.
//!
//! Every benchmark runs against storage at its bounds:
//! - the instance's template is `MAX_TEMPLATE_SLOTS` max-name slots (largest `Templates` decode,
//!   longest slot scan);
//! - the instance carries `MAX_INSTANCE_METADATA` bytes and `MAX_LINKS - 1` links;
//! - every paged index the call appends to (investor history, reverse links) has a tail page one
//!   short of `HISTORY_PAGE_SIZE`;
//! - tx hashes are `MAX_TX_HASH_LEN` bytes and differ only in their last bytes, so the duplicate
//!   check compares them in full.
//!
//! `record_tx` scales on lane declarations `d` and metadata bytes `m` (attempt + instance
//! metadata, as `#[pallet::weight]` sums them) and is measured on its heaviest path: the call
//! completes the last required main slot — which already holds `MAX_ATTEMPTS - 1` max-size
//! attempts — adds the last link, re-declares `d` lanes that are already done, and so closes the
//! instance (active-index removal, history page write, reverse-link page write).
//! `record_tx_batch` is charged as `n × record_tx(0, m)`.

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::{
	defaults, history::HISTORY_PAGE_SIZE, slug, AttemptMetadata, BenchmarkHelper, ChainAddress,
	CloseOutcome, FlowId, HistoryPage, InstanceKey, InstanceMetadata, InstanceRef,
	LaneDeclarations, ProductModel, SlotDef, SlotId, Template, TemplateId, TrackDef, MAX_ATTEMPTS,
	MAX_ATTEMPT_METADATA, MAX_DEFAULT_FLOWS, MAX_INSTANCE_METADATA, MAX_LANE_DECLARATIONS,
	MAX_LINKS, MAX_MAIN_SLOTS, MAX_TEMPLATE_SLOTS, MAX_TRACKS, MAX_TRACK_SLOTS,
};
use frame_benchmarking::v2::*;
use frame_support::{traits::EnsureOrigin, BoundedVec};
use sp_core::H256;
use sp_std::{vec, vec::Vec};

const PID: ProductId = 1;
const BENCH_FLOW: FlowId = slug("bench-flow");
const INVESTOR: ChainAddress = H256::repeat_byte(0xaa);
/// The link the measured `record_tx` adds.
const LINK: InstanceRef = (defaults::FLOW_SETTLEMENT, H256::repeat_byte(0x09));

/// A max-length tx hash that differs from the others only in its last 4 bytes.
fn tx_hash(n: u32) -> TxHash {
	let len = crate::MAX_TX_HASH_LEN as usize;
	let mut bytes = vec![0xffu8; len];
	bytes[len - 4..].copy_from_slice(&n.to_be_bytes());
	TxHash::truncate_from(bytes)
}

fn gov<T: Config>() -> T::RuntimeOrigin {
	T::GovernanceOrigin::try_successful_origin().expect("GovernanceOrigin benchmark helper")
}

fn rec<T: Config>() -> T::RuntimeOrigin {
	T::BenchmarkHelper::seed_recorder();
	T::RecorderOrigin::try_successful_origin().expect("RecorderOrigin benchmark helper")
}

fn name(i: u32) -> crate::SlotName {
	BoundedVec::truncate_from(vec![b'a' + (i % 26) as u8; crate::MAX_SLOT_NAME_LEN as usize])
}

/// A template with `s` slots: `main` first, then full tracks. Every slot is
/// required, so `finalize_template` does the most work.
fn sized_template(s: u32) -> Template {
	let s = s.max(1).min(MAX_TEMPLATE_SLOTS);
	let main_n = s.min(MAX_MAIN_SLOTS);
	let main = (0..main_n).map(|i| SlotDef { id: i as u8, name: name(i), required: true });
	let mut tracks = Vec::new();
	let mut next = main_n;
	while next < s && tracks.len() < MAX_TRACKS as usize {
		let take = (s - next).min(MAX_TRACK_SLOTS);
		tracks.push(TrackDef {
			name: name(next),
			slots: BoundedVec::truncate_from(
				(next..next + take)
					.map(|i| SlotDef { id: i as u8, name: name(i), required: true })
					.collect(),
			),
			required_count: 0,
		});
		next += take;
	}
	Template {
		investor_scoped: true,
		main: BoundedVec::truncate_from(main.collect()),
		main_required_count: 0,
		tracks: BoundedVec::truncate_from(tracks),
	}
}

/// `main` slot that opens a bench instance.
const OPEN_SLOT: SlotId = 0;
/// Optional `main` slot the setup links / sets metadata through.
const SETUP_SLOT: SlotId = 1;
/// Last `main` slot — required, completes the instance.
const LAST_SLOT: SlotId = MAX_MAIN_SLOTS as SlotId - 1;
/// First slot of track 0 — required, completes a lane.
const LANE_SLOT: SlotId = MAX_MAIN_SLOTS as SlotId;

/// A full-size (`MAX_TEMPLATE_SLOTS`) template assigned to `(PID, BENCH_FLOW)`.
/// Required slots: `main` 0 and the last one; the first slot of every track.
fn bench_template<T: Config>() {
	let id = TemplateId { name: slug("bench-flow"), version: 1 };
	let slot = |i: u32, required: bool| SlotDef { id: i as u8, name: name(i), required };
	let main = (0..MAX_MAIN_SLOTS)
		.map(|i| slot(i, i == OPEN_SLOT as u32 || i == LAST_SLOT as u32))
		.collect();
	let tracks = (0..MAX_TRACKS)
		.map(|t| {
			let first = MAX_MAIN_SLOTS + t * MAX_TRACK_SLOTS;
			TrackDef {
				name: name(first),
				slots: BoundedVec::truncate_from(
					(first..first + MAX_TRACK_SLOTS).map(|i| slot(i, i == first)).collect(),
				),
				required_count: 0,
			}
		})
		.collect();
	let template = Template {
		investor_scoped: true,
		main: BoundedVec::truncate_from(main),
		main_required_count: 0,
		tracks: BoundedVec::truncate_from(tracks),
	};
	Pallet::<T>::do_register_template(id, template).expect("bench template");
	ProductFlows::<T>::insert(PID, BENCH_FLOW, id);
}

fn bench_record<T: Config>(
	key: InstanceKey,
	lane: Option<u64>,
	slot_id: SlotId,
	n: u32,
	completes: bool,
	metadata: Option<AttemptMetadata>,
	instance_metadata: Option<InstanceMetadata>,
	link: Option<InstanceRef>,
) {
	Pallet::<T>::do_record_tx(
		PID,
		BENCH_FLOW,
		key,
		lane,
		slot_id,
		1,
		tx_hash(n),
		completes,
		None,
		metadata,
		Some(INVESTOR),
		instance_metadata,
		Some(H256::repeat_byte(0xbb)),
		Default::default(),
		link,
	)
	.expect("bench record");
}

/// Fills `entries` with `HISTORY_PAGE_SIZE - 1` items so the next append
/// decodes and re-encodes a near-full tail page.
fn near_full_page<E: Clone>(entry: E) -> (u32, HistoryPage<E>) {
	let n = HISTORY_PAGE_SIZE - 1;
	(n, BoundedVec::truncate_from(vec![entry; n as usize]))
}

/// Opens `key` on the bench template with max instance metadata and
/// `MAX_LINKS - 1` links, and a near-full investor-history tail page.
fn open_heavy_instance<T: Config>(key: InstanceKey) {
	bench_record::<T>(key, None, OPEN_SLOT, 0, true, None, None, None);
	let instance_metadata =
		InstanceMetadata::truncate_from(vec![3u8; MAX_INSTANCE_METADATA as usize]);
	for i in 0..MAX_LINKS - 1 {
		bench_record::<T>(
			key,
			None,
			SETUP_SLOT,
			1 + i,
			true,
			None,
			Some(instance_metadata.clone()),
			Some((BENCH_FLOW, H256::from_low_u64_be(i as u64 + 1))),
		);
	}
	let (len, page) = near_full_page(H256::repeat_byte(0xcc));
	InvestorHistoryLen::<T>::insert((INVESTOR, PID, BENCH_FLOW), len);
	InvestorHistoryPage::<T>::insert((INVESTOR, PID, BENCH_FLOW, 0u32), page);
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn register_template(s: Linear<1, MAX_TEMPLATE_SLOTS>) {
		let id = TemplateId { name: slug("bench"), version: 1 };
		let template = sized_template(s);
		#[extrinsic_call]
		_(gov::<T>() as T::RuntimeOrigin, id, template);
		assert!(Templates::<T>::contains_key(id));
	}

	#[benchmark]
	fn set_default_flow() {
		defaults::seed::<T>();
		// one slot left — the call inserts into a map of `MAX_DEFAULT_FLOWS - 1`
		DefaultFlows::<T>::mutate(ProductModel::Multichain, |d| {
			let mut i = 0u8;
			while (d.len() as u32) < MAX_DEFAULT_FLOWS - 1 {
				let _ = d.try_insert(slug("fill").map(|b| b ^ i), defaults::RECEIVE_V1);
				i += 1;
			}
		});
		#[extrinsic_call]
		_(
			gov::<T>() as T::RuntimeOrigin,
			ProductModel::Multichain,
			slug("bench"),
			Some(defaults::RECEIVE_V1),
		);
		assert_eq!(
			DefaultFlows::<T>::get(ProductModel::Multichain).len() as u32,
			MAX_DEFAULT_FLOWS
		);
	}

	#[benchmark]
	fn assign_flow() {
		defaults::seed::<T>();
		#[extrinsic_call]
		_(
			gov::<T>() as T::RuntimeOrigin,
			PID,
			defaults::FLOW_DEPOSIT,
			Some(defaults::MC_DEPOSIT_V1),
		);
	}

	#[benchmark]
	fn assign_default_flows(n: Linear<0, MAX_DEFAULT_FLOWS>) {
		defaults::seed::<T>();
		DefaultFlows::<T>::mutate(ProductModel::Multichain, |d| {
			d.clear();
			for i in 0..n {
				let _ = d.try_insert(slug("f").map(|b| b ^ i as u8), defaults::RECEIVE_V1);
			}
		});
		#[extrinsic_call]
		_(gov::<T>() as T::RuntimeOrigin, PID, ProductModel::Multichain);
	}

	#[benchmark]
	fn record_tx(
		d: Linear<0, MAX_LANE_DECLARATIONS>,
		m: Linear<0, { MAX_ATTEMPT_METADATA + MAX_INSTANCE_METADATA }>,
	) {
		bench_template::<T>();
		let key = H256::repeat_byte(1);
		open_heavy_instance::<T>(key);
		// `d` lanes, each already done — re-declaring them reads every one
		for i in 0..d {
			bench_record::<T>(
				key,
				Some(1_000 + i as u64),
				LANE_SLOT,
				100 + i,
				true,
				None,
				None,
				None,
			);
		}
		// near-full slot to decode / scan / re-encode
		let full = AttemptMetadata::truncate_from(vec![7u8; MAX_ATTEMPT_METADATA as usize]);
		for i in 1..MAX_ATTEMPTS {
			bench_record::<T>(
				key,
				None,
				LAST_SLOT,
				1_000 + i,
				false,
				Some(full.clone()),
				None,
				None,
			);
		}
		// the target's reverse-link index has a near-full tail page
		let (len, page) = near_full_page((BENCH_FLOW, H256::repeat_byte(0xdd)));
		LinkedFromLen::<T>::insert((PID, LINK.0, LINK.1), len);
		LinkedFromPage::<T>::insert((PID, LINK.0, LINK.1, 0u32), page);

		let declare: LaneDeclarations =
			BoundedVec::truncate_from((0..d).map(|i| (0u8, 1_000 + i as u64)).collect());
		let attempt_len = m.min(MAX_ATTEMPT_METADATA);
		let metadata = AttemptMetadata::truncate_from(vec![9u8; attempt_len as usize]);
		let instance_metadata =
			InstanceMetadata::truncate_from(vec![4u8; (m - attempt_len) as usize]);
		let origin = rec::<T>();
		#[extrinsic_call]
		_(
			origin as T::RuntimeOrigin,
			PID,
			BENCH_FLOW,
			key,
			None,
			LAST_SLOT,
			1,
			tx_hash(u32::MAX),
			true,
			None,
			Some(metadata),
			None,
			Some(instance_metadata),
			None,
			declare,
			Some(LINK),
		);
		let instance = Instances::<T>::get((PID, BENCH_FLOW, key)).expect("instance");
		assert_eq!(instance.status, crate::InstanceStatus::Completed);
		assert_eq!(instance.links.len() as u32, MAX_LINKS);
		assert_eq!(InvestorHistoryLen::<T>::get((INVESTOR, PID, BENCH_FLOW)), HISTORY_PAGE_SIZE);
	}

	#[benchmark]
	fn close_instance() {
		bench_template::<T>();
		let key = H256::repeat_byte(1);
		open_heavy_instance::<T>(key);
		let origin = rec::<T>();
		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, PID, BENCH_FLOW, key, CloseOutcome::Failed);
		assert_eq!(InvestorHistoryLen::<T>::get((INVESTOR, PID, BENCH_FLOW)), HISTORY_PAGE_SIZE);
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
