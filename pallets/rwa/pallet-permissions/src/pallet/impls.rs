use crate::Role;

use pallet_pools::{PermissionInspect, PoolId, TrancheId};

use super::pallet::*;

impl<T: Config> Pallet<T> {
	/// Returns `true` if `who` specifically holds `role` for `pool_id`.
	pub(crate) fn has_role(pool_id: PoolId, who: &T::AccountId, role: &Role) -> bool {
		match role {
			Role::PoolAdmin => PoolAdmins::<T>::get(pool_id).as_ref() == Some(who),
			Role::Borrower => Borrowers::<T>::get(pool_id).as_ref() == Some(who),
			Role::OracleFeeder => OracleFeeders::<T>::contains_key(pool_id, who),
			Role::TrancheInvestor(tranche_id) => {
				TrancheInvestors::<T>::contains_key(tranche_id, who)
			},
		}
	}

	/// Returns `true` if the 1:1 role slot for `pool_id` is already occupied by anyone.
	/// Always returns `false` for DoubleMap-based roles (OracleFeeder, TrancheInvestor).
	pub(crate) fn role_occupied(pool_id: PoolId, role: &Role) -> bool {
		match role {
			Role::PoolAdmin => PoolAdmins::<T>::contains_key(pool_id),
			Role::Borrower => Borrowers::<T>::contains_key(pool_id),
			Role::OracleFeeder | Role::TrancheInvestor(_) => false,
		}
	}

	pub(crate) fn insert_role(pool_id: PoolId, who: &T::AccountId, role: Role) {
		match role {
			Role::PoolAdmin => PoolAdmins::<T>::insert(pool_id, who),
			Role::Borrower => Borrowers::<T>::insert(pool_id, who),
			Role::OracleFeeder => OracleFeeders::<T>::insert(pool_id, who, ()),
			Role::TrancheInvestor(tranche_id) => TrancheInvestors::<T>::insert(tranche_id, who, ()),
		}
	}

	pub(crate) fn remove_role(pool_id: PoolId, who: &T::AccountId, role: &Role) {
		match role {
			// 1:1 roles: key is just pool_id; `who` verified by has_role before this call.
			Role::PoolAdmin => PoolAdmins::<T>::remove(pool_id),
			Role::Borrower => Borrowers::<T>::remove(pool_id),
			Role::OracleFeeder => OracleFeeders::<T>::remove(pool_id, who),
			Role::TrancheInvestor(tranche_id) => TrancheInvestors::<T>::remove(tranche_id, who),
		}
	}
}

impl<T: Config> PermissionInspect<T::AccountId> for Pallet<T> {
	fn is_pool_admin(pool_id: PoolId, who: &T::AccountId) -> bool {
		PoolAdmins::<T>::get(pool_id).as_ref() == Some(who)
	}

	fn is_borrower(pool_id: PoolId, who: &T::AccountId) -> bool {
		Borrowers::<T>::get(pool_id).as_ref() == Some(who)
	}

	fn is_oracle_feeder(pool_id: PoolId, who: &T::AccountId) -> bool {
		OracleFeeders::<T>::contains_key(pool_id, who)
	}

	fn is_tranche_investor(tranche_id: &TrancheId, who: &T::AccountId) -> bool {
		TrancheInvestors::<T>::contains_key(tranche_id, who)
	}

	fn grant_borrower(pool_id: PoolId, who: T::AccountId) {
		Borrowers::<T>::insert(pool_id, &who);
	}
}
