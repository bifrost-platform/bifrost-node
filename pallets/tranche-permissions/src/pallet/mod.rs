use pallet_tranche_system::{ProductId, VaultId};

use frame_support::{pallet_prelude::*, traits::StorageVersion};

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	// -----------------------------------------------------------------------
	// Errors / Events / Extrinsics — next pass: grant_permission,
	// revoke_permission. This pass is storage only, per the current design
	// step.
	// -----------------------------------------------------------------------

	// -----------------------------------------------------------------------
	// Storage
	// -----------------------------------------------------------------------

	#[pallet::storage]
	/// The single ProductAdmin per product. Only writable by sudo — see
	/// pallet-tranche-system's `create_product`, which is only callable by
	/// whoever already holds this role for the `product_id` they supply.
	/// Mirrors pallet-pools' `PoolAdmins`.
	pub type ProductAdmins<T: Config> = StorageMap<_, Blake2_128Concat, ProductId, T::AccountId>;

	#[pallet::storage]
	/// OracleFeeders per product — many per product.
	/// O(1) lookup via `contains_key(product_id, who)`; iterate all feeders
	/// with `iter_prefix(product_id)`. Mirrors pallet-pools' `OracleFeeders`.
	pub type OracleFeeders<T: Config> =
		StorageDoubleMap<_, Blake2_128Concat, ProductId, Blake2_128Concat, T::AccountId, ()>;

	#[pallet::storage]
	/// Whitelisted investors per tranche — many per tranche.
	/// `VaultId` is globally unique (chain_id + vault_address, enforced by
	/// pallet-tranche-system), so no product key is needed here, same as old
	/// pools' `TrancheInvestors: TrancheId -> AccountId -> ()`.
	pub type TrancheInvestors<T: Config> =
		StorageDoubleMap<_, Blake2_128Concat, VaultId, Blake2_128Concat, T::AccountId, ()>;

	// -----------------------------------------------------------------------
	// Extrinsics — next pass: grant_permission, revoke_permission
	// -----------------------------------------------------------------------
}
