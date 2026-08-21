mod impls;

use crate::{Role, WeightInfo};
use pallet_tranche_system::{ProductId, VaultId, VaultInspect};

use frame_support::{pallet_prelude::*, traits::StorageVersion};
use frame_system::pallet_prelude::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Vault inspector — implemented by pallet-tranche-system (it owns the
		/// `Vaults` reverse index). Used to verify a vault actually belongs to
		/// `product_id` before granting `Role::TrancheInvestor` for it.
		type Vaults: VaultInspect;
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
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
	/// Whitelisted investors per tranche — many per tranche.
	/// `VaultId` is globally unique (chain_id + vault_address, enforced by
	/// pallet-tranche-system), so no product key is needed here.
	pub type TrancheInvestors<T: Config> =
		StorageDoubleMap<_, Blake2_128Concat, VaultId, Blake2_128Concat, T::AccountId, ()>;

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
		/// - `Role::OracleFeeder` | `Role::TrancheInvestor` — caller must hold `ProductAdmin` for
		///   the given product.
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::grant_permission())]
		pub fn grant_permission(
			origin: OriginFor<T>,
			product_id: ProductId,
			role: Role,
			who: T::AccountId,
		) -> DispatchResult {
			match &role {
				Role::ProductAdmin => {
					ensure_root(origin)?;
				},
				_ => {
					let caller = ensure_signed(origin)?;
					ensure!(
						ProductAdmins::<T>::get(product_id).as_ref() == Some(&caller),
						Error::<T>::NotProductAdmin
					);
				},
			}
			// For TrancheInvestor, verify the vault belongs to this product before
			// writing — otherwise a ProductAdmin could whitelist investors for a
			// vault owned by a different product.
			if let Role::TrancheInvestor(vault) = &role {
				ensure!(
					T::Vaults::vault_belongs_to_product(product_id, vault),
					Error::<T>::ProductOrVaultNotFound
				);
			}

			// ProductAdmin is 1:1: fail if the slot is already occupied by
			// anyone. OracleFeeder and TrancheInvestor are 1:many, so check
			// the specific account instead.
			let already_granted = match &role {
				Role::OracleFeeder | Role::TrancheInvestor(_) => {
					Self::has_role(product_id, &who, &role)
				},
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
			match &role {
				Role::ProductAdmin => {
					ensure_root(origin)?;
				},
				_ => {
					let caller = ensure_signed(origin)?;
					ensure!(
						ProductAdmins::<T>::get(product_id).as_ref() == Some(&caller),
						Error::<T>::NotProductAdmin
					);
				},
			}
			if let Role::TrancheInvestor(vault) = &role {
				ensure!(
					T::Vaults::vault_belongs_to_product(product_id, vault),
					Error::<T>::ProductOrVaultNotFound
				);
			}
			ensure!(Self::has_role(product_id, &who, &role), Error::<T>::NotGranted);
			Self::remove_role(product_id, &who, &role);
			Self::deposit_event(Event::PermissionRevoked { product_id, role, who });
			Ok(())
		}
	}
}
