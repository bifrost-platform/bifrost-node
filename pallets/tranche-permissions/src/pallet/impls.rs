use crate::Role;

use pallet_tranche_system::{PermissionInspect, ProductId};

use super::pallet::*;

impl<T: Config> Pallet<T> {
	/// Returns `true` if `who` specifically holds `role` for `product_id`.
	pub(crate) fn has_role(product_id: ProductId, who: &T::AccountId, role: &Role) -> bool {
		match role {
			Role::ProductAdmin => ProductAdmins::<T>::get(product_id).as_ref() == Some(who),
			Role::OracleFeeder => OracleFeeders::<T>::contains_key(product_id, who),
			Role::TrancheInvestor(vault) => TrancheInvestors::<T>::contains_key(vault, who),
		}
	}

	/// Returns `true` if the 1:1 role slot for `product_id` is already
	/// occupied by anyone. Only meaningful for `Role::ProductAdmin` — the
	/// other roles are 1:many, checked per-account via `has_role` instead.
	pub(crate) fn role_occupied(product_id: ProductId, role: &Role) -> bool {
		match role {
			Role::ProductAdmin => ProductAdmins::<T>::contains_key(product_id),
			Role::OracleFeeder | Role::TrancheInvestor(_) => false,
		}
	}

	pub(crate) fn insert_role(product_id: ProductId, who: &T::AccountId, role: Role) {
		match role {
			Role::ProductAdmin => ProductAdmins::<T>::insert(product_id, who),
			Role::OracleFeeder => OracleFeeders::<T>::insert(product_id, who, ()),
			Role::TrancheInvestor(vault) => TrancheInvestors::<T>::insert(vault, who, ()),
		}
	}

	pub(crate) fn remove_role(product_id: ProductId, who: &T::AccountId, role: &Role) {
		match role {
			Role::ProductAdmin => ProductAdmins::<T>::remove(product_id),
			Role::OracleFeeder => OracleFeeders::<T>::remove(product_id, who),
			Role::TrancheInvestor(vault) => TrancheInvestors::<T>::remove(vault, who),
		}
	}
}

impl<T: Config> PermissionInspect<T::AccountId> for Pallet<T> {
	fn is_product_admin(product_id: ProductId, who: &T::AccountId) -> bool {
		ProductAdmins::<T>::get(product_id).as_ref() == Some(who)
	}
}
