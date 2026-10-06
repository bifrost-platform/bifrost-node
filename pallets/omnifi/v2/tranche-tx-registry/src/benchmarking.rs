//! Benchmarks for `pallet-tranche-tx-registry-v2`.
//!
//! Ported from v1's benchmarks (see sibling `pallet-tranche-tx-registry`) with
//! one adaptation: `record_settlement_tx` gained `chunk_index`/`chunk_count`
//! params for chunked Collect/Response/Finalize legs (see
//! `docs/tranche-tx-registry/settlement-leg-chunking-design.md`) — this
//! benchmark exercises the non-leg `RequestsApproved` step, which requires
//! both to be `0`.
//!
//! Each `record_*` extrinsic is benchmarked at the heaviest step that opens an
//! entry (`Requested` / `WhitelistRequested` / a `receive()`), with the
//! trusted-recorder origin pre-seeded and the vault registered in
//! pallet-tranche-system via `T::BenchmarkHelper`. `record_settlement_tx`
//! carries the linear `n` (`request_ids`) its `WeightInfo` already declares.
//!
//! **security-review C1, fixed 2026-09-18**: the `SettleApplied`/`NavReceived`/
//! `SettleStarted`/`Settled` leg-completion close-cascade over
//! `SettlementRequests` (`close_active_requests`) now has its own worst-case
//! component, `close_cascade(n)`, benchmarked below by calling that helper
//! directly via `#[block]` rather than through an extrinsic (it isn't one).
//! `record_settlement_tx`'s dispatchable declares a pre-dispatch weight that
//! always budgets for `close_cascade(MAX_SETTLEMENT_REQUESTS)` on every step
//! that could possibly trigger the cascade (`SettleStarted`/`Settled`/the six
//! leg steps — never `RequestsApproved`/`Extended`), then reports the much
//! smaller *actual* cost via `PostDispatchInfo` once it knows whether the
//! cascade ran at all and how large `SettlementRequests` actually was.
//!
//! Also not yet covered: a chunked leg step's own cost
//! (`CollectBridgeExecuted`/.../`SettleApplied` with nonzero chunk info) —
//! this benchmark's `RequestsApproved` scenario calibrates the same flat
//! two-parameter `WeightInfo::record_settlement_tx` formula applied to every
//! step, same simplification v1 already made pre-chunking.
//!
//! `RequestStep::Extended`/`SettlementStep::Extended` are deliberately not
//! benchmarked — `RequestSubStepV2`/`SettlementSubStepV2` are uninhabited
//! (zero variants, see their doc comments), so `RequestExtraV2`/
//! `SettlementExtraV2::decode` can never succeed today; there is no valid
//! `extra` payload to benchmark a success path with, and the actual
//! decode-failure path is O(1) regardless of `extra`'s length, so
//! `record_request_tx()`/`record_settlement_tx(n)` correctly have no
//! `extra`-length weight term as things stand. Re-benchmark both once those
//! enums gain real variants — see the weight note on `RequestSubStepV2`'s doc
//! comment in `lib.rs`.

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::{
	BenchmarkHelper, OrderType, ReceiveKind, RequestOpening, RequestStep, SettlementStep,
	WhitelistStep, MAX_SETTLEMENT_REQUESTS,
};
use bp_tranche::MAX_TX_HASH_LEN;
use frame_benchmarking::v2::*;
use frame_support::{traits::EnsureOrigin, BoundedVec};
use frame_system::RawOrigin;
use pallet_tranche_system::{ChainAddress, VaultId};
use sp_core::{H160, H256, U256};
use sp_std::vec::Vec;

const PID: ProductId = 1;

/// Seeds the shared recorder identity — v2's `RecorderOrigin` is v1's
/// `pallet_tranche_tx_registry::EnsureTxRecorder` (single place to manage the
/// recorder address, see `runtime/dev/src/lib.rs`'s `Config` impl), so v2's own
/// `TxRecorder` storage is never consulted for authorization; `T::BenchmarkHelper`
/// seeds v1's instead.
fn recorder_origin<T: Config>() -> T::RuntimeOrigin {
	T::BenchmarkHelper::seed_recorder();
	T::RecorderOrigin::try_successful_origin().expect("RecorderOrigin benchmark helper")
}
fn h160(n: u64) -> ChainAddress {
	H160::from_low_u64_be(n).into()
}
fn h256(n: u64) -> H256 {
	H256::from_low_u64_be(n)
}

/// A worst-case (max-length) tx hash — the Solana-sized 64 bytes.
fn tx_hash(n: u64) -> TxHash {
	let mut bytes = [0xffu8; MAX_TX_HASH_LEN as usize];
	bytes[..8].copy_from_slice(&n.to_be_bytes());
	TxHash::truncate_from(bytes.to_vec())
}
fn vault(seed: u64) -> VaultId {
	VaultId { chain_id: 1, vault_address: h160(0x10_000 + seed) }
}

fn open_request<T: Config>(request_id: RequestId) {
	open_request_for::<T>(request_id, h160(0x1_2_3));
}

