use crate::{
	BridgeAttempt, BridgeAttempts, BridgeStatus, ChainId, Config, OrderType, Pallet,
	RequestChainEntry, RequestEntry, RequestId, SettlementChainEntry, SettlementId, TxRecord,
	WhitelistEntry, WhitelistNonce,
};
use pallet_tranche_system::{ProductId, VaultId};

use frame_support::{
	migrations::VersionedMigration, pallet_prelude::*, storage_alias,
	traits::UncheckedOnRuntimeUpgrade, weights::Weight, Blake2_128Concat,
};
use frame_system::pallet_prelude::BlockNumberFor;
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::{H160, U256};
use sp_runtime::RuntimeDebug;
use sp_std::marker::PhantomData;

pub(crate) const LOG_TARGET: &str = "runtime::tranche-tx-registry";

// syntactic sugar for logging.
macro_rules! log {
	($level:tt, $patter:expr $(, $values:expr)* $(,)?) => {
		log::$level!(
			target: LOG_TARGET,
			$patter $(, $values)*
		)
	};
}

/// Converts a pre-`BridgeAttempts` single-evidence-slot `bridge_tx` field into the
/// new attempt-list shape — used by both `v1::MigrateV0ToV1` (whose target struct
/// now has `bridge_attempts` instead of `bridge_tx`, purely for this file to keep
/// compiling; that migration itself is dead code on any chain that's already past
/// v0) and `v2::MigrateV1ToV2` (the migration that actually matters, converting
/// genuine live v1 data). `Some(tx)` becomes a single `Executed` attempt — the
/// only outcome a `bridge_tx` slot could ever have held before `BridgeStatus`
/// existed (a rolled-back Bridge message was never recordable at all pre-v2); `None`
/// becomes an empty list, the exact same "no attempt observed yet" meaning it had
/// before.
pub(crate) fn attempts_from_tx<BlockNumber>(
	tx: Option<TxRecord<BlockNumber>>,
) -> BridgeAttempts<BlockNumber> {
	let mut attempts = BridgeAttempts::default();
	if let Some(tx) = tx {
		// Bounded at `MAX_BRIDGE_ATTEMPTS` (10) — pushing the sole pre-existing
		// attempt into a fresh, empty list can never fail.
		let _ = attempts.try_push(BridgeAttempt { status: BridgeStatus::Executed, tx });
	}
	attempts
}

/// v0 -> v1: `RequestEntry` gained two fields, `settlement_id: Option<SettlementId>` and
/// `approved_tx: Option<TxRecord<BlockNumber>>`, written by the new
/// `RequestStep::SettlementApproved` step — this pallet's own copy of the
/// request<->settlement linkage that used to be queried cross-pallet from
/// pallet-tranche-investments (via the now-removed `RequestSettlementInspect` trait; see
/// `SettlementRequests`' doc comment). Every pre-existing entry backfills both as `None` —
/// genuinely correct, not merely a placeholder: no entry written before this upgrade could
/// ever have recorded a `SettlementApproved` step (it didn't exist yet), so `None` is the
/// exact right value here, unlike the `recorded_at`/`timestamp` backfills elsewhere in this
/// pallet family, which stamp an approximate "as of this upgrade" value in place of a
/// genuinely unrecoverable original.
pub mod v1 {
	use super::*;

