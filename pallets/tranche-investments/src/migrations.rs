use crate::{ApprovedInvestment, Config, Pallet, RequestedInvestment};
use pallet_tranche_system::ProductId;

use frame_support::{
	migrations::VersionedMigration, pallet_prelude::*, storage_alias,
	traits::UncheckedOnRuntimeUpgrade, weights::Weight, Blake2_128Concat,
};
use sp_core::{H256, U256};
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

/// v0 -> v1: `RequestId` (the second key of `RequestedInvestments`/`ApprovedInvestments`,
/// and the value of `get_pending_requests`' returned array) switched from `U256` to `H256`
/// — interface.sol's `request_id` is now `bytes32` rather than `uint256`, matching what the
/// Valuation Contract actually generates. This re-keys both maps entry-by-entry rather than
/// translating values in place, since the key's own type — not just the value — changed.
pub mod v1 {
	use super::*;

	/// Storage as it existed under `STORAGE_VERSION::new(0)`, keyed by the old `U256`
	/// `RequestId`. Same pallet/item names as the live storage, so this resolves to the
	/// same storage keys — only the declared `RequestId` type here differs.
	#[storage_alias]
	type RequestedInvestments<T: Config> = StorageDoubleMap<
		Pallet<T>,
		Blake2_128Concat,
		ProductId,
		Blake2_128Concat,
		U256,
		RequestedInvestment,
	>;

	#[storage_alias]
	type ApprovedInvestments<T: Config> = StorageDoubleMap<
		Pallet<T>,
		Blake2_128Concat,
		ProductId,
		Blake2_128Concat,
		U256,
		ApprovedInvestment,
	>;

	/// Reinterprets a `U256` request_id as `H256` by writing out its big-endian byte
	/// representation — the same bytes a Solidity `bytes32(uint256(request_id))` cast would
	/// produce, so a request_id that used to read as e.g. `5` keeps meaning
	/// `0x00..05` under the new encoding.
	fn to_h256(id: U256) -> H256 {
		H256(id.to_big_endian())
	}

	pub struct MigrateV0ToV1<T>(PhantomData<T>);

	impl<T: Config> UncheckedOnRuntimeUpgrade for MigrateV0ToV1<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();

			let requested = RequestedInvestments::<T>::drain().collect::<sp_std::vec::Vec<_>>();
			weight = weight.saturating_add(
				T::DbWeight::get().reads_writes(requested.len() as u64, requested.len() as u64),
			);
			let requested_count = requested.len();
			for (product_id, old_id, value) in requested {
				crate::RequestedInvestments::<T>::insert(product_id, to_h256(old_id), value);
			}
			weight = weight.saturating_add(T::DbWeight::get().writes(requested_count as u64));

			let approved = ApprovedInvestments::<T>::drain().collect::<sp_std::vec::Vec<_>>();
			weight = weight.saturating_add(
				T::DbWeight::get().reads_writes(approved.len() as u64, approved.len() as u64),
			);
			let approved_count = approved.len();
			for (product_id, old_id, value) in approved {
				crate::ApprovedInvestments::<T>::insert(product_id, to_h256(old_id), value);
			}
			weight = weight.saturating_add(T::DbWeight::get().writes(approved_count as u64));

			log!(
				info,
				"tranche-investments v0->v1: re-keyed {} RequestedInvestments and {} ApprovedInvestments entries (RequestId U256 -> H256) ✅",
				requested_count,
				approved_count,
			);

			weight
		}
	}

	/// Gated `on_chain == 0 && in_code == 1`, and bumps the on-chain version itself —
	/// wire this (not `MigrateV0ToV1` directly) into the runtime's migrations tuple.
	pub type MigrateToV1<T> = VersionedMigration<
		0,
		1,
		MigrateV0ToV1<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;
}
