use crate::Role;

use pallet_tranche_system::{ProductId, ProductInspect, VaultInspect};

use super::pallet::*;
use frame_support::{ensure, pallet_prelude::DispatchResult};
use frame_system::pallet_prelude::{ensure_root, ensure_signed, OriginFor};

impl<T: Config> Pallet<T> {
	/// Checks `origin` is authorized to grant/revoke `role` for `product_id`
	/// — shared by `grant_permission`/`revoke_permission`, which apply the
	/// exact same authorization rule in both directions. `Role::ProductAdmin`
	/// requires root (sudo) — see `grant_permission`'s doc comment for why a
	/// precompile-dispatched call with this role always reverts here. Every
	/// other role requires a signed origin that already holds
	/// `Role::ProductAdmin` for `product_id`.
	pub(crate) fn ensure_role_authorized(
		origin: OriginFor<T>,
		product_id: ProductId,
		role: &Role,
	) -> DispatchResult {
		match role {
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
		Ok(())
	}

	/// For `Role::TrancheInvestor(vault)`, checks `vault` actually belongs to
	/// `product_id` — shared by `grant_permission`/`revoke_permission`. A
	/// no-op for every other role. Without this, a ProductAdmin could
	/// grant/revoke the TrancheInvestor whitelist for a vault owned by a
	/// different product.
	pub(crate) fn ensure_tranche_investor_vault_registered(
		product_id: ProductId,
		role: &Role,
	) -> DispatchResult {
		if let Role::TrancheInvestor(vault) = role {
			ensure!(
				T::Vaults::vault_belongs_to_product(product_id, vault),
				Error::<T>::ProductOrVaultNotFound
			);
		}
		Ok(())
	}

	/// For `Role::TrancheInvestor`, checks `product_id` is a `Multichain`
	/// product — shared by `grant_permission`/`revoke_permission`. A no-op for
	/// every other role. A `SingleChain` product's own TrancheManager Contract
	/// manages investor whitelisting directly; it never dispatches through
	/// this pallet at all, so this storage should never gain an entry under a
	/// `SingleChain` `product_id` in the first place.
	pub(crate) fn ensure_tranche_investor_multichain_only(
		product_id: ProductId,
		role: &Role,
	) -> DispatchResult {
		if let Role::TrancheInvestor(_) = role {
			ensure!(
				T::Products::single_chain_id(product_id).is_none(),
				Error::<T>::TrancheInvestorMultichainOnly
			);
		}
		Ok(())
	}

	/// Returns `true` if `who` specifically holds `role` for `product_id`.
	pub(crate) fn has_role(product_id: ProductId, who: &T::AccountId, role: &Role) -> bool {
		match role {
			Role::ProductAdmin => ProductAdmins::<T>::get(product_id).as_ref() == Some(who),
			Role::OracleFeeder => OracleFeeders::<T>::contains_key(product_id, who),
			Role::TrancheInvestor(vault) => {
				TrancheInvestors::<T>::contains_key((product_id, vault, who))
			},
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
			Role::TrancheInvestor(vault) => {
				TrancheInvestors::<T>::insert((product_id, vault, who), ())
			},
		}
	}

	pub(crate) fn remove_role(product_id: ProductId, who: &T::AccountId, role: &Role) {
		match role {
			Role::ProductAdmin => ProductAdmins::<T>::remove(product_id),
			Role::OracleFeeder => OracleFeeders::<T>::remove(product_id, who),
			Role::TrancheInvestor(vault) => TrancheInvestors::<T>::remove((product_id, vault, who)),
		}
	}
}
