use crate::Role;

use pallet_tranche_system::{ProductId, ProductInspect, VaultId, VaultInspect};

use super::pallet::*;
use frame_support::{ensure, pallet_prelude::DispatchResult};
use frame_system::pallet_prelude::{ensure_root, ensure_signed, OriginFor};

impl<T: Config> Pallet<T> {
	/// `Role::ProductAdmin` is root-only; every other role (and the tranche
	/// investor whitelist) requires the caller to hold `ProductAdmin` for `product_id`.
	pub(crate) fn ensure_role_authorized(
		origin: OriginFor<T>,
		product_id: ProductId,
		role: &Role,
	) -> DispatchResult {
		match role {
			Role::ProductAdmin => {
				ensure_root(origin)?;
			},
			Role::OracleFeeder => Self::ensure_product_admin(origin, product_id)?,
		}
		Ok(())
	}

	fn ensure_product_admin(origin: OriginFor<T>, product_id: ProductId) -> DispatchResult {
		let caller = ensure_signed(origin)?;
		ensure!(
			ProductAdmins::<T>::get(product_id).as_ref() == Some(&caller),
			Error::<T>::NotProductAdmin
		);
		Ok(())
	}

	/// Shared checks for `grant_tranche_investor`/`revoke_tranche_investor`: the
	/// caller is the product's admin, `vault` is an active tranche of the product,
	/// and the product is `Multichain` (single-chain products whitelist on their
	/// own chain, not through the Hub).
	pub(crate) fn ensure_tranche_investor_allowed(
		origin: OriginFor<T>,
		product_id: ProductId,
		vault: &VaultId,
	) -> DispatchResult {
		Self::ensure_product_admin(origin, product_id)?;
		ensure!(
			T::Vaults::vault_belongs_to_product(product_id, vault),
			Error::<T>::ProductOrVaultNotFound
		);
		ensure!(
			T::Products::single_chain_id(product_id).is_none(),
			Error::<T>::TrancheInvestorMultichainOnly
		);
		Ok(())
	}

	pub(crate) fn has_role(product_id: ProductId, who: &T::AccountId, role: &Role) -> bool {
		match role {
			Role::ProductAdmin => ProductAdmins::<T>::get(product_id).as_ref() == Some(who),
			Role::OracleFeeder => OracleFeeders::<T>::contains_key(product_id, who),
		}
	}

	pub(crate) fn role_occupied(product_id: ProductId, role: &Role) -> bool {
		match role {
			Role::ProductAdmin => ProductAdmins::<T>::contains_key(product_id),
			Role::OracleFeeder => false,
		}
	}

	pub(crate) fn insert_role(product_id: ProductId, who: &T::AccountId, role: Role) {
		match role {
			Role::ProductAdmin => ProductAdmins::<T>::insert(product_id, who),
			Role::OracleFeeder => OracleFeeders::<T>::insert(product_id, who, ()),
		}
	}

	pub(crate) fn remove_role(product_id: ProductId, who: &T::AccountId, role: &Role) {
		match role {
			Role::ProductAdmin => ProductAdmins::<T>::remove(product_id),
			Role::OracleFeeder => OracleFeeders::<T>::remove(product_id, who),
		}
	}
}