	/// `RequestEntry` as it existed under `STORAGE_VERSION::new(0)`, before
	/// `settlement_id`/`approved_tx` existed.
	#[derive(
		Clone,
		Encode,
		Decode,
		DecodeWithMemTracking,
		PartialEq,
		Eq,
		RuntimeDebug,
		TypeInfo,
		MaxEncodedLen,
	)]
	pub struct RequestEntryV0<BlockNumber> {
		pub product_id: ProductId,
		pub vault: VaultId,
		pub investor: H160,
		pub amount: U256,
		pub order_type: OrderType,
		pub request_tx: Option<TxRecord<BlockNumber>>,
		pub bridge_tx: Option<TxRecord<BlockNumber>>,
		pub queued_tx: Option<TxRecord<BlockNumber>>,
	}

	#[storage_alias]
	type RequestEntries<T: Config> = StorageDoubleMap<
		Pallet<T>,
		Blake2_128Concat,
		ProductId,
		Blake2_128Concat,
		RequestId,
		RequestEntryV0<BlockNumberFor<T>>,
	>;

	pub struct MigrateV0ToV1<T>(PhantomData<T>);

	impl<T: Config> UncheckedOnRuntimeUpgrade for MigrateV0ToV1<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();

			let entries = RequestEntries::<T>::drain().collect::<sp_std::vec::Vec<_>>();
			weight = weight.saturating_add(
				T::DbWeight::get().reads_writes(entries.len() as u64, entries.len() as u64),
			);
			let entries_count = entries.len();
			for (product_id, request_id, old) in entries {
				crate::RequestEntries::<T>::insert(
					product_id,
					request_id,
					RequestEntry {
						product_id: old.product_id,
						vault: old.vault,
						investor: old.investor,
						amount: old.amount,
						order_type: old.order_type,
						request_tx: old.request_tx,
						bridge_attempts: attempts_from_tx(old.bridge_tx),
						queued_tx: old.queued_tx,
						settlement_id: None,
						approved_tx: None,
						// `extension` didn't exist at v0 either — same "genuinely correct,
						// not a placeholder" reasoning as `v3::MigrateV2ToV3`'s own backfill
						// (see that migration's doc comment): no entry from this era could
						// have been opened under any `FlowVersion` other than `V1`, since
						// that axis didn't exist yet.
						extension: crate::RequestFlowExtension::V1,
					},
				);
			}
			weight = weight.saturating_add(T::DbWeight::get().writes(entries_count as u64));

			log!(
				info,
				"tranche-tx-registry v0->v1: backfilled settlement_id/approved_tx as None for {} RequestEntries entries ✅",
				entries_count,
			);

			weight
		}
	}

	/// Gated `on_chain == 0 && in_code == 1`, and bumps the on-chain version itself — wire
	/// this (not `MigrateV0ToV1` directly) into `Pallet::on_runtime_upgrade`.
	pub type MigrateToV1<T> = VersionedMigration<
		0,
		1,
		MigrateV0ToV1<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;
}

/// v1 -> v2: every Bridge-phase evidence field (`RequestEntry::bridge_tx`,
/// `RequestChainEntry::bridge_tx`, `SettlementChainEntry::{collect,response,
/// finalize}_bridge_tx`, `WhitelistEntry::bridge_tx`) changes shape from a single
/// `Option<TxRecord<BlockNumber>>` evidence slot to a `BridgeAttempts<BlockNumber>`
/// (bounded list of `{ status, tx }`), so a rolled-back (`Reverted`) Bridge message
/// can be recorded and retried without losing the original attempt's evidence — see
/// `BridgeAttempt`'s doc comment for the full rationale. Every pre-existing
/// `Some(tx)` becomes a single-element list with `status: BridgeStatus::Executed` —
/// the only outcome a `bridge_tx` slot could ever have held before this upgrade (a
/// rejected Bridge message was never recordable at all pre-v2, so there's no
/// existing data that could genuinely mean `Reverted`); `None` becomes an empty
/// list, the exact same "no attempt observed yet" meaning it had before.
pub mod v2 {
	use super::*;

