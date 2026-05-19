mod impls;

use crate::{
	CollateralAsset, EpochInfo, InvestmentSettlement, PoolDetails, PoolId, SettlementMode, Tranche,
	TrancheId, TrancheInput, TranchePendingOrders, MAX_TRANCHES,
};

use frame_support::{pallet_prelude::*, traits::StorageVersion};
use frame_system::pallet_prelude::*;
use sp_core::{H160, U256};
use sp_runtime::DispatchError;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
		/// Investment settlement — implemented by pallet-investments.
		/// Called during `on_initialize` to settle pending invest orders when epochs advance.
		type Investments: InvestmentSettlement<PoolId, TrancheId, sp_core::U256>;
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
		/// Collateral already exists.
		CollateralAlreadyExists,
		/// Tranche already exists.
		TrancheAlreadyExists,
		/// Out of range.
		OutOfRange,
	}

	// -----------------------------------------------------------------------
	// Events
	// -----------------------------------------------------------------------

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new pool was created.
		PoolCreated { pool_id: PoolId, epoch_length: u32 },
		/// Pool parameters were updated by the admin.
		PoolUpdated { pool_id: PoolId },
		/// Maximum reserve was changed.
		MaxReserveSet { pool_id: PoolId, max_reserve: U256 },
		/// An ERC-7540 vault was registered to a tranche.
		VaultAdded { pool_id: PoolId, tranche_id: TrancheId },
		/// Pool reserve was updated (via invest settlement or repay).
		ReserveUpdated { pool_id: PoolId, total: U256, available: U256 },
		/// An epoch ended and a new one began.
		EpochAdvanced { pool_id: PoolId, new_epoch: u32 },
	}

	// -----------------------------------------------------------------------
	// Storage
	// -----------------------------------------------------------------------

	#[pallet::storage]
	#[pallet::unbounded]
	/// All active pools, keyed by pool ID.
	pub type Pool<T: Config> = StorageMap<_, Blake2_128Concat, PoolId, PoolDetails>;

	#[pallet::storage]
	/// Mapped collateral assets to pool IDs.
	pub type Collaterals<T: Config> = StorageMap<_, Blake2_128Concat, CollateralAsset, PoolId>;

	#[pallet::storage]
	/// Mapped tranche IDs to pool IDs.
	pub type Tranches<T: Config> = StorageMap<_, Blake2_128Concat, TrancheId, PoolId>;

	#[pallet::storage]
	/// Monotonically increasing pool ID counter.
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
						// TODO: Implement automatic invest settlement.
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
			nft_contract: H160,
			nft_token_id: U256,
			epoch_length: u32,
			settlement_offset: u32,
			invest_settlement: SettlementMode,
			redeem_settlement: SettlementMode,
			tranches: BoundedVec<TrancheInput, ConstU32<MAX_TRANCHES>>,
		) -> DispatchResult {
			ensure_root(origin)?;

			let pool_id = NextPoolId::<T>::get();
			let now = Self::current_block();

			let collateral = CollateralAsset { nft_contract, nft_token_id };
			ensure!(
				!Collaterals::<T>::contains_key(collateral.clone()),
				Error::<T>::CollateralAlreadyExists
			);

			let mut built_tranches: BoundedBTreeMap<TrancheId, Tranche, ConstU32<MAX_TRANCHES>> =
				BoundedBTreeMap::new();
			for tranche in tranches.iter() {
				ensure!(
					!Tranches::<T>::contains_key(tranche.tranche_id.clone()),
					Error::<T>::TrancheAlreadyExists
				);
				built_tranches
					.try_insert(
						tranche.tranche_id.clone(),
						Tranche {
							tranche_type: tranche.tranche_type.clone(),
							max_supply: tranche.max_supply,
							token_supply: U256::zero(),
							invested: U256::zero(),
							borrowed: U256::zero(),
							pending_orders: TranchePendingOrders::default(),
						},
					)
					.map_err(|_| Error::<T>::OutOfRange)?;
			}

			let pool = PoolDetails {
				total: U256::zero(),
				tranches: built_tranches.clone(),
				epoch: EpochInfo::new(epoch_length, settlement_offset, now),
				collateral: collateral.clone(),
				invest_settlement,
				redeem_settlement,
			};

			Collaterals::<T>::insert(collateral, pool_id);
			for tranche in tranches.iter() {
				Tranches::<T>::insert(tranche.tranche_id.clone(), pool_id);
			}
			Pool::<T>::insert(pool_id, pool);
			NextPoolId::<T>::put(pool_id.saturating_add(1));

			Self::deposit_event(Event::PoolCreated { pool_id, epoch_length });
			Ok(())
		}

		/// Register an ERC-7540 vault (chain_id + vault_address) to a tranche (admin only).
		///
		/// Each tranche slot is created at pool creation; this call associates it with
		/// the deployed vault contract on the external EVM chain.
		#[pallet::call_index(1)]
		#[pallet::weight(Weight::from_parts(5_000, 0))]
		pub fn add_vault(
			origin: OriginFor<T>,
			pool_id: PoolId,
			tranche: TrancheInput,
		) -> DispatchResult {
			ensure_root(origin)?;

			ensure!(
				!Tranches::<T>::contains_key(tranche.tranche_id.clone()),
				Error::<T>::TrancheAlreadyExists
			);

			Pool::<T>::try_mutate(pool_id, |maybe_pool| -> Result<(), DispatchError> {
				let pool = maybe_pool.as_mut().ok_or(Error::<T>::PoolNotFound)?;
				pool.tranches
					.try_insert(
						tranche.tranche_id.clone(),
						Tranche {
							tranche_type: tranche.tranche_type.clone(),
							max_supply: tranche.max_supply,
							token_supply: U256::zero(),
							invested: U256::zero(),
							borrowed: U256::zero(),
							pending_orders: TranchePendingOrders::default(),
						},
					)
					.map_err(|_| Error::<T>::OutOfRange)?;
				Ok(())
			})?;
			Self::deposit_event(Event::VaultAdded { pool_id, tranche_id: tranche.tranche_id });
			Ok(())
		}
	}
}
