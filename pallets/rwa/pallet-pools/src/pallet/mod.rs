mod impls;

use crate::{
	EpochInfo, PoolDetails, PoolId, PoolNAV, ReserveDetails, Tranche, TrancheId, TrancheIndex,
	TrancheInput,
};

use frame_support::{pallet_prelude::*, traits::StorageVersion};
use frame_system::pallet_prelude::*;
use sp_core::{H160, U256};
use sp_runtime::DispatchError;
use sp_std::vec::Vec;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
		/// NAV provider — implemented by pallet-loans.
		type NAV: PoolNAV<PoolId, U256>;

		/// Maximum number of tranches per pool (enforced at creation).
		#[pallet::constant]
		type MaxTranches: Get<u32>;
	}

	// -----------------------------------------------------------------------
	// Errors
	// -----------------------------------------------------------------------

	#[pallet::error]
	pub enum Error<T> {
		/// No pool exists with this ID.
		PoolNotFound,
		/// Caller is not the pool admin.
		NotPoolAdmin,
		/// The junior (residual) tranche must be the last entry in the tranche list.
		JuniorTrancheMustBeLast,
		/// At least one tranche is required and the last must be residual.
		MissingResidualTranche,
		/// Tranche index is out of range.
		TrancheNotFound,
		/// Pool reserve is insufficient for the requested withdrawal.
		InsufficientReserve,
	}

	// -----------------------------------------------------------------------
	// Events
	// -----------------------------------------------------------------------

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new pool was created.
		PoolCreated { pool_id: PoolId, admin: T::AccountId, epoch_length: u32 },
		/// Pool parameters were updated by the admin.
		PoolUpdated { pool_id: PoolId },
		/// Maximum reserve was changed.
		MaxReserveSet { pool_id: PoolId, max_reserve: U256 },
		/// A tranche ID (chain + vault address) was registered or updated.
		TrancheIdSet { pool_id: PoolId, tranche_id: TrancheId },
		/// Pool reserve was updated (via pallet-loans borrow or repay).
		ReserveUpdated { pool_id: PoolId, total: U256, available: U256 },
	}

	// -----------------------------------------------------------------------
	// Storage
	// -----------------------------------------------------------------------

	/// All active pools, keyed by pool ID.
	#[pallet::storage]
	#[pallet::unbounded]
	pub type Pool<T: Config> = StorageMap<_, Blake2_128Concat, PoolId, PoolDetails<T::AccountId>>;

	/// Monotonically increasing pool ID counter.
	#[pallet::storage]
	pub type NextPoolId<T: Config> = StorageValue<_, PoolId, ValueQuery>;

	// -----------------------------------------------------------------------
	// Extrinsics
	// -----------------------------------------------------------------------

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Create a new RWA pool.
		///
		/// `tranches` must end with exactly one `Residual` (junior) tranche.
		/// All preceding entries must be `NonResidual` (senior), ordered most-senior first.
		/// Each tranche is identified by its ERC-7540 `vault_address` on the external chain.
		///
		/// `epoch_length` is the number of blocks each epoch lasts.
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::from_parts(10_000, 0))]
		pub fn create_pool(
			origin: OriginFor<T>,
			currency: H160,
			epoch_length: u32,
			max_nav_age: u32,
			max_reserve: U256,
			tranches: BoundedVec<TrancheInput, T::MaxTranches>,
		) -> DispatchResult {
			let admin = ensure_signed(origin)?;

			ensure!(!tranches.is_empty(), Error::<T>::MissingResidualTranche);
			ensure!(
				tranches.last().map(|t| t.tranche_type.is_junior()).unwrap_or(false),
				Error::<T>::JuniorTrancheMustBeLast
			);

			let pool_id = NextPoolId::<T>::get();
			let now = Self::current_block();

			let built_tranches: Vec<Tranche> = tranches
				.iter()
				.map(|input| Tranche {
					tranche_type: input.tranche_type.clone(),
					tranche_id: input.tranche_id.clone(),
					principal: U256::zero(),
					interest: U256::zero(),
					total: U256::zero(),
					last_updated_interest: now,
					seniority: input.seniority,
				})
				.collect();

			let pool = PoolDetails {
				admin: admin.clone(),
				currency,
				reserve: ReserveDetails {
					max: max_reserve,
					total: U256::zero(),
					available: U256::zero(),
				},
				tranches: built_tranches,
				epoch: EpochInfo::new(epoch_length, now),
				max_nav_age,
				last_nav: U256::zero(),
				last_nav_update: 0,
			};

			Pool::<T>::insert(pool_id, pool);
			NextPoolId::<T>::put(pool_id.saturating_add(1));

			Self::deposit_event(Event::PoolCreated { pool_id, admin, epoch_length });
			Ok(())
		}

		/// Update epoch length and max NAV age (admin only).
		#[pallet::call_index(1)]
		#[pallet::weight(Weight::from_parts(5_000, 0))]
		pub fn update_pool(
			origin: OriginFor<T>,
			pool_id: PoolId,
			epoch_length: u32,
			max_nav_age: u32,
		) -> DispatchResult {
			let caller = ensure_signed(origin)?;
			Pool::<T>::try_mutate(pool_id, |maybe_pool| -> Result<(), DispatchError> {
				let pool = maybe_pool.as_mut().ok_or(Error::<T>::PoolNotFound)?;
				ensure!(pool.admin == caller, Error::<T>::NotPoolAdmin);
				pool.epoch.epoch_length = epoch_length;
				pool.max_nav_age = max_nav_age;
				Ok(())
			})?;
			Self::deposit_event(Event::PoolUpdated { pool_id });
			Ok(())
		}

		/// Change the maximum reserve (admin only).
		#[pallet::call_index(2)]
		#[pallet::weight(Weight::from_parts(5_000, 0))]
		pub fn set_max_reserve(
			origin: OriginFor<T>,
			pool_id: PoolId,
			max_reserve: U256,
		) -> DispatchResult {
			let caller = ensure_signed(origin)?;
			Pool::<T>::try_mutate(pool_id, |maybe_pool| -> Result<(), DispatchError> {
				let pool = maybe_pool.as_mut().ok_or(Error::<T>::PoolNotFound)?;
				ensure!(pool.admin == caller, Error::<T>::NotPoolAdmin);
				pool.reserve.max = max_reserve;
				Ok(())
			})?;
			Self::deposit_event(Event::MaxReserveSet { pool_id, max_reserve });
			Ok(())
		}

		/// Register or update the TrancheId (chain + vault address) for a tranche (admin only).
		#[pallet::call_index(3)]
		#[pallet::weight(Weight::from_parts(5_000, 0))]
		pub fn set_tranche_id(
			origin: OriginFor<T>,
			pool_id: PoolId,
			tranche_index: TrancheIndex,
			tranche_id: TrancheId,
		) -> DispatchResult {
			let caller = ensure_signed(origin)?;
			Pool::<T>::try_mutate(pool_id, |maybe_pool| -> Result<(), DispatchError> {
				let pool = maybe_pool.as_mut().ok_or(Error::<T>::PoolNotFound)?;
				ensure!(pool.admin == caller, Error::<T>::NotPoolAdmin);
				let tranche = pool
					.tranches
					.get_mut(tranche_index as usize)
					.ok_or(Error::<T>::TrancheNotFound)?;
				tranche.tranche_id = tranche_id.clone();
				Ok(())
			})?;
			Self::deposit_event(Event::TrancheIdSet { pool_id, tranche_id });
			Ok(())
		}
	}
}