	/// `RequestEntry` as it existed under `STORAGE_VERSION::new(1)`, before
	/// `bridge_tx` became `bridge_attempts`.
	#[derive(
		Clone,
		Encode,
		Decode,
		DecodeWithMemTracking,
		PartialEq,
		Eq,
		RuntimeDebug,
		TypeInfo,
		MaxEncodedLen,
	)]
	pub struct RequestEntryV1<BlockNumber> {
		pub product_id: ProductId,
		pub vault: VaultId,
		pub investor: H160,
		pub amount: U256,
		pub order_type: OrderType,
		pub request_tx: Option<TxRecord<BlockNumber>>,
		pub bridge_tx: Option<TxRecord<BlockNumber>>,
		pub queued_tx: Option<TxRecord<BlockNumber>>,
		pub settlement_id: Option<SettlementId>,
		pub approved_tx: Option<TxRecord<BlockNumber>>,
	}

	#[storage_alias]
	type RequestEntries<T: Config> = StorageDoubleMap<
		Pallet<T>,
		Blake2_128Concat,
		ProductId,
		Blake2_128Concat,
		RequestId,
		RequestEntryV1<BlockNumberFor<T>>,
	>;

	/// `RequestChainEntry` as it existed under `STORAGE_VERSION::new(1)`.
	#[derive(
		Clone,
		Encode,
		Decode,
		DecodeWithMemTracking,
		PartialEq,
		Eq,
		RuntimeDebug,
		TypeInfo,
		MaxEncodedLen,
		Default,
	)]
	pub struct RequestChainEntryV1<BlockNumber> {
		pub bridge_tx: Option<TxRecord<BlockNumber>>,
		pub applied_tx: Option<TxRecord<BlockNumber>>,
	}

	#[storage_alias]
	type RequestChainEntries<T: Config> = StorageNMap<
		Pallet<T>,
		(
			NMapKey<Blake2_128Concat, ProductId>,
			NMapKey<Blake2_128Concat, RequestId>,
			NMapKey<Blake2_128Concat, ChainId>,
		),
		RequestChainEntryV1<BlockNumberFor<T>>,
		ValueQuery,
	>;

	/// `SettlementChainEntry` as it existed under `STORAGE_VERSION::new(1)`.
	#[derive(
		Clone,
		Encode,
		Decode,
		DecodeWithMemTracking,
		PartialEq,
		Eq,
		RuntimeDebug,
		TypeInfo,
		MaxEncodedLen,
		Default,
	)]
	pub struct SettlementChainEntryV1<BlockNumber> {
		pub collect_bridge_tx: Option<TxRecord<BlockNumber>>,
		pub nav_reported_tx: Option<TxRecord<BlockNumber>>,
		pub response_bridge_tx: Option<TxRecord<BlockNumber>>,
		pub nav_received_tx: Option<TxRecord<BlockNumber>>,
		pub finalize_bridge_tx: Option<TxRecord<BlockNumber>>,
		pub settle_applied_tx: Option<TxRecord<BlockNumber>>,
	}

	#[storage_alias]
	type SettlementChainEntries<T: Config> = StorageNMap<
		Pallet<T>,
		(
			NMapKey<Blake2_128Concat, ProductId>,
			NMapKey<Blake2_128Concat, SettlementId>,
			NMapKey<Blake2_128Concat, ChainId>,
		),
		SettlementChainEntryV1<BlockNumberFor<T>>,
		ValueQuery,
	>;

	/// `WhitelistEntry` as it existed under `STORAGE_VERSION::new(1)`.
	#[derive(
		Clone,
		Encode,
		Decode,
		DecodeWithMemTracking,
		PartialEq,
		Eq,
		RuntimeDebug,
		TypeInfo,
		MaxEncodedLen,
	)]
	pub struct WhitelistEntryV1<BlockNumber> {
		pub product_id: ProductId,
		pub vault: VaultId,
		pub who: H160,
		pub grant: bool,
		pub request_tx: Option<TxRecord<BlockNumber>>,
		pub bridge_tx: Option<TxRecord<BlockNumber>>,
		pub applied_tx: Option<TxRecord<BlockNumber>>,
	}

	#[storage_alias]
	type WhitelistEntries<T: Config> = StorageNMap<
		Pallet<T>,
		(
			NMapKey<Blake2_128Concat, H160>,
			NMapKey<Blake2_128Concat, VaultId>,
			NMapKey<Blake2_128Concat, WhitelistNonce>,
		),
		WhitelistEntryV1<BlockNumberFor<T>>,
	>;

	pub struct MigrateV1ToV2<T>(PhantomData<T>);

	impl<T: Config> UncheckedOnRuntimeUpgrade for MigrateV1ToV2<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();

			let requests = RequestEntries::<T>::drain().collect::<sp_std::vec::Vec<_>>();
			let requests_count = requests.len();
			for (product_id, request_id, old) in requests {
				crate::RequestEntries::<T>::insert(
					product_id,
					request_id,
					RequestEntry {
						product_id: old.product_id,
						vault: old.vault,
						investor: old.investor,
						amount: old.amount,
						order_type: old.order_type,
						request_tx: old.request_tx,
						bridge_attempts: attempts_from_tx(old.bridge_tx),
						queued_tx: old.queued_tx,
						settlement_id: old.settlement_id,
						approved_tx: old.approved_tx,
						// Same reasoning as `v1::MigrateV0ToV1`'s own backfill above — the
						// `FlowVersion` axis didn't exist at v1 either.
						extension: crate::RequestFlowExtension::V1,
					},
				);
			}
			weight = weight.saturating_add(
				T::DbWeight::get().reads_writes(requests_count as u64, requests_count as u64),
			);
			weight = weight.saturating_add(T::DbWeight::get().writes(requests_count as u64));

			let request_chains = RequestChainEntries::<T>::drain().collect::<sp_std::vec::Vec<_>>();
			let request_chains_count = request_chains.len();
			for ((product_id, request_id, chain_id), old) in request_chains {
				crate::RequestChainEntries::<T>::insert(
					(product_id, request_id, chain_id),
					RequestChainEntry {
						bridge_attempts: attempts_from_tx(old.bridge_tx),
						applied_tx: old.applied_tx,
					},
				);
			}
			weight = weight.saturating_add(
				T::DbWeight::get()
					.reads_writes(request_chains_count as u64, request_chains_count as u64),
			);
			weight = weight.saturating_add(T::DbWeight::get().writes(request_chains_count as u64));

			let settlement_chains =
				SettlementChainEntries::<T>::drain().collect::<sp_std::vec::Vec<_>>();
			let settlement_chains_count = settlement_chains.len();
			for ((product_id, settlement_id, chain_id), old) in settlement_chains {
				crate::SettlementChainEntries::<T>::insert(
					(product_id, settlement_id, chain_id),
					SettlementChainEntry {
						collect_bridge_attempts: attempts_from_tx(old.collect_bridge_tx),
						nav_reported_tx: old.nav_reported_tx,
						response_bridge_attempts: attempts_from_tx(old.response_bridge_tx),
						nav_received_tx: old.nav_received_tx,
						finalize_bridge_attempts: attempts_from_tx(old.finalize_bridge_tx),
						settle_applied_tx: old.settle_applied_tx,
						// `extension` didn't exist at v1 either — same reasoning as
						// `v4::MigrateV3ToV4`'s own backfill.
						extension: crate::SettlementChainFlowExtension::V1,
					},
				);
			}
			weight = weight.saturating_add(
				T::DbWeight::get()
					.reads_writes(settlement_chains_count as u64, settlement_chains_count as u64),
			);
			weight =
				weight.saturating_add(T::DbWeight::get().writes(settlement_chains_count as u64));

			let whitelists = WhitelistEntries::<T>::drain().collect::<sp_std::vec::Vec<_>>();
			let whitelists_count = whitelists.len();
			for ((who, vault, nonce), old) in whitelists {
				crate::WhitelistEntries::<T>::insert(
					(who, vault, nonce),
					WhitelistEntry {
						product_id: old.product_id,
						vault: old.vault,
						who: old.who,
						grant: old.grant,
						request_tx: old.request_tx,
						bridge_attempts: attempts_from_tx(old.bridge_tx),
						applied_tx: old.applied_tx,
					},
				);
			}
			weight = weight.saturating_add(
				T::DbWeight::get().reads_writes(whitelists_count as u64, whitelists_count as u64),
			);
			weight = weight.saturating_add(T::DbWeight::get().writes(whitelists_count as u64));

			log!(
				info,
				"tranche-tx-registry v1->v2: converted bridge_tx -> bridge_attempts for {} RequestEntries, {} RequestChainEntries, {} SettlementChainEntries, {} WhitelistEntries entries ✅",
				requests_count,
				request_chains_count,
				settlement_chains_count,
				whitelists_count,
			);

			weight
		}
	}

	/// Gated `on_chain == 1 && in_code == 2`, and bumps the on-chain version itself —
	/// wire this (not `MigrateV1ToV2` directly) into `Pallet::on_runtime_upgrade`,
	/// chained after `v1::MigrateToV1` so a chain still at v0 runs both in the same
	/// upgrade (see `Pallet::on_runtime_upgrade`'s own doc comment).
	pub type MigrateToV2<T> = VersionedMigration<
		1,
		2,
		MigrateV1ToV2<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;
}

