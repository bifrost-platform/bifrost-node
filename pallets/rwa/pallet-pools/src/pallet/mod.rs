mod impls;

use crate::{
	CollateralAsset, EpochInfo, PermissionInspect, PoolDetails, PoolId, PoolNAV, Settlement,
	SettlementMode, Tranche, TrancheId, TrancheInput, TranchePendingOrders, TrancheType,
	MAX_COLLATERALS, MAX_TRANCHES,
};

use frame_support::{pallet_prelude::*, traits::StorageVersion, traits::UnixTime};
use frame_system::pallet_prelude::*;
use sp_core::{H160, U256};
use sp_runtime::{DispatchError, FixedU128};
use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::origin]
	pub enum Origin {
		/// Dispatched by the pools precompile on behalf of the Gateway contract.
		Gateway,
		/// Dispatched by the pools precompile on behalf of a Pool Admin EOA.
		/// Used to ensure `create_pool` can only be called through the precompile,
		/// guaranteeing the Gateway vault-deployment message is always sent alongside
		/// the Hub state change.
		PoolAdmin,
	}

	#[pallet::config]
	pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
		/// Only accepted origin for `borrow` and `repay`.
		/// Wire as `pallet_pools::EnsureGateway` in the runtime so that only the
		/// pools precompile (called by the Gateway contract) can invoke those extrinsics.
		type GatewayOrigin: frame_support::traits::EnsureOrigin<Self::RuntimeOrigin>;
		/// Only accepted origin for `create_pool`.
		/// Wire as `pallet_pools::EnsurePoolAdmin` in the runtime so that only the
		/// pools precompile can invoke that extrinsic, ensuring the Gateway message
		/// that deploys Spoke-chain vaults is always sent alongside the Hub state change.
		type PoolAdminOrigin: frame_support::traits::EnsureOrigin<Self::RuntimeOrigin>;
		/// Order settlement — implemented by pallet-investments.
		/// Called during `on_initialize` to settle pending deposit and redeem orders when
		/// epochs advance in Automatic mode.
		type Investments: Settlement<PoolId, TrancheId, sp_core::U256>;
		/// NAV oracle — implemented externally. Called to read the finalized collateral NAV
		/// when the settlement window opens.
		type NAV: PoolNAV<PoolId, sp_core::U256>;
		/// Unix wall-clock time. Wire as `pallet_timestamp::Pallet<Runtime>` in the runtime.
		/// Used for timestamp-based epoch advancement and interest accrual, so that skipped
		/// Aura slots (each 3 s) do not distort elapsed-time calculations.
		type Time: UnixTime;
		/// Permission inspector — implemented by pallet-permissions.
		/// Used to gate `create_pool`, `add_vault`, and other pool-admin actions.
		type Permissions: PermissionInspect<Self::AccountId>;
	}

	// -----------------------------------------------------------------------
	// Errors
	// -----------------------------------------------------------------------

	#[pallet::error]
	pub enum Error<T> {
		/// No pool exists with this ID.
		PoolNotFound,
		/// Tranche index is out of range.
		TrancheNotFound,
		/// At least one collateral NFT is required.
		MissingCollateral,
		/// Collateral already exists.
		CollateralAlreadyExists,
		/// Tranche already exists.
		TrancheAlreadyExists,
		/// Out of range.
		OutOfRange,
		/// Borrow amount exceeds available tranche treasury liquidity (invested − borrowed).
		InsufficientTreasuryLiquidity,
		/// Amount must be greater than zero.
		ZeroAmount,
		/// `settlement_offset` must be greater than zero and less than `epoch_length`.
		/// A zero offset means the settlement window never opens before the epoch ends,
		/// so `epoch_price` is never set and automatic settlement falls back to 1:1 pricing.
		InvalidSettlementOffset,
		/// The provided APR could not be converted to a per-second rate factor.
		InvalidRate,
		/// Caller does not hold the required role for this pool.
		Unauthorized,
		/// A pool with this ID already exists.
		PoolAlreadyExists,
	}

	// -----------------------------------------------------------------------
	// Events
	// -----------------------------------------------------------------------

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new pool was created.
		PoolCreated { pool_id: PoolId, epoch_length_secs: u64 },
		/// An ERC-7540 vault was registered to a tranche.
		VaultAdded { pool_id: PoolId, tranche_id: TrancheId },
		/// An epoch ended and a new one began.
		EpochAdvanced { pool_id: PoolId, new_epoch: u32 },
		/// Borrower drew funds from a tranche treasury.
		Borrowed { pool_id: PoolId, tranche_id: TrancheId, amount: U256, available: U256 },
		/// Borrower repaid funds into a tranche treasury.
		Repaid { pool_id: PoolId, tranche_id: TrancheId, amount: U256, available: U256 },
		/// The Gateway EVM contract address was updated by sudo.
		GatewayUpdated { address: H160 },
	}

	// -----------------------------------------------------------------------
	// Storage
	// -----------------------------------------------------------------------

	#[pallet::storage]
	#[pallet::unbounded]
	/// All active pools, keyed by pool ID.
	pub type Pools<T: Config> = StorageMap<_, Blake2_128Concat, PoolId, PoolDetails>;

	#[pallet::storage]
	/// Mapped collateral assets to pool IDs.
	pub type Collaterals<T: Config> = StorageMap<_, Blake2_128Concat, CollateralAsset, PoolId>;

	#[pallet::storage]
	/// Mapped tranche IDs to pool IDs.
	pub type Tranches<T: Config> = StorageMap<_, Blake2_128Concat, TrancheId, PoolId>;

	#[pallet::storage]
	/// The EVM address of the deployed Gateway contract.
	/// All precompile calls are rejected unless `msg.sender` matches this address.
	/// Defaults to the zero address (disables precompile access) until set by sudo.
	pub type GatewayAddress<T: Config> = StorageValue<_, H160, ValueQuery>;

	#[pallet::storage]
	/// Unix timestamp (seconds) of the next on_initialize action for each pool.
	/// Set to the settlement window open time at pool creation, then updated after
	/// each settlement (→ epoch end) and each epoch advance (→ next settlement open).
	/// Allows on_initialize to skip idle pools without decoding their full PoolDetails.
	pub type NextEpochAction<T: Config> = StorageMap<_, Blake2_128Concat, PoolId, u64, ValueQuery>;

	// -----------------------------------------------------------------------
	// Hooks
	// -----------------------------------------------------------------------

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_: BlockNumberFor<T>) -> Weight {
			let now_secs = T::Time::now().as_secs();
			let mut weight = Weight::zero();

			// Iterate the lightweight NextEpochAction index (one u64 per pool) rather than
			// decoding full PoolDetails for every pool every block. Only pools whose next
			// scheduled action time has been reached incur the expensive Pools::get() decode.
			for (pool_id, next_action_secs) in NextEpochAction::<T>::iter() {
				weight = weight.saturating_add(Weight::from_parts(100, 0));
				if next_action_secs > now_secs {
					continue;
				}

				let Some(mut pool) = Pools::<T>::get(pool_id) else { continue };
				weight = weight.saturating_add(Weight::from_parts(900, 0));
				let mut changed = false;

				// Settlement window just opened: lock epoch price and settle Automatic orders.
				// `needs_finalization` is the once-only guard — after prices are set it
				// stays false for the remainder of the window, preventing double-settlement.
				//
				// If no block was produced during a prior settlement window (e.g. validator
				// outage), that epoch's window is simply skipped. Pending orders are NOT lost —
				// they remain in PendingDepositOrders / PendingRedeemOrders and are carried
				// forward to this window, where they settle at the current epoch's NAV price.
				// Investors should expect settlement in the next available window, not
				// necessarily the epoch in which they submitted.
				if pool.epoch.in_settlement_window(now_secs) {
					let needs_finalization =
						pool.tranches.values().any(|t| t.epoch_price.is_none());
					if needs_finalization {
						let oracle_nav = T::NAV::nav(pool_id).map(|(n, _)| n).unwrap_or_default();
						// Accrue for the full intended epoch duration, not `now - epoch_start_secs`.
						// The settlement window opens `settlement_offset_secs` before epoch end,
						// so using the real elapsed time would under-accrue by that offset.
						let elapsed_secs = pool.epoch.epoch_length_secs;

						// Step 1: Compound-accrue senior NAVs for the elapsed epoch.
						// All deposits settled in this epoch — including those submitted
						// mid-epoch — accrue interest for the full epoch_length_secs.
						// This is an intentional approximation: tracking per-deposit
						// timestamps to pro-rate within-epoch accrual would add significant
						// complexity for a negligible per-epoch difference.
						for (_, tranche) in pool.tranches.iter_mut() {
							tranche.accrue_interest(elapsed_secs);
						}

						// Step 2: Waterfall — split total pool value between tranches.
						// total_pool_value = oracle_nav + sum(treasury_liquidity across all tranches)
						// Invariant: the NAV oracle must be finalized before epoch settlement
						// begins (the Gateway refreshes it as the first step of the settlement
						// flow). This ensures oracle_nav reflects only outstanding loan value
						// and treasury_liquidity reflects only uninvested cash, with no overlap.
						let total_treasury: U256 = pool
							.tranches
							.values()
							.map(|t| t.treasury_liquidity())
							.fold(U256::zero(), |acc, v| acc.saturating_add(v));
						let total_pool_value = oracle_nav.saturating_add(total_treasury);
						let mut remaining = total_pool_value;

						// Senior tranches claim first (BTreeMap order); junior takes the residual.
						let mut tranche_navs: Vec<(TrancheId, U256)> = Vec::new();
						for (id, tranche) in pool.tranches.iter() {
							if let TrancheType::Senior { .. } = &tranche.tranche_type {
								let share = remaining.min(tranche.accrued_nav);
								remaining = remaining.saturating_sub(share);
								tranche_navs.push((id.clone(), share));
							}
						}
						for (id, tranche) in pool.tranches.iter() {
							if tranche.tranche_type.is_junior() {
								tranche_navs.push((id.clone(), remaining));
								remaining = U256::zero();
							}
						}

						// Step 3: Lock epoch_price for each tranche.
						for (id, nav) in &tranche_navs {
							if let Some(t) = pool.tranches.get_mut(id) {
								if t.epoch_price.is_none() {
									t.epoch_price = Some(t.token_price(*nav));
								}
							}
						}

						// Snapshot liquidity before deposit settlement so that freshly settled
						// deposits cannot immediately fund same-epoch redeem payouts.
						// BTreeMap gives O(log n) lookup in the redeem loop vs O(n²) with Vec::find.
						let pre_deposit_liquidity: BTreeMap<TrancheId, U256> = pool
							.tranches
							.iter()
							.map(|(id, t)| (id.clone(), t.treasury_liquidity()))
							.collect();

						if pool.deposit_settlement == SettlementMode::Automatic {
							for (tranche_id, tranche) in pool.tranches.iter_mut() {
								if !tranche.pending_orders.deposit.is_zero() {
									let epoch_price =
										tranche.epoch_price.unwrap_or(FixedU128::one());
									if let Ok(confirmed) = T::Investments::settle_deposit_orders(
										pool_id,
										tranche_id.clone(),
										pool.epoch.current_epoch,
										epoch_price,
									) {
										tranche.invested =
											tranche.invested.saturating_add(confirmed);
										tranche.pending_orders.deposit = U256::zero();
										// Senior accrued_nav grows by the newly settled deposit.
										if let TrancheType::Senior { .. } = &tranche.tranche_type {
											tranche.accrued_nav =
												tranche.accrued_nav.saturating_add(confirmed);
										}
									}
								}
							}
						}

						if pool.redeem_settlement == SettlementMode::Automatic {
							for (tranche_id, tranche) in pool.tranches.iter_mut() {
								let max_asset_payout = pre_deposit_liquidity
									.get(tranche_id)
									.copied()
									.unwrap_or_default();
								if !max_asset_payout.is_zero()
									&& !tranche.pending_orders.redeem.is_zero()
								{
									let epoch_price =
										tranche.epoch_price.unwrap_or(FixedU128::one());
									if let Ok((tokens_settled, asset_payout)) =
										T::Investments::settle_redeem_orders(
											pool_id,
											tranche_id.clone(),
											pool.epoch.current_epoch,
											max_asset_payout,
											epoch_price,
										) {
										tranche.invested =
											tranche.invested.saturating_sub(asset_payout);
										tranche.pending_orders.redeem = tranche
											.pending_orders
											.redeem
											.saturating_sub(tokens_settled);
										// Senior accrued_nav shrinks by the redeemed asset payout.
										if let TrancheType::Senior { .. } = &tranche.tranche_type {
											tranche.accrued_nav =
												tranche.accrued_nav.saturating_sub(asset_payout);
										}
									}
								}
							}
						}

						changed = true;
					}

					// Settlement window was entered (finalized or already done).
					// Next action: epoch end, when we need to advance.
					NextEpochAction::<T>::insert(
						pool_id,
						pool.epoch.epoch_start_secs.saturating_add(pool.epoch.epoch_length_secs),
					);
				}

				// Epoch over: reset prices and advance.
				if pool.epoch.should_advance(now_secs) {
					for (_, tranche) in pool.tranches.iter_mut() {
						tranche.epoch_price = None;
					}

					pool.epoch.advance(now_secs);
					let new_epoch = pool.epoch.current_epoch;
					changed = true;
					Self::deposit_event(Event::EpochAdvanced { pool_id, new_epoch });

					// Next action: when the settlement window opens for the new epoch.
					NextEpochAction::<T>::insert(pool_id, pool.epoch.settlement_start_secs());
				}

				if changed {
					Pools::<T>::insert(pool_id, pool);
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
		/// Must be called through the pools precompile — direct extrinsic submission is rejected.
		/// The precompile also dispatches a Gateway vault-deployment message to the Spoke chain,
		/// so bypassing it would leave the pool with no deployed vaults.
		///
		/// `pool_admin` must hold the `PoolAdmin` role for `pool_id` (granted by sudo in advance).
		/// `pool_id` is caller-specified; returns `PoolAlreadyExists` if already taken.
		/// `collaterals` must contain at least one NFT; each must not already be registered.
		/// `settlement_offset_secs` is how many seconds before epoch end the settlement window
		/// opens. During this window new orders are rejected.
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::from_parts(10_000, 0))]
		pub fn create_pool(
			origin: OriginFor<T>,
			pool_id: PoolId,
			pool_admin: T::AccountId,
			borrower: T::AccountId,
			collaterals: BoundedVec<CollateralAsset, ConstU32<MAX_COLLATERALS>>,
			epoch_length_secs: u64,
			settlement_offset_secs: u64,
			deposit_settlement: SettlementMode,
			redeem_settlement: SettlementMode,
			tranches: BoundedVec<TrancheInput, ConstU32<MAX_TRANCHES>>,
		) -> DispatchResult {
			T::PoolAdminOrigin::ensure_origin(origin)?;
			ensure!(T::Permissions::is_pool_admin(pool_id, &pool_admin), Error::<T>::Unauthorized);
			ensure!(!Pools::<T>::contains_key(pool_id), Error::<T>::PoolAlreadyExists);
			ensure!(!collaterals.is_empty(), Error::<T>::MissingCollateral);
			ensure!(
				settlement_offset_secs > 0 && settlement_offset_secs < epoch_length_secs,
				Error::<T>::InvalidSettlementOffset
			);

			let now_secs = T::Time::now().as_secs();

			for collateral in collaterals.iter() {
				ensure!(
					!Collaterals::<T>::contains_key(collateral),
					Error::<T>::CollateralAlreadyExists
				);
			}

			let mut built_tranches: BoundedBTreeMap<TrancheId, Tranche, ConstU32<MAX_TRANCHES>> =
				BoundedBTreeMap::new();
			for tranche in tranches.iter() {
				ensure!(
					!Tranches::<T>::contains_key(tranche.tranche_id.clone()),
					Error::<T>::TrancheAlreadyExists
				);
				let tranche_type = tranche
					.tranche_type
					.clone()
					.try_into_tranche_type()
					.ok_or(Error::<T>::InvalidRate)?;
				built_tranches
					.try_insert(
						tranche.tranche_id.clone(),
						Tranche {
							tranche_type,
							max_deposits: tranche.max_deposits,
							token_supply: U256::zero(),
							invested: U256::zero(),
							borrowed: U256::zero(),
							pending_orders: TranchePendingOrders::default(),
							epoch_price: None,
							accrued_nav: U256::zero(),
						},
					)
					.map_err(|_| Error::<T>::OutOfRange)?;
			}

			let pool = PoolDetails {
				tranches: built_tranches.clone(),
				epoch: EpochInfo::new(epoch_length_secs, settlement_offset_secs, now_secs),
				collaterals: collaterals.clone(),
				deposit_settlement,
				redeem_settlement,
			};

			for collateral in collaterals.iter() {
				Collaterals::<T>::insert(collateral, pool_id);
			}
			for tranche in tranches.iter() {
				Tranches::<T>::insert(tranche.tranche_id.clone(), pool_id);
			}
			// Initialise next action to the first settlement window open.
			let first_action = EpochInfo::new(epoch_length_secs, settlement_offset_secs, now_secs)
				.settlement_start_secs();
			NextEpochAction::<T>::insert(pool_id, first_action);
			Pools::<T>::insert(pool_id, pool);
			T::Permissions::grant_borrower(pool_id, borrower);

			Self::deposit_event(Event::PoolCreated { pool_id, epoch_length_secs });
			Ok(())
		}

		/// Register an ERC-7540 vault (chain_id + vault_address) to a tranche.
		///
		/// Caller must hold the `PoolAdmin` role for `pool_id`.
		/// Each tranche slot is created at pool creation; this call associates it with
		/// the deployed vault contract on the external EVM chain.
		#[pallet::call_index(1)]
		#[pallet::weight(Weight::from_parts(5_000, 0))]
		pub fn add_vault(
			origin: OriginFor<T>,
			pool_id: PoolId,
			tranche: TrancheInput,
		) -> DispatchResult {
			let caller = ensure_signed(origin)?;
			ensure!(T::Permissions::is_pool_admin(pool_id, &caller), Error::<T>::Unauthorized);

			ensure!(
				!Tranches::<T>::contains_key(tranche.tranche_id.clone()),
				Error::<T>::TrancheAlreadyExists
			);

			let tranche_type = tranche
				.tranche_type
				.clone()
				.try_into_tranche_type()
				.ok_or(Error::<T>::InvalidRate)?;
			Pools::<T>::try_mutate(pool_id, |maybe_pool| -> Result<(), DispatchError> {
				let pool = maybe_pool.as_mut().ok_or(Error::<T>::PoolNotFound)?;
				pool.tranches
					.try_insert(
						tranche.tranche_id.clone(),
						Tranche {
							tranche_type,
							max_deposits: tranche.max_deposits,
							token_supply: U256::zero(),
							invested: U256::zero(),
							borrowed: U256::zero(),
							pending_orders: TranchePendingOrders::default(),
							epoch_price: None,
							accrued_nav: U256::zero(),
						},
					)
					.map_err(|_| Error::<T>::OutOfRange)?;
				Ok(())
			})?;
			Self::deposit_event(Event::VaultAdded { pool_id, tranche_id: tranche.tranche_id });
			Ok(())
		}

		/// Called by the pools precompile (via Gateway) when a borrow request arrives.
		///
		/// Only callable through the Gateway origin — direct extrinsic calls are rejected.
		/// `borrower` must hold the Borrower role for `pool_id`; the address is forwarded
		/// from the originating EVM call so the pallet can verify it independently of the
		/// Gateway caller.
		/// Draws `amount` from the tranche treasury by incrementing `borrowed`.
		/// Fails if available liquidity (invested − borrowed) is less than `amount`.
		#[pallet::call_index(2)]
		#[pallet::weight(Weight::from_parts(10_000, 0))]
		pub fn borrow(
			origin: OriginFor<T>,
			pool_id: PoolId,
			chain_id: u64,
			vault_address: H160,
			borrower: T::AccountId,
			amount: U256,
		) -> DispatchResult {
			T::GatewayOrigin::ensure_origin(origin)?;
			ensure!(T::Permissions::is_borrower(pool_id, &borrower), Error::<T>::Unauthorized);
			ensure!(!amount.is_zero(), Error::<T>::ZeroAmount);

			let tranche_id = TrancheId { chain_id, vault_address };

			Pools::<T>::try_mutate(pool_id, |maybe_pool| -> Result<(), DispatchError> {
				let pool = maybe_pool.as_mut().ok_or(Error::<T>::PoolNotFound)?;
				let tranche =
					pool.tranches.get_mut(&tranche_id).ok_or(Error::<T>::TrancheNotFound)?;

				ensure!(
					tranche.treasury_liquidity() >= amount,
					Error::<T>::InsufficientTreasuryLiquidity
				);

				tranche.borrowed = tranche.borrowed.saturating_add(amount);
				let available = tranche.treasury_liquidity();

				Self::deposit_event(Event::Borrowed { pool_id, tranche_id, amount, available });
				Ok(())
			})
		}

		/// Called by the pools precompile (via Gateway) when a repay message arrives.
		///
		/// Only callable through the Gateway origin — direct extrinsic calls are rejected.
		/// `borrower` must hold the Borrower role for `pool_id`; the address is forwarded
		/// from the originating EVM call so the pallet can verify it independently of the
		/// Gateway caller.
		/// Reduces `borrowed` by `amount`, restoring tranche treasury liquidity.
		///
		/// `borrowed` saturates to zero — this is intentional. The borrower repays
		/// principal + accrued interest in a single transfer. Since `borrowed` only tracks
		/// the principal drawn, the interest portion of the repayment causes `borrowed` to
		/// underflow. The surplus USDC physically sits in the treasury and is distributed
		/// to investors through the NAV waterfall at the next epoch settlement.
		/// The Gateway contract is responsible for ensuring `amount` is backed by an actual
		/// USDC transfer before dispatching this extrinsic.
		#[pallet::call_index(3)]
		#[pallet::weight(Weight::from_parts(10_000, 0))]
		pub fn repay(
			origin: OriginFor<T>,
			pool_id: PoolId,
			chain_id: u64,
			vault_address: H160,
			borrower: T::AccountId,
			amount: U256,
		) -> DispatchResult {
			T::GatewayOrigin::ensure_origin(origin)?;
			ensure!(T::Permissions::is_borrower(pool_id, &borrower), Error::<T>::Unauthorized);
			ensure!(!amount.is_zero(), Error::<T>::ZeroAmount);

			let tranche_id = TrancheId { chain_id, vault_address };

			Pools::<T>::try_mutate(pool_id, |maybe_pool| -> Result<(), DispatchError> {
				let pool = maybe_pool.as_mut().ok_or(Error::<T>::PoolNotFound)?;
				let tranche =
					pool.tranches.get_mut(&tranche_id).ok_or(Error::<T>::TrancheNotFound)?;

				tranche.borrowed = tranche.borrowed.saturating_sub(amount);
				let available = tranche.treasury_liquidity();

				Self::deposit_event(Event::Repaid { pool_id, tranche_id, amount, available });
				Ok(())
			})
		}

		/// Update the on-chain Gateway EVM contract address (sudo only).
		///
		/// Both pool and investment precompiles read this address to enforce that only
		/// the Gateway contract can trigger pallet dispatch.
		#[pallet::call_index(4)]
		#[pallet::weight(Weight::from_parts(5_000, 0))]
		pub fn set_gateway(origin: OriginFor<T>, address: H160) -> DispatchResult {
			ensure_root(origin)?;
			GatewayAddress::<T>::put(address);
			Self::deposit_event(Event::GatewayUpdated { address });
			Ok(())
		}
	}
}
