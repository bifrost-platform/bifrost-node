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
//! NOT yet covered (security-review C1): the `SettleApplied`/`NavReceived`
//! close-cascade over `SettlementRequests` — that needs its own worst-case
//! component and is tracked as follow-up. Also not yet covered: a chunked leg
//! step's own cost (`CollectBridgeExecuted`/.../`SettleApplied` with nonzero
//! chunk info) — this benchmark's `RequestsApproved` scenario calibrates the
//! same flat two-parameter `WeightInfo::record_settlement_tx` formula applied
//! to every step, same simplification v1 already made pre-chunking.
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
use frame_benchmarking::v2::*;
use frame_support::{traits::EnsureOrigin, BoundedVec};
use frame_system::RawOrigin;
use pallet_tranche_system::VaultId;
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
fn h160(n: u64) -> H160 {
	H160::from_low_u64_be(n)
}
fn h256(n: u64) -> H256 {
	H256::from_low_u64_be(n)
}
fn vault(seed: u64) -> VaultId {
	VaultId { chain_id: 1, vault_address: h160(0x10_000 + seed) }
}

fn open_request<T: Config>(request_id: RequestId) {
	let v = vault(request_id.to_low_u64_be());
	T::BenchmarkHelper::register_vault(PID, v.clone());
	Pallet::<T>::record_request_tx(
		recorder_origin::<T>(),
		PID,
		request_id,
		Some(RequestOpening {
			investor: h160(0x1_2_3),
			vault: v,
			amount: U256::from(1_000_000u64),
			order_type: OrderType::Deposit,
		}),
		None,
		RequestStep::Requested,
		1u64,
		h256(request_id.to_low_u64_be()),
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
			h256(0xa1),
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
			h256(0x5e7),
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
			h256(0x4ec),
		);

		assert!(ReceiveEntries::<T>::contains_key((h160(0x1_2_3), v, h256(0x4ec))));
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
			h256(0x1157),
			None,
		);

		assert!(WhitelistEntries::<T>::contains_key((h160(0x1_2_3), v, U256::from(1u64))));
	}
}