/// v2 -> v3: introduces this pallet's `FlowVersion`-scoped extension fields
/// (see `lib.rs`'s "Flow versioning" section) for both the request and
/// settlement pipelines at once — two backfills land in the same on-chain
/// bump because both are needed before those fields are usable at all. The
/// `FlowVersion` registry itself (`RequestFlowVersion`/`SettlementFlowVersion`)
/// lives in pallet-tranche-system, not here — see that pallet's own `v4`
/// migration for the corresponding backfill; this migration only concerns
/// `RequestEntry`/`SettlementChainEntry`'s own storage shape.
///
/// 1. `RequestEntry` gains `extension: RequestFlowExtension<BlockNumber>`. Every
///    pre-existing entry backfills `RequestFlowExtension::V1` — genuinely correct,
///    not a placeholder: no entry could have been opened under any other
///    `FlowVersion` before this upgrade, since no other version existed yet.
/// 2. `SettlementChainEntry` gains `extension: SettlementChainFlowExtension<BlockNumber>`,
///    backfilled with `SettlementChainFlowExtension::V1` for every pre-existing
///    entry — same reasoning as (1). (`SettlementFlowExtension`, the
///    settlement-*wide* counterpart, needs no such backfill — see its own doc
///    comment for why.)
pub mod v3 {
	use super::*;
	use crate::{RequestFlowExtension, SettlementChainFlowExtension};

