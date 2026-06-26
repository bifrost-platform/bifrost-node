mod impls;

use crate::{PoolId, Role, WeightInfo};
use pallet_pools::{PoolInspect, TrancheId};

use frame_support::{pallet_prelude::*, traits::StorageVersion};
use frame_system::pallet_prelude::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
		/// Pool inspection — implemented by pallet-pools.
		/// Used to verify that a tranche belongs to the given pool before
		/// granting the `TrancheInvestor` role.
		type Pools: PoolInspect;
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	// -----------------------------------------------------------------------
	// Errors
	// -----------------------------------------------------------------------

	#[pallet::error]
	pub enum Error<T> {
		/// Caller does not hold the PoolAdmin role for this pool.
		NotPoolAdmin,
		/// The role is already granted to this account.
		AlreadyGranted,
		/// The role is not currently granted to this account.
		NotGranted,
		/// The tranche does not exist or does not belong to the given pool.
		PoolOrTrancheNotFound,
		/// The Borrower role is managed exclusively by `create_pool` and cannot
		/// be granted or revoked through this extrinsic.
		BorrowerRoleReserved,
	}

	// -----------------------------------------------------------------------
	// Events
	// -----------------------------------------------------------------------

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A role was granted to an account for a pool.
		PermissionGranted { pool_id: PoolId, role: Role, who: T::AccountId },
		/// A role was revoked from an account for a pool.
		PermissionRevoked { pool_id: PoolId, role: Role, who: T::AccountId },
	}

	// -----------------------------------------------------------------------
	// Storage
	// -----------------------------------------------------------------------

	/// The single PoolAdmin per pool. Only writable by sudo.
	#[pallet::storage]
	pub type PoolAdmins<T: Config> = StorageMap<_, Blake2_128Concat, PoolId, T::AccountId>;

	/// The single Borrower per pool. Set atomically by `create_pool`.
	#[pallet::storage]
	pub type Borrowers<T: Config> = StorageMap<_, Blake2_128Concat, PoolId, T::AccountId>;

	/// Oracle feeders per pool.
	/// O(1) lookup via `contains_key(pool_id, who)`; iterate all feeders with `iter_prefix(pool_id)`.
	#[pallet::storage]
	pub type OracleFeeders<T: Config> =
		StorageDoubleMap<_, Blake2_128Concat, PoolId, Blake2_128Concat, T::AccountId, ()>;

	/// Whitelisted investors per tranche.
	/// TrancheId is globally unique (chain_id + vault_address), so no pool key is needed.
	/// O(1) lookup via `contains_key(tranche_id, who)`; iterate all investors with `iter_prefix(tranche_id)`.
	#[pallet::storage]
	pub type TrancheInvestors<T: Config> =
		StorageDoubleMap<_, Blake2_128Concat, TrancheId, Blake2_128Concat, T::AccountId, ()>;

	// -----------------------------------------------------------------------
	// Extrinsics
	// -----------------------------------------------------------------------

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Grant `role` to `who` for `pool_id`.
		///
		/// Authorization:
		/// - `Role::PoolAdmin` → caller must be sudo (root).
		/// - `Role::OracleFeeder` | `Role::TrancheInvestor`
		///   → caller must hold `PoolAdmin` for the given pool.
		///
		/// `Role::Borrower` cannot be granted here; it is set atomically by `create_pool`.
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn grant_permission(
			origin: OriginFor<T>,
			pool_id: PoolId,
			role: Role,
			who: T::AccountId,
		) -> DispatchResult {
			ensure!(role != Role::Borrower, Error::<T>::BorrowerRoleReserved);
			match &role {
				Role::PoolAdmin => {
					ensure_root(origin)?;
				},
				_ => {
					let caller = ensure_signed(origin)?;
					ensure!(
						PoolAdmins::<T>::get(pool_id).as_ref() == Some(&caller),
						Error::<T>::NotPoolAdmin
					);
				},
			}
			// For TrancheInvestor, verify the tranche belongs to this pool before
			// writing — otherwise a PoolAdmin could whitelist investors for a tranche
			// owned by a different pool.
			if let Role::TrancheInvestor(tranche_id) = &role {
				ensure!(
					T::Pools::tranche_exists(pool_id, tranche_id.clone()),
					Error::<T>::PoolOrTrancheNotFound
				);
			}

			// 1:1 roles: fail if the slot is already occupied by anyone.
			// OracleFeeder and TrancheInvestor are 1:many, so check the specific account.
			let already_granted = match &role {
				Role::OracleFeeder | Role::TrancheInvestor(_) => {
					Self::has_role(pool_id, &who, &role)
				},
				_ => Self::role_occupied(pool_id, &role),
			};
			ensure!(!already_granted, Error::<T>::AlreadyGranted);
			Self::insert_role(pool_id, &who, role.clone());
			Self::deposit_event(Event::PermissionGranted { pool_id, role, who });
			Ok(())
		}

		/// Revoke `role` from `who` for `pool_id`.
		///
		/// Authorization:
		/// - `Role::PoolAdmin` → caller must be sudo (root).
		/// - `Role::OracleFeeder` | `Role::TrancheInvestor`
		///   → caller must hold `PoolAdmin` for the given pool.
		///
		/// `Role::Borrower` cannot be revoked here; it is tied to the pool lifecycle.
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn revoke_permission(
			origin: OriginFor<T>,
			pool_id: PoolId,
			role: Role,
			who: T::AccountId,
		) -> DispatchResult {
			ensure!(role != Role::Borrower, Error::<T>::BorrowerRoleReserved);
			match &role {
				Role::PoolAdmin => {
					ensure_root(origin)?;
				},
				_ => {
					let caller = ensure_signed(origin)?;
					ensure!(
						PoolAdmins::<T>::get(pool_id).as_ref() == Some(&caller),
						Error::<T>::NotPoolAdmin
					);
				},
			}
			if let Role::TrancheInvestor(tranche_id) = &role {
				ensure!(
					T::Pools::tranche_exists(pool_id, tranche_id.clone()),
					Error::<T>::PoolOrTrancheNotFound
				);
			}
			ensure!(Self::has_role(pool_id, &who, &role), Error::<T>::NotGranted);
			Self::remove_role(pool_id, &who, &role);
			Self::deposit_event(Event::PermissionRevoked { pool_id, role, who });
			Ok(())
		}
	}
}
