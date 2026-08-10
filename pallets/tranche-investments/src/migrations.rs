use crate::{
	Allocation, ApprovedInvestment, Config, OrderType, Pallet, RequestId, RequestedInvestment,
	SettlementId, TrancheSettle, TrancheSettlement, MAX_ALLOCATIONS,
};
use pallet_tranche_system::{ProductId, VaultId};

use frame_support::{
	migrations::VersionedMigration, pallet_prelude::*, storage_alias,
	traits::UncheckedOnRuntimeUpgrade, weights::Weight, Blake2_128Concat,
};
use frame_system::pallet_prelude::BlockNumberFor;
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::{ConstU32, H160, U256};
use sp_runtime::{BoundedVec, RuntimeDebug};
use sp_std::marker::PhantomData;

pub(crate) const LOG_TARGET: &str = "runtime::tranche-investments";

// syntactic sugar for logging.
macro_rules! log {
	($level:tt, $patter:expr $(, $values:expr)* $(,)?) => {
		log::$level!(
			target: LOG_TARGET,
			$patter $(, $values)*
		)
	};
}

/// v1 -> v2: `RequestedInvestment`/`ApprovedInvestment`/`TrancheSettlement` each gained a
/// `recorded_at: BlockNumber` field (this chain's own block number when the entry was
/// written). For entries that already existed before this upgrade, the true original
/// write-time isn't recoverable from on-chain state, so this migration backfills
/// `recorded_at` with the migration's own block number instead — an explicit "as of this
/// upgrade" stamp, not the genuine historical record time. Every entry touched by one
/// migration run gets the exact same stamp.
pub mod v2 {
	use super::*;

	/// `RequestedInvestment` as it existed under `STORAGE_VERSION::new(1)`, before
	/// `recorded_at` existed.
	#[derive(
		Clone,
		Encode,
		Decode,
		DecodeWithMemTracking,
		PartialEq,
		RuntimeDebug,
		TypeInfo,
		MaxEncodedLen,
	)]
	pub struct RequestedInvestmentV1 {
		pub product_id: ProductId,
		pub settlement_id: SettlementId,
		pub vault: VaultId,
		pub investor_address: H160,
		pub amount: U256,
		pub order_type: OrderType,
	}

	/// `ApprovedInvestment` as it existed under `STORAGE_VERSION::new(1)`.
	#[derive(
		Clone,
		Encode,
		Decode,
		DecodeWithMemTracking,
		PartialEq,
		RuntimeDebug,
		TypeInfo,
		MaxEncodedLen,
	)]
	pub struct ApprovedInvestmentV1 {
		pub requested: RequestedInvestmentV1,
		pub settlement_id: SettlementId,
		pub allocations: BoundedVec<Allocation, ConstU32<MAX_ALLOCATIONS>>,
		pub receivable_amount: U256,
	}

	/// `TrancheSettlement` as it existed under `STORAGE_VERSION::new(1)`.
	#[derive(
		Clone,
		Encode,
		Decode,
		DecodeWithMemTracking,
		PartialEq,
		RuntimeDebug,
		TypeInfo,
		MaxEncodedLen,
	)]
	pub struct TrancheSettlementV1 {
		pub tranches: BoundedVec<TrancheSettle, ConstU32<{ pallet_tranche_system::MAX_TRANCHES }>>,
		pub pending_deposit_assets: U256,
	}

	#[storage_alias]
	type RequestedInvestments<T: Config> = StorageDoubleMap<
		Pallet<T>,
		Blake2_128Concat,
		ProductId,
		Blake2_128Concat,
		RequestId,
		RequestedInvestmentV1,
	>;

	#[storage_alias]
	type ApprovedInvestments<T: Config> = StorageDoubleMap<
		Pallet<T>,
		Blake2_128Concat,
		ProductId,
		Blake2_128Concat,
		RequestId,
		ApprovedInvestmentV1,
	>;

	#[storage_alias]
	type TrancheSettlements<T: Config> = StorageDoubleMap<
		Pallet<T>,
		Blake2_128Concat,
		ProductId,
		Blake2_128Concat,
		SettlementId,
		TrancheSettlementV1,
	>;

	pub struct MigrateV1ToV2<T>(PhantomData<T>);

	impl<T: Config> UncheckedOnRuntimeUpgrade for MigrateV1ToV2<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();
			let recorded_at: BlockNumberFor<T> = frame_system::Pallet::<T>::block_number();

			let requested = RequestedInvestments::<T>::drain().collect::<sp_std::vec::Vec<_>>();
			weight = weight.saturating_add(
				T::DbWeight::get().reads_writes(requested.len() as u64, requested.len() as u64),
			);
			let requested_count = requested.len();
			for (product_id, request_id, old) in requested {
				crate::RequestedInvestments::<T>::insert(
					product_id,
					request_id,
					RequestedInvestment {
						product_id: old.product_id,
						settlement_id: old.settlement_id,
						vault: old.vault,
						investor_address: old.investor_address,
						amount: old.amount,
						order_type: old.order_type,
						recorded_at,
					},
				);
			}
			weight = weight.saturating_add(T::DbWeight::get().writes(requested_count as u64));

			let approved = ApprovedInvestments::<T>::drain().collect::<sp_std::vec::Vec<_>>();
			weight = weight.saturating_add(
				T::DbWeight::get().reads_writes(approved.len() as u64, approved.len() as u64),
			);
			let approved_count = approved.len();
			for (product_id, request_id, old) in approved {
				crate::ApprovedInvestments::<T>::insert(
					product_id,
					request_id,
					ApprovedInvestment {
						requested: RequestedInvestment {
							product_id: old.requested.product_id,
							settlement_id: old.requested.settlement_id,
							vault: old.requested.vault,
							investor_address: old.requested.investor_address,
							amount: old.requested.amount,
							order_type: old.requested.order_type,
							recorded_at,
						},
						settlement_id: old.settlement_id,
						allocations: old.allocations,
						receivable_amount: old.receivable_amount,
						recorded_at,
					},
				);
			}
			weight = weight.saturating_add(T::DbWeight::get().writes(approved_count as u64));

			let settlements = TrancheSettlements::<T>::drain().collect::<sp_std::vec::Vec<_>>();
			weight = weight.saturating_add(
				T::DbWeight::get().reads_writes(settlements.len() as u64, settlements.len() as u64),
			);
			let settlements_count = settlements.len();
			for (product_id, settlement_id, old) in settlements {
				crate::TrancheSettlements::<T>::insert(
					product_id,
					settlement_id,
					TrancheSettlement {
						tranches: old.tranches,
						pending_deposit_assets: old.pending_deposit_assets,
						recorded_at,
					},
				);
			}
			weight = weight.saturating_add(T::DbWeight::get().writes(settlements_count as u64));

			log!(
				info,
				"tranche-investments v1->v2: backfilled recorded_at (stamped with migration block {:?}, not original write time) for {} RequestedInvestments, {} ApprovedInvestments, {} TrancheSettlements entries ✅",
				recorded_at,
				requested_count,
				approved_count,
				settlements_count,
			);

			weight
		}
	}

	/// Gated `on_chain == 1 && in_code == 2`, and bumps the on-chain version itself —
	/// wire this (not `MigrateV1ToV2` directly) into the runtime's migrations tuple.
	pub type MigrateToV2<T> = VersionedMigration<
		1,
		2,
		MigrateV1ToV2<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;
}
