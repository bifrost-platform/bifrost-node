mod impls;

use crate::{
	ApprovedDepositOrder, ApprovedRedeemOrder, ClaimableDepositOrder, ClaimableRedeemOrder,
	PendingDepositOrder, PendingRedeemOrder, PoolId, PoolInspect, SettlementMode, TrancheId,
	TrancheMutate, MAX_INVESTORS_PER_APPROVAL,
};

use frame_support::{pallet_prelude::*, traits::StorageVersion};
use frame_system::pallet_prelude::*;
use sp_core::{H160, U256};

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
		/// Only accepted origin for all investments extrinsics.
		/// Wire as `pallet_pools::EnsureGateway` in the runtime so that only the
		/// investments precompile (called by the Gateway contract) can invoke them.
		type GatewayOrigin: frame_support::traits::EnsureOrigin<Self::RuntimeOrigin>;
		/// Pool inspection and tranche mutation — implemented by pallet-pools.
		type Pools: PoolInspect<Self::AccountId> + TrancheMutate<U256>;
	}

	// -----------------------------------------------------------------------
	// Errors
	// -----------------------------------------------------------------------

	#[pallet::error]
	pub enum Error<T> {
		/// No pool exists with this ID, or the vault address is not registered to it.
		PoolOrTrancheNotFound,
		/// Amount must be greater than zero.
		ZeroAmount,
		/// New orders cannot be submitted while the pool is in its settlement window.
		PoolInSettlementWindow,
		/// This call is only valid during the pool's settlement window.
		NotInSettlementWindow,
		/// Deposit would push total invested + pending above the tranche's cap.
		DepositCapExceeded,
		/// Tranche treasury has no available liquidity to cover redemptions.
		InsufficientLiquidity,
		/// Settlement window is open but NAV has not been finalized for this epoch yet.
		EpochPriceNotSet,
		/// No claimable deposit order found for this investor (Automatic mode only).
		NoClaimableDeposit,
		/// No claimable redeem order found for this investor (Automatic mode only).
		NoClaimableRedeem,
		/// Approval extrinsic called on a pool that is not in Approval settlement mode.
		WrongSettlementMode,
	}

	// -----------------------------------------------------------------------
	// Events
	// -----------------------------------------------------------------------

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A deposit order was submitted and is pending epoch settlement.
		DepositOrderSubmitted {
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor_id: H160,
			amount: U256,
		},
		/// A redeem order was submitted and is pending epoch settlement.
		RedeemOrderSubmitted {
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor_id: H160,
			amount: U256,
		},
		/// Automatic mode: a pending deposit order was settled into `ClaimableDepositOrders`.
		/// The investor must send `requestTrancheClaim()` from the spoke to trigger minting.
		DepositOrderSettled {
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor_id: H160,
			amount: U256,
			shares_to_mint: U256,
		},
		/// Automatic mode: a pending redeem order was settled into `ClaimableRedeemOrders`.
		/// The investor must send `requestTrancheClaim()` from the spoke to trigger payout.
		RedeemOrderSettled {
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor_id: H160,
			shares_redeemed: U256,
			payout: U256,
		},
		/// Approval mode: a pending deposit order was approved into `ApprovedDepositOrders`.
		/// The Gateway smart contract observes this event and sends the mint instruction outbound.
		DepositOrderApproved {
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor_id: H160,
			amount: U256,
			shares_to_mint: U256,
		},
		/// Approval mode: a pending redeem order was approved into `ApprovedRedeemOrders`.
		/// The Gateway smart contract observes this event and sends the payout instruction outbound.
		RedeemOrderApproved {
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor_id: H160,
			shares_redeemed: U256,
			payout: U256,
		},
		/// Automatic mode: an investor claimed their settled deposit.
		/// Moved from `ClaimableDepositOrders` to `ApprovedDepositOrders`.
		/// The Gateway smart contract observes this event and sends the mint instruction outbound.
		DepositClaimed {
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor_id: H160,
			amount: U256,
			shares_to_mint: U256,
		},
		/// Automatic mode: an investor claimed their settled redemption.
		/// Moved from `ClaimableRedeemOrders` to `ApprovedRedeemOrders`.
		/// The Gateway smart contract observes this event and sends the payout instruction outbound.
		RedeemClaimed {
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor_id: H160,
			shares_redeemed: U256,
			payout: U256,
		},
	}

	// -----------------------------------------------------------------------
	// Storage
	// -----------------------------------------------------------------------

	/// Pending deposit orders awaiting epoch settlement.
	/// tranche_id → investor → order (amount + epoch/block metadata)
	#[pallet::storage]
	pub type PendingDepositOrders<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		TrancheId,
		Blake2_128Concat,
		H160,
		PendingDepositOrder,
	>;

	/// Claimable deposit orders awaiting investor pull-claim (Automatic mode).
	/// Written by `settle_deposit_orders` during `on_initialize`.
	/// Cleared by `claim_shares` when the investor initiates from the spoke.
	/// tranche_id → investor_id → order (shares-to-mint + epoch/block metadata)
	#[pallet::storage]
	pub type ClaimableDepositOrders<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		TrancheId,
		Blake2_128Concat,
		H160,
		ClaimableDepositOrder,
	>;

	/// Approved deposit orders ready for outbound mint.
	/// Written by `approve_deposit_orders` (Approval mode) or `claim_shares` (Automatic mode).
	/// tranche_id → investor_id → order (shares-to-mint + epoch/block metadata)
	#[pallet::storage]
	pub type ApprovedDepositOrders<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		TrancheId,
		Blake2_128Concat,
		H160,
		ApprovedDepositOrder,
	>;

	/// Pending redeem orders awaiting epoch settlement.
	/// tranche_id → investor → order (amount + epoch/block metadata)
	#[pallet::storage]
	pub type PendingRedeemOrders<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		TrancheId,
		Blake2_128Concat,
		H160,
		PendingRedeemOrder,
	>;

	/// Claimable redeem orders awaiting investor pull-claim (Automatic mode).
	/// Written by `settle_redeem_orders` during `on_initialize`.
	/// Cleared by `claim_assets` when the investor initiates from the spoke.
	/// tranche_id → investor_id → order (payout + epoch/block metadata)
	#[pallet::storage]
	pub type ClaimableRedeemOrders<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		TrancheId,
		Blake2_128Concat,
		H160,
		ClaimableRedeemOrder,
	>;

	/// Approved redeem orders ready for outbound payout.
	/// Written by `approve_redeem_orders` (Approval mode) or `claim_assets` (Automatic mode).
	/// tranche_id → investor_id → order (payout + epoch/block metadata)
	#[pallet::storage]
	pub type ApprovedRedeemOrders<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		TrancheId,
		Blake2_128Concat,
		H160,
		ApprovedRedeemOrder,
	>;

	// -----------------------------------------------------------------------
	// Extrinsics
	// -----------------------------------------------------------------------

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Entry point called by the investments precompile when a `requestDeposit`
		/// message arrives on Bifrost via CCCP.
		///
		/// Rejected during the pool's settlement window.
		/// Updates the tranche's aggregate pending deposit total.
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::from_parts(10_000, 0))]
		pub fn submit_deposit_order(
			origin: OriginFor<T>,
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor_id: H160,
			amount: U256,
		) -> DispatchResult {
			T::GatewayOrigin::ensure_origin(origin)?;
			ensure!(
				T::Pools::tranche_exists(pool_id, tranche_id.clone()),
				Error::<T>::PoolOrTrancheNotFound
			);
			ensure!(!amount.is_zero(), Error::<T>::ZeroAmount);
			ensure!(!T::Pools::in_settlement_window(pool_id), Error::<T>::PoolInSettlementWindow);
			ensure!(
				!T::Pools::deposit_cap_exceeded(pool_id, tranche_id.clone(), amount),
				Error::<T>::DepositCapExceeded
			);

			let now = Self::current_block();
			let epoch_id = T::Pools::current_epoch(pool_id).unwrap_or_default();

			PendingDepositOrders::<T>::mutate(tranche_id.clone(), investor_id, |entry| {
				match entry {
					Some(existing) => {
						// Accumulate amount; preserve original epoch/block metadata.
						existing.amount = existing.amount.saturating_add(amount);
					},
					None => {
						*entry = Some(PendingDepositOrder { amount, epoch_id, submitted_at: now });
					},
				}
			});

			T::Pools::add_pending_deposit(pool_id, tranche_id.clone(), amount)?;

			Self::deposit_event(Event::DepositOrderSubmitted {
				pool_id,
				tranche_id,
				investor_id,
				amount,
			});
			Ok(())
		}

		/// Entry point called by the investments precompile when a `requestRedeem`
		/// message arrives on Bifrost via CCCP.
		///
		/// Tranche tokens are burned on the Spoke chain when the request is submitted,
		/// so `token_supply` is decremented here immediately.
		/// Rejected during the pool's settlement window.
		#[pallet::call_index(1)]
		#[pallet::weight(Weight::from_parts(10_000, 0))]
		pub fn submit_redeem_order(
			origin: OriginFor<T>,
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor_id: H160,
			amount: U256,
		) -> DispatchResult {
			T::GatewayOrigin::ensure_origin(origin)?;
			ensure!(
				T::Pools::tranche_exists(pool_id, tranche_id.clone()),
				Error::<T>::PoolOrTrancheNotFound
			);
			ensure!(!amount.is_zero(), Error::<T>::ZeroAmount);
			ensure!(!T::Pools::in_settlement_window(pool_id), Error::<T>::PoolInSettlementWindow);

			let now = Self::current_block();
			let epoch_id = T::Pools::current_epoch(pool_id).unwrap_or_default();

			PendingRedeemOrders::<T>::mutate(
				tranche_id.clone(),
				investor_id,
				|entry| match entry {
					Some(existing) => {
						existing.amount = existing.amount.saturating_add(amount);
					},
					None => {
						*entry = Some(PendingRedeemOrder { amount, epoch_id, submitted_at: now });
					},
				},
			);

			T::Pools::add_pending_redeem(pool_id, tranche_id.clone(), amount)?;
			// Tokens were burned on the spoke chain at request time.
			T::Pools::sub_token_supply(pool_id, tranche_id.clone(), amount)?;

			Self::deposit_event(Event::RedeemOrderSubmitted {
				pool_id,
				tranche_id,
				investor_id,
				amount,
			});
			Ok(())
		}

		/// Approval mode: pool admin approves a batch of pending deposit orders during
		/// the settlement window.
		///
		/// Moves each investor's entry from `PendingDepositOrders` to `ApprovedDepositOrders`.
		/// Investors without a pending order are silently skipped.
		/// The Gateway smart contract observes the `DepositOrderApproved` events and sends
		/// mint instructions to the spoke chain.
		#[pallet::call_index(2)]
		#[pallet::weight(Weight::from_parts(10_000, 0).saturating_mul(investor_ids.len() as u64))]
		pub fn approve_deposit_orders(
			origin: OriginFor<T>,
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor_ids: BoundedVec<H160, ConstU32<MAX_INVESTORS_PER_APPROVAL>>,
		) -> DispatchResult {
			T::GatewayOrigin::ensure_origin(origin)?;
			ensure!(T::Pools::pool_exists(pool_id), Error::<T>::PoolOrTrancheNotFound);
			ensure!(
				T::Pools::deposit_settlement_mode(pool_id) == Some(SettlementMode::Approval),
				Error::<T>::WrongSettlementMode
			);
			ensure!(T::Pools::in_settlement_window(pool_id), Error::<T>::NotInSettlementWindow);

			let epoch_price = T::Pools::epoch_price(pool_id, tranche_id.clone())
				.ok_or(Error::<T>::EpochPriceNotSet)?;

			let now = Self::current_block();
			let epoch_id = T::Pools::current_epoch(pool_id).unwrap_or_default();

			let mut total_approved = U256::zero();
			let mut total_shares_minted = U256::zero();

			for investor_id in investor_ids {
				let Some(pending) =
					PendingDepositOrders::<T>::take(tranche_id.clone(), investor_id)
				else {
					continue;
				};

				let shares_to_mint = Self::assets_to_shares(pending.amount, epoch_price);

				ApprovedDepositOrders::<T>::mutate(tranche_id.clone(), investor_id, |entry| {
					match entry {
						Some(existing) => {
							existing.amount = existing.amount.saturating_add(pending.amount);
							existing.shares_to_mint =
								existing.shares_to_mint.saturating_add(shares_to_mint);
							existing.epoch_id = epoch_id;
							existing.approved_at = now;
						},
						None => {
							*entry = Some(ApprovedDepositOrder {
								amount: pending.amount,
								shares_to_mint,
								epoch_id,
								approved_at: now,
							});
						},
					}
				});

				total_approved = total_approved.saturating_add(pending.amount);
				total_shares_minted = total_shares_minted.saturating_add(shares_to_mint);

				Self::deposit_event(Event::DepositOrderApproved {
					pool_id,
					tranche_id: tranche_id.clone(),
					investor_id,
					amount: pending.amount,
					shares_to_mint,
				});
			}

			if !total_approved.is_zero() {
				T::Pools::sub_pending_deposit(pool_id, tranche_id.clone(), total_approved)?;
				T::Pools::add_invested(pool_id, tranche_id.clone(), total_approved)?;
				T::Pools::add_token_supply(pool_id, tranche_id, total_shares_minted)?;
			}

			Ok(())
		}

		/// Approval mode: pool admin approves a batch of pending redeem orders during
		/// the settlement window.
		///
		/// Moves each investor's entry from `PendingRedeemOrders` to `ApprovedRedeemOrders`.
		/// Investors without a pending order are silently skipped.
		/// The Gateway smart contract observes the `RedeemOrderApproved` events and sends
		/// payout instructions to the spoke chain.
		#[pallet::call_index(3)]
		#[pallet::weight(Weight::from_parts(10_000, 0).saturating_mul(investor_ids.len() as u64))]
		pub fn approve_redeem_orders(
			origin: OriginFor<T>,
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor_ids: BoundedVec<H160, ConstU32<MAX_INVESTORS_PER_APPROVAL>>,
		) -> DispatchResult {
			T::GatewayOrigin::ensure_origin(origin)?;
			ensure!(T::Pools::pool_exists(pool_id), Error::<T>::PoolOrTrancheNotFound);
			ensure!(
				T::Pools::redeem_settlement_mode(pool_id) == Some(SettlementMode::Approval),
				Error::<T>::WrongSettlementMode
			);
			ensure!(T::Pools::in_settlement_window(pool_id), Error::<T>::NotInSettlementWindow);

			let epoch_price = T::Pools::epoch_price(pool_id, tranche_id.clone())
				.ok_or(Error::<T>::EpochPriceNotSet)?;

			// Validate aggregate payout before mutating state — sub_invested uses saturating
			// arithmetic, so the check must cover the full batch.
			let expected_total_payout =
				investor_ids.iter().fold(U256::zero(), |acc, investor_id| {
					PendingRedeemOrders::<T>::get(tranche_id.clone(), *investor_id)
						.map_or(acc, |p| {
							acc.saturating_add(Self::shares_to_assets(p.amount, epoch_price))
						})
				});
			ensure!(
				expected_total_payout <= T::Pools::treasury_liquidity(pool_id, tranche_id.clone()),
				Error::<T>::InsufficientLiquidity
			);

			let now = Self::current_block();
			let epoch_id = T::Pools::current_epoch(pool_id).unwrap_or_default();

			let mut total_tokens_approved = U256::zero();
			let mut total_payout = U256::zero();

			for investor_id in investor_ids {
				let Some(pending) = PendingRedeemOrders::<T>::take(tranche_id.clone(), investor_id)
				else {
					continue;
				};

				let payout = Self::shares_to_assets(pending.amount, epoch_price);

				ApprovedRedeemOrders::<T>::mutate(tranche_id.clone(), investor_id, |entry| {
					match entry {
						Some(existing) => {
							existing.shares_redeemed =
								existing.shares_redeemed.saturating_add(pending.amount);
							existing.payout = existing.payout.saturating_add(payout);
							existing.epoch_id = epoch_id;
							existing.approved_at = now;
						},
						None => {
							*entry = Some(ApprovedRedeemOrder {
								shares_redeemed: pending.amount,
								payout,
								epoch_id,
								approved_at: now,
							});
						},
					}
				});

				total_tokens_approved = total_tokens_approved.saturating_add(pending.amount);
				total_payout = total_payout.saturating_add(payout);

				Self::deposit_event(Event::RedeemOrderApproved {
					pool_id,
					tranche_id: tranche_id.clone(),
					investor_id,
					shares_redeemed: pending.amount,
					payout,
				});
			}

			if !total_tokens_approved.is_zero() {
				T::Pools::sub_pending_redeem(pool_id, tranche_id.clone(), total_tokens_approved)?;
				T::Pools::sub_invested(pool_id, tranche_id, total_payout)?;
			}

			Ok(())
		}

		/// Automatic mode: process an investor's pull-claim for a settled deposit order.
		///
		/// Called by the investments precompile when a `requestTrancheClaim()` message
		/// for a deposit arrives from the spoke chain via CCCP.
		/// Moves the entry from `ClaimableDepositOrders` to `ApprovedDepositOrders` and
		/// emits `DepositClaimed` — the Gateway smart contract then sends the mint instruction.
		#[pallet::call_index(4)]
		#[pallet::weight(Weight::from_parts(10_000, 0))]
		pub fn claim_shares(
			origin: OriginFor<T>,
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor_id: H160,
		) -> DispatchResult {
			T::GatewayOrigin::ensure_origin(origin)?;
			ensure!(
				T::Pools::tranche_exists(pool_id, tranche_id.clone()),
				Error::<T>::PoolOrTrancheNotFound
			);

			let claimable = ClaimableDepositOrders::<T>::take(tranche_id.clone(), investor_id)
				.ok_or(Error::<T>::NoClaimableDeposit)?;

			let now = Self::current_block();

			ApprovedDepositOrders::<T>::mutate(
				tranche_id.clone(),
				investor_id,
				|entry| match entry {
					Some(existing) => {
						existing.amount = existing.amount.saturating_add(claimable.amount);
						existing.shares_to_mint =
							existing.shares_to_mint.saturating_add(claimable.shares_to_mint);
						existing.epoch_id = claimable.epoch_id;
						existing.approved_at = now;
					},
					None => {
						*entry = Some(ApprovedDepositOrder {
							amount: claimable.amount,
							shares_to_mint: claimable.shares_to_mint,
							epoch_id: claimable.epoch_id,
							approved_at: now,
						});
					},
				},
			);

			Self::deposit_event(Event::DepositClaimed {
				pool_id,
				tranche_id,
				investor_id,
				amount: claimable.amount,
				shares_to_mint: claimable.shares_to_mint,
			});
			Ok(())
		}

		/// Automatic mode: process an investor's pull-claim for a settled redeem order.
		///
		/// Called by the investments precompile when a `requestTrancheClaim()` message
		/// for a redemption arrives from the spoke chain via CCCP.
		/// Moves the entry from `ClaimableRedeemOrders` to `ApprovedRedeemOrders` and
		/// emits `RedeemClaimed` — the Gateway smart contract then sends the payout instruction.
		#[pallet::call_index(5)]
		#[pallet::weight(Weight::from_parts(10_000, 0))]
		pub fn claim_assets(
			origin: OriginFor<T>,
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor_id: H160,
		) -> DispatchResult {
			T::GatewayOrigin::ensure_origin(origin)?;
			ensure!(
				T::Pools::tranche_exists(pool_id, tranche_id.clone()),
				Error::<T>::PoolOrTrancheNotFound
			);

			let claimable = ClaimableRedeemOrders::<T>::take(tranche_id.clone(), investor_id)
				.ok_or(Error::<T>::NoClaimableRedeem)?;

			let now = Self::current_block();

			ApprovedRedeemOrders::<T>::mutate(
				tranche_id.clone(),
				investor_id,
				|entry| match entry {
					Some(existing) => {
						existing.shares_redeemed =
							existing.shares_redeemed.saturating_add(claimable.shares_redeemed);
						existing.payout = existing.payout.saturating_add(claimable.payout);
						existing.epoch_id = claimable.epoch_id;
						existing.approved_at = now;
					},
					None => {
						*entry = Some(ApprovedRedeemOrder {
							shares_redeemed: claimable.shares_redeemed,
							payout: claimable.payout,
							epoch_id: claimable.epoch_id,
							approved_at: now,
						});
					},
				},
			);

			Self::deposit_event(Event::RedeemClaimed {
				pool_id,
				tranche_id,
				investor_id,
				shares_redeemed: claimable.shares_redeemed,
				payout: claimable.payout,
			});
			Ok(())
		}
	}
}