	/// `RequestEntry` as it existed under `STORAGE_VERSION::new(2)`, before
	/// `extension` existed.
	#[derive(
		Clone,
		Encode,
		Decode,
		DecodeWithMemTracking,
		PartialEq,
		Eq,
		RuntimeDebug,
		TypeInfo,
		MaxEncodedLen,
	)]
	pub struct RequestEntryV2<BlockNumber> {
		pub product_id: ProductId,
		pub vault: VaultId,
		pub investor: H160,
		pub amount: U256,
		pub order_type: OrderType,
		pub request_tx: Option<TxRecord<BlockNumber>>,
		pub bridge_attempts: BridgeAttempts<BlockNumber>,
		pub queued_tx: Option<TxRecord<BlockNumber>>,
		pub settlement_id: Option<SettlementId>,
		pub approved_tx: Option<TxRecord<BlockNumber>>,
	}

	#[storage_alias]
	type RequestEntries<T: Config> = StorageDoubleMap<
		Pallet<T>,
		Blake2_128Concat,
		ProductId,
		Blake2_128Concat,
		RequestId,
		RequestEntryV2<BlockNumberFor<T>>,
	>;

	/// `SettlementChainEntry` as it existed under `STORAGE_VERSION::new(2)`,
	/// before `extension` existed.
	#[derive(
		Clone,
		Encode,
		Decode,
		DecodeWithMemTracking,
		PartialEq,
		Eq,
		RuntimeDebug,
		TypeInfo,
		MaxEncodedLen,
		Default,
	)]
	pub struct SettlementChainEntryV2<BlockNumber> {
		pub collect_bridge_attempts: BridgeAttempts<BlockNumber>,
		pub nav_reported_tx: Option<TxRecord<BlockNumber>>,
		pub response_bridge_attempts: BridgeAttempts<BlockNumber>,
		pub nav_received_tx: Option<TxRecord<BlockNumber>>,
		pub finalize_bridge_attempts: BridgeAttempts<BlockNumber>,
		pub settle_applied_tx: Option<TxRecord<BlockNumber>>,
	}

	#[storage_alias]
	type SettlementChainEntries<T: Config> = StorageNMap<
		Pallet<T>,
		(
			NMapKey<Blake2_128Concat, ProductId>,
			NMapKey<Blake2_128Concat, SettlementId>,
			NMapKey<Blake2_128Concat, ChainId>,
		),
		SettlementChainEntryV2<BlockNumberFor<T>>,
		ValueQuery,
	>;

	pub struct MigrateV2ToV3<T>(PhantomData<T>);

	impl<T: Config> UncheckedOnRuntimeUpgrade for MigrateV2ToV3<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();

			let requests = RequestEntries::<T>::drain().collect::<sp_std::vec::Vec<_>>();
			let requests_count = requests.len();
			for (product_id, request_id, old) in requests {
				crate::RequestEntries::<T>::insert(
					product_id,
					request_id,
					RequestEntry {
						product_id: old.product_id,
						vault: old.vault,
						investor: old.investor,
						amount: old.amount,
						order_type: old.order_type,
						request_tx: old.request_tx,
						bridge_attempts: old.bridge_attempts,
						queued_tx: old.queued_tx,
						settlement_id: old.settlement_id,
						approved_tx: old.approved_tx,
						extension: RequestFlowExtension::V1,
					},
				);
			}
			weight = weight.saturating_add(
				T::DbWeight::get().reads_writes(requests_count as u64, requests_count as u64),
			);
			weight = weight.saturating_add(T::DbWeight::get().writes(requests_count as u64));

			let settlement_chains =
				SettlementChainEntries::<T>::drain().collect::<sp_std::vec::Vec<_>>();
			let settlement_chains_count = settlement_chains.len();
			for ((product_id, settlement_id, chain_id), old) in settlement_chains {
				crate::SettlementChainEntries::<T>::insert(
					(product_id, settlement_id, chain_id),
					SettlementChainEntry {
						collect_bridge_attempts: old.collect_bridge_attempts,
						nav_reported_tx: old.nav_reported_tx,
						response_bridge_attempts: old.response_bridge_attempts,
						nav_received_tx: old.nav_received_tx,
						finalize_bridge_attempts: old.finalize_bridge_attempts,
						settle_applied_tx: old.settle_applied_tx,
						extension: SettlementChainFlowExtension::V1,
					},
				);
			}
			weight = weight.saturating_add(
				T::DbWeight::get()
					.reads_writes(settlement_chains_count as u64, settlement_chains_count as u64),
			);
			weight =
				weight.saturating_add(T::DbWeight::get().writes(settlement_chains_count as u64));

			log!(
				info,
				"tranche-tx-registry v2->v3: backfilled extension=V1 for {} RequestEntries and {} SettlementChainEntries entries ✅",
				requests_count,
				settlement_chains_count,
			);

			weight
		}
	}

	/// Gated `on_chain == 2 && in_code == 3`, and bumps the on-chain version itself —
	/// wire this (not `MigrateV2ToV3` directly) into `Pallet::on_runtime_upgrade`,
	/// chained after `v1::MigrateToV1`/`v2::MigrateToV2` so a chain still behind v2
	/// runs all three in the same upgrade (see `Pallet::on_runtime_upgrade`'s own
	/// doc comment).
	pub type MigrateToV3<T> = VersionedMigration<
		2,
		3,
		MigrateV2ToV3<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;
}