/// Same as `open_request`, but with a caller-chosen `investor` — used by
/// `close_cascade` below to seed one distinct investor per request, so each
/// investor's own `InvestorActiveRequests` list stays length-1 (the
/// benchmark is meant to calibrate `close_active_requests`' per-*entry* cost,
/// not additionally fold in `close_one_active_request`'s own O(that
/// investor's list length) position-scan cost by piling every request onto a
/// single investor).
fn open_request_for<T: Config>(request_id: RequestId, investor: ChainAddress) {
	let v = vault(request_id.to_low_u64_be());
	T::BenchmarkHelper::register_vault(PID, v.clone());
	Pallet::<T>::record_request_tx(
		recorder_origin::<T>(),
		PID,
		request_id,
		Some(RequestOpening {
			investor,
			vault: v,
			amount: U256::from(1_000_000u64),
			order_type: OrderType::Deposit,
		}),
		None,
		RequestStep::Requested,
		1u64,
		tx_hash(request_id.to_low_u64_be()),
		None,
		None,
	)
	.expect("open request");
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn set_tx_recorder() {
		let who: T::AccountId = whitelisted_caller();
		#[extrinsic_call]
		_(RawOrigin::Root, who.clone());
		assert_eq!(TxRecorder::<T>::get(), Some(who));
	}

	#[benchmark]
	fn record_request_tx() {
		let v = vault(1);
		T::BenchmarkHelper::register_vault(PID, v.clone());

		#[extrinsic_call]
		_(
			recorder_origin::<T>(),
			PID,
			h256(1),
			Some(RequestOpening {
				investor: h160(0x1_2_3),
				vault: v.clone(),
				amount: U256::from(1_000_000u64),
				order_type: OrderType::Deposit,
			}),
			None,
			RequestStep::Requested,
			1u64,
			tx_hash(0xa1),
			None,
			None,
		);

		assert!(RequestEntries::<T>::contains_key(PID, h256(1)));
	}

	#[benchmark]
	fn record_settlement_tx(n: Linear<1, { MAX_SETTLEMENT_REQUESTS }>) {
		let mut ids = Vec::new();
		for i in 0..n {
			let r = h256(1000 + i as u64);
			open_request::<T>(r);
			ids.push(r);
		}
		let request_ids = BoundedVec::try_from(ids).expect("MAX_SETTLEMENT_REQUESTS");

		#[extrinsic_call]
		_(
			recorder_origin::<T>(),
			PID,
			U256::from(9u64),
			None,
			None,
			None,
			Some(request_ids),
			SettlementStep::RequestsApproved,
			1u64,
			tx_hash(0x5e7),
			None,
			0u32,
			0u32,
			None,
		);

		assert_eq!(
			RequestEntries::<T>::get(PID, h256(1000)).and_then(|e| e.settlement_id),
			Some(U256::from(9u64))
		);
	}

	/// Worst-case per-entry cost of `close_active_requests`' leg-completion
	/// cascade — the scan `record_settlement_tx` runs (via
	/// `try_close_local_requests`/`close_active_requests` directly) whenever a
	/// `SettleStarted`/`Settled`/leg-step call happens to complete a
	/// settlement's Response leg locally or a chain's Finalize leg
	/// (security-review C1). Not itself an extrinsic — measured with
	/// `#[block]` around a direct call to the helper, same technique used
	/// wherever a weight component isn't a dispatchable's own top-level cost.
	/// One distinct investor per seeded request (see `open_request_for`'s doc
	/// comment) isolates `close_active_requests`' own per-entry cost from
	/// `close_one_active_request`'s separate, investor-list-length-dependent
	/// cost, which is out of scope here (that list is bounded only by how
	/// many products/requests one investor has open concurrently, not by
	/// anything this settlement-scoped cascade controls).
	#[benchmark]
	fn close_cascade(n: Linear<0, { MAX_SETTLEMENT_REQUESTS }>) {
		let settlement_id = U256::from(77u64);
		let mut ids = Vec::new();
		for i in 0..n {
			let r = h256(5_000 + i as u64);
			let investor = h160(0x9_0000 + i as u64);
			open_request_for::<T>(r, investor);
			ids.push(r);
		}
		if n > 0 {
			let request_ids = BoundedVec::try_from(ids).expect("MAX_SETTLEMENT_REQUESTS");
			Pallet::<T>::record_settlement_tx(
				recorder_origin::<T>(),
				PID,
				settlement_id,
				None,
				None,
				None,
				Some(request_ids),
				SettlementStep::RequestsApproved,
				1u64,
				tx_hash(0x5e8),
				None,
				0u32,
				0u32,
				None,
			)
			.expect("approve batch");
		}

		let tx = TxRecord {
			chain_id: 1u64,
			tx_hash: tx_hash(0x999),
			recorded_at: frame_system::Pallet::<T>::block_number(),
		};

		#[block]
		{
			Pallet::<T>::close_active_requests(PID, settlement_id, None, tx);
		}

		if n > 0 {
			assert!(InvestorActiveRequests::<T>::get(h160(0x9_0000)).is_empty());
		}
	}

	#[benchmark]
	fn record_receive_tx() {
		let v = vault(2);
		T::BenchmarkHelper::register_vault(PID, v.clone());

		#[extrinsic_call]
		_(
			recorder_origin::<T>(),
			PID,
			v.clone(),
			h160(0x1_2_3),
			h160(0x1_2_3),
			U256::from(500u64),
			ReceiveKind::Deposit,
			1u64,
			tx_hash(0x4ec),
		);

		assert!(ReceiveEntries::<T>::contains_key((h160(0x1_2_3), v, tx_hash(0x4ec))));
	}

	#[benchmark]
	fn record_whitelist_tx() {
		let v = vault(3);
		T::BenchmarkHelper::register_vault(PID, v.clone());

		#[extrinsic_call]
		_(
			recorder_origin::<T>(),
			v.clone(),
			h160(0x1_2_3),
			true,
			U256::from(1u64),
			WhitelistStep::WhitelistRequested,
			1u64,
			tx_hash(0x1157),
			None,
		);

		assert!(WhitelistEntries::<T>::contains_key((h160(0x1_2_3), v, U256::from(1u64))));
	}
}
