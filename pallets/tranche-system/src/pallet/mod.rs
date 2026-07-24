use crate::{AdapterKey, PermissionInspect, ProductDetails, ProductId, VaultId};

use frame_support::{pallet_prelude::*, traits::StorageVersion};

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::origin]
	#[derive(
		Clone,
		PartialEq,
		Eq,
		RuntimeDebug,
		Encode,
		Decode,
		DecodeWithMemTracking,
		TypeInfo,
		MaxEncodedLen,
	)]
	pub enum Origin {
		/// Dispatched by the tranche-system precompile on behalf of a Product Admin EOA.
		/// Used to ensure `create_product` can only be called through the precompile —
		/// mirrors pallet-pools' `Origin::PoolAdmin`.
		ProductAdmin,
	}

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Only accepted origin for `create_product`.
		/// Wire as `pallet_tranche_system::EnsureProductAdmin` in the runtime so that
		/// only the tranche-system precompile can invoke that extrinsic.
		type ProductAdminOrigin: frame_support::traits::EnsureOrigin<Self::RuntimeOrigin>;
		/// Permission inspector — implemented by pallet-tranche-permissions.
		/// Used to gate `create_product`, `set_tranche`, `set_adapter`, and
		/// `set_multichain_adapters`. Borrower identity is not gated here at
		/// all — it lives directly on each OffchainSource adapter (see
		/// `SourceType`), not as a permissions-pallet role.
		type Permissions: PermissionInspect<Self::AccountId>;
	}

	// -----------------------------------------------------------------------
	// Errors / Events / Extrinsics — next pass, alongside create_product,
	// set_tranche, set_adapter, set_multichain_adapters. This pass is storage
	// only, per the current design step.
	// -----------------------------------------------------------------------

	// -----------------------------------------------------------------------
	// Storage
	// -----------------------------------------------------------------------

	#[pallet::storage]
	#[pallet::unbounded]
	/// All active products, keyed by product ID.
	pub type Products<T: Config> =
		StorageMap<_, Blake2_128Concat, ProductId, ProductDetails<T::AccountId>>;

	#[pallet::storage]
	/// Reverse index: which product a tranche's vault (chain_id, vault_address)
	/// belongs to. Globally unique across all products — enforces that the same
	/// vault can't be registered to two different products, and lets other
	/// pallets (tranche-investments, tranche-permissions) resolve a vault to its
	/// product without the caller supplying `product_id` up front.
	/// Mirrors pallet-pools' `Tranches: TrancheId -> PoolId`.
	pub type Vaults<T: Config> = StorageMap<_, Blake2_128Concat, VaultId, ProductId>;

	#[pallet::storage]
	/// Reverse index: which product an individual Adapter (source_address, chain_id)
	/// belongs to. Globally unique across all products, same rationale as `Vaults`.
	/// Mirrors pallet-pools' `Collaterals: CollateralAsset -> PoolId`.
	pub type AdapterIndex<T: Config> = StorageMap<_, Blake2_128Concat, AdapterKey, ProductId>;

	#[pallet::storage]
	/// Reverse index: which product a MultichainAdapter (adapter_address, chain_id)
	/// belongs to. A separate namespace from `AdapterIndex` even though the key
	/// shape is identical — globally unique across all products, same rationale.
	pub type MultichainAdapterIndex<T: Config> =
		StorageMap<_, Blake2_128Concat, AdapterKey, ProductId>;

	// -----------------------------------------------------------------------
	// Extrinsics — next pass: create_product, set_tranche, set_adapter,
	// set_multichain_adapters
	// -----------------------------------------------------------------------
}
