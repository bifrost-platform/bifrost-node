mod impls;

use crate::{migrations, Role, WeightInfo};
use pallet_tranche_system::{ChainAddress, ProductId, ProductInspect, VaultId, VaultInspect};

use frame_support::{
	pallet_prelude::*,
	traits::{Hooks, OnRuntimeUpgrade, StorageVersion},
};
use frame_system::pallet_prelude::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(2);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Vault inspector — implemented by pallet-tranche-system (it owns the
		/// `Vaults` reverse index). Used to verify a vault actually belongs to
		/// `product_id` before granting `Role::TrancheInvestor` for it.
		type Vaults: VaultInspect;
		/// Product inspector — implemented by pallet-tranche-system (it owns
		/// `Products`). Used to tell a `SingleChain` product apart from a
		/// `Multichain` one before granting/revoking `Role::TrancheInvestor` —
		/// see `Error::TrancheInvestorMultichainOnly`.
		type Products: ProductInspect;
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
		/// Benchmark-only setup hook — see [`crate::BenchmarkHelper`].
		#[cfg(feature = "runtime-benchmarks")]
		type BenchmarkHelper: crate::BenchmarkHelper;
	}

	// -----------------------------------------------------------------------
	// Errors
	// -----------------------------------------------------------------------

	#[pallet::error]
	pub enum Error<T> {
		/// Caller does not hold the ProductAdmin role for this product.
		NotProductAdmin,
		/// The role is already granted to this account.
		AlreadyGranted,
		/// The role is not currently granted to this account.
		NotGranted,
		/// The vault does not exist or does not belong to the given product.
		ProductOrVaultNotFound,
		/// `Role::TrancheInvestor` grants/revokes are `Multichain`-only — a
		/// `SingleChain` product's own TrancheManager Contract manages investor
		/// whitelisting directly, never through this pallet.
		TrancheInvestorMultichainOnly,
	}

	// -----------------------------------------------------------------------
	// Events
	// -----------------------------------------------------------------------

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A role was granted to an account for a product.
		PermissionGranted { product_id: ProductId, role: Role, who: T::AccountId },
		/// A role was revoked from an account for a product.
		PermissionRevoked { product_id: ProductId, role: Role, who: T::AccountId },
		/// `investor` was whitelisted for `vault` of `product_id`.
		TrancheInvestorGranted { product_id: ProductId, vault: VaultId, investor: ChainAddress },
		/// `investor` was removed from `vault`'s whitelist of `product_id`.
		TrancheInvestorRevoked { product_id: ProductId, vault: VaultId, investor: ChainAddress },
	}

	// -----------------------------------------------------------------------
	// Storage
	// -----------------------------------------------------------------------

	#[pallet::storage]
	/// The single ProductAdmin per product. Only writable by sudo — see
	/// pallet-tranche-system's `create_product`, which is only callable by
	/// whoever already holds this role for the `product_id` they supply.
	pub type ProductAdmins<T: Config> = StorageMap<_, Blake2_128Concat, ProductId, T::AccountId>;

	#[pallet::storage]
	/// OracleFeeders per product — many per product.
	/// O(1) lookup via `contains_key(product_id, who)`; iterate all feeders
	/// with `iter_prefix(product_id)`.
	pub type OracleFeeders<T: Config> =
		StorageDoubleMap<_, Blake2_128Concat, ProductId, Blake2_128Concat, T::AccountId, ()>;

	#[pallet::storage]
	/// Whitelisted investors per `(product_id, vault)` — many per tranche.
	/// Keyed by `product_id` in addition to `VaultId` (2026-08-26, `v1`) even
	/// though `VaultId` is otherwise globally unique (chain_id +
	/// vault_address, enforced by pallet-tranche-system) — that uniqueness
	/// only holds while the vault is *currently registered*.
	/// `pallet_tranche_system::set_tranche(Remove)` frees a `VaultId` for a
	/// *different* product to register later (`Vaults::remove` there has no
	/// memory of who used to own it), so keying on `VaultId` alone would let
	/// a removed tranche's stale investor whitelist silently resurrect itself
	/// for whatever new product reuses that same `(chain_id, vault_address)`.
	/// Look up with `contains_key((product_id, vault, investor))`; enumerate a
	/// tranche's investors with `iter_prefix((product_id, vault))`.
	///
	/// The investor is a `ChainAddress` on `vault.chain_id` (EVM addresses
	/// left-padded), not a Hub account — investors may live on non-EVM chains.
	pub type TrancheInvestors<T: Config> = StorageNMap<
		_,
		(
			NMapKey<Blake2_128Concat, ProductId>,
			NMapKey<Blake2_128Concat, VaultId>,
			NMapKey<Blake2_128Concat, ChainAddress>,
		),
		(),
	>;

	// -----------------------------------------------------------------------
	// Hooks
	// -----------------------------------------------------------------------

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_runtime_upgrade() -> Weight {
			// Only the latest migration is kept — every live chain is already at v1.
			migrations::v2::MigrateToV2::<T>::on_runtime_upgrade()
		}
	}

	// -----------------------------------------------------------------------
	// Extrinsics
	// -----------------------------------------------------------------------

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Grant `role` to `who` for `product_id`.
		///
		/// Authorization:
		/// - `Role::ProductAdmin` — caller must be sudo (root). The tranche-permissions precompile
		///   always dispatches as a signed origin, so a `grant_permission`/`revoke_permission` call
		///   routed through it with `role == ProductAdmin` reverts here naturally — no
		///   special-casing needed at the precompile boundary.
		/// - `Role::OracleFeeder` — caller must hold `ProductAdmin` for the given product.
		///
		/// Tranche investors are granted with `grant_tranche_investor`.
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::grant_permission())]
		pub fn grant_permission(
			origin: OriginFor<T>,
			product_id: ProductId,
			role: Role,
			who: T::AccountId,
		) -> DispatchResult {
			Self::ensure_role_authorized(origin, product_id, &role)?;

			// ProductAdmin is 1:1: fail if the slot is already occupied by
			// anyone. OracleFeeder is 1:many, so check the specific account instead.
			let already_granted = match &role {
				Role::OracleFeeder => Self::has_role(product_id, &who, &role),
				Role::ProductAdmin => Self::role_occupied(product_id, &role),
			};
			ensure!(!already_granted, Error::<T>::AlreadyGranted);
			Self::insert_role(product_id, &who, role.clone());
			Self::deposit_event(Event::PermissionGranted { product_id, role, who });
			Ok(())
		}

		/// Revoke `role` from `who` for `product_id`. Same authorization rules
		/// as `grant_permission`.
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::revoke_permission())]
		pub fn revoke_permission(
			origin: OriginFor<T>,
			product_id: ProductId,
			role: Role,
			who: T::AccountId,
		) -> DispatchResult {
			Self::ensure_role_authorized(origin, product_id, &role)?;
			ensure!(Self::has_role(product_id, &who, &role), Error::<T>::NotGranted);
			Self::remove_role(product_id, &who, &role);
			Self::deposit_event(Event::PermissionRevoked { product_id, role, who });
			Ok(())
		}

		/// Whitelist `investor` (a `ChainAddress` on `vault.chain_id`) for `vault`
		/// of `product_id`. Caller must hold `ProductAdmin` for the product; `vault`
		/// must be an active tranche of it, and the product must be `Multichain`
		/// (see `Error::TrancheInvestorMultichainOnly`).
		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::grant_tranche_investor())]
		pub fn grant_tranche_investor(
			origin: OriginFor<T>,
			product_id: ProductId,
			vault: VaultId,
			investor: ChainAddress,
		) -> DispatchResult {
			Self::ensure_tranche_investor_allowed(origin, product_id, &vault)?;
			let key = (product_id, vault.clone(), investor);
			ensure!(!TrancheInvestors::<T>::contains_key(&key), Error::<T>::AlreadyGranted);
			TrancheInvestors::<T>::insert(key, ());
			Self::deposit_event(Event::TrancheInvestorGranted { product_id, vault, investor });
			Ok(())
		}

		/// Remove `investor` from `vault`'s whitelist of `product_id`. Same
		/// authorization rules as `grant_tranche_investor`.
		#[pallet::call_index(3)]
		#[pallet::weight(<T as Config>::WeightInfo::revoke_tranche_investor())]
		pub fn revoke_tranche_investor(
			origin: OriginFor<T>,
			product_id: ProductId,
			vault: VaultId,
			investor: ChainAddress,
		) -> DispatchResult {
			Self::ensure_tranche_investor_allowed(origin, product_id, &vault)?;
			let key = (product_id, vault.clone(), investor);
			ensure!(TrancheInvestors::<T>::contains_key(&key), Error::<T>::NotGranted);
			TrancheInvestors::<T>::remove(key);
			Self::deposit_event(Event::TrancheInvestorRevoked { product_id, vault, investor });
			Ok(())
		}
	}
}
