mod impls;

use crate::{
	CollateralAsset, EpochInfo, PoolDetails, PoolId, ReserveDetails, SettlementMode, Tranche,
	TrancheId, TrancheIndex, TrancheInput,
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
		/// At least one tranche is required and the last must be junior.
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
		/// An ERC-7540 vault was registered to a tranche.
		VaultAdded { pool_id: PoolId, tranche_index: TrancheIndex, tranche_id: TrancheId },
		/// Pool reserve was updated (via invest settlement or repay).
		ReserveUpdated { pool_id: PoolId, total: U256, available: U256 },
		/// An epoch ended and a new one began.
		EpochAdvanced { pool_id: PoolId, new_epoch: u32 },
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
	// Hooks
	// -----------------------------------------------------------------------

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_now: BlockNumberFor<T>) -> Weight {
			let now = Self::current_block();
			let mut weight = Weight::zero();

			for (pool_id, mut pool) in Pool::<T>::iter() {
				weight = weight.saturating_add(Weight::from_parts(1_000, 0));

				if pool.epoch.should_advance(now) {
					pool.epoch.advance(now);
					let new_epoch = pool.epoch.current_epoch;

					// Automatic invest settlement runs on epoch transition.
					// Confirmed invest orders are written here; the off-chain bot
					// handles cross-chain mint and clears them afterwards.
					if pool.invest_settlement == SettlementMode::Automatic {
						Self::settle_invest_orders(pool_id, &mut pool);
					}

					Pool::<T>::insert(pool_id, pool);
					Self::deposit_event(Event::EpochAdvanced { pool_id, new_epoch });
				}
			}

			weight
		}
	}

	// -----------------------------------------------------------------------
	// Extrinsics
	// -----------------------------------------------------------------------

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Create a new RWA pool.
		///
		/// `tranches` must end with exactly one junior tranche.
		/// All preceding entries must be senior, ordered most-senior first.
		///
		/// `settlement_start_offset` is how many blocks before epoch end
		/// the settlement window opens. During this window new orders are rejected.
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::from_parts(10_000, 0))]
		pub fn create_pool(
			origin: OriginFor<T>,
			currency: H160,
			epoch_length: u32,
			settlement_start_offset: u32,
			max_reserve: U256,
			investment_ceiling: U256,
			invest_settlement: SettlementMode,
			redeem_settlement: SettlementMode,
			nft_contract: H160,
			nft_token_id: U256,
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
					total: U256::zero(),
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
				epoch: EpochInfo::new(epoch_length, settlement_start_offset, now),
				collateral: CollateralAsset { nft_contract, nft_token_id },
				investment_ceiling,
				invest_settlement,
				redeem_settlement,
			};

			Pool::<T>::insert(pool_id, pool);
			NextPoolId::<T>::put(pool_id.saturating_add(1));

			Self::deposit_event(Event::PoolCreated { pool_id, admin, epoch_length });
			Ok(())
		}

		/// Update epoch parameters (admin only).
		#[pallet::call_index(1)]
		#[pallet::weight(Weight::from_parts(5_000, 0))]
		pub fn update_pool(
			origin: OriginFor<T>,
			pool_id: PoolId,
			epoch_length: u32,
			settlement_start_offset: u32,
		) -> DispatchResult {
			let caller = ensure_signed(origin)?;
			Pool::<T>::try_mutate(pool_id, |maybe_pool| -> Result<(), DispatchError> {
				let pool = maybe_pool.as_mut().ok_or(Error::<T>::PoolNotFound)?;
				ensure!(pool.admin == caller, Error::<T>::NotPoolAdmin);
				pool.epoch.epoch_length = epoch_length;
				pool.epoch.settlement_start_offset = settlement_start_offset;
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

		/// Register an ERC-7540 vault (chain_id + vault_address) to a tranche (admin only).
		///
		/// Each tranche slot is created at pool creation; this call associates it with
		/// the deployed vault contract on the external EVM chain.
		#[pallet::call_index(3)]
		#[pallet::weight(Weight::from_parts(5_000, 0))]
		pub fn add_vault(
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
			Self::deposit_event(Event::VaultAdded { pool_id, tranche_index, tranche_id });
			Ok(())
		}
	}
}
