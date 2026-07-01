mod impls;

use crate::{
	ApprovedDepositOrder, ApprovedRedeemOrder, ClaimableDepositOrder, ClaimableRedeemOrder,
	EpochId, OrderKey, PendingDepositOrder, PendingRedeemOrder, PermissionInspect, PoolId,
	PoolInspect, SettlementMode, TrancheId, TrancheMutate, WeightInfo, MAX_INVESTORS_PER_APPROVAL,
};

use frame_support::{pallet_prelude::*, traits::StorageVersion};
use frame_system::pallet_prelude::*;
use sp_core::U256;
use sp_std::collections::btree_set::BTreeSet;

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
		type Pools: PoolInspect + TrancheMutate<U256>;
		/// Permission inspector — implemented by pallet-permissions.
		/// Used to verify the Borrower role on `approve_deposit_orders` and
		/// `approve_redeem_orders`.
		type Permissions: PermissionInspect<Self::AccountId>;
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
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
		/// Caller does not hold the Borrower role for this pool.
		Unauthorized,
		/// Investor is not whitelisted as a TrancheInvestor for this tranche.
		NotWhitelisted,
		/// The orders list contains a duplicate (investor_id, epoch_id) pair.
		DuplicateOrderKey,
		/// No pending order exists for the given (investor, epoch) key.
		PendingOrderNotFound,
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
			investor_id: T::AccountId,
			amount: U256,
		},
		/// A redeem order was submitted and is pending epoch settlement.
		RedeemOrderSubmitted {
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor_id: T::AccountId,
			amount: U256,
		},
		/// Automatic mode: a pending deposit order was settled into `ClaimableDepositOrders`.
		/// The investor must send `requestTrancheClaim()` from the spoke to trigger minting.
		DepositOrderSettled {
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor_id: T::AccountId,
			amount: U256,
			shares_to_mint: U256,
		},
		/// Automatic mode: a pending redeem order was settled into `ClaimableRedeemOrders`.
		/// The investor must send `requestTrancheClaim()` from the spoke to trigger payout.
		RedeemOrderSettled {
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor_id: T::AccountId,
			shares_redeemed: U256,
			payout: U256,
		},
		/// Approval mode: a pending deposit order was approved into `ApprovedDepositOrders`.
		/// The Gateway smart contract observes this event and sends the mint instruction outbound.
		DepositOrderApproved {
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor_id: T::AccountId,
			amount: U256,
			shares_to_mint: U256,
		},
		/// Approval mode: a pending redeem order was approved into `ApprovedRedeemOrders`.
		/// The Gateway smart contract observes this event and sends the payout instruction outbound.
		RedeemOrderApproved {
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor_id: T::AccountId,
			shares_redeemed: U256,
			payout: U256,
		},
		/// Automatic mode: an investor claimed their settled deposit.
		/// Moved from `ClaimableDepositOrders` to `ApprovedDepositOrders`.
		/// The Gateway smart contract observes this event and sends the mint instruction outbound.
		DepositClaimed {
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor_id: T::AccountId,
			amount: U256,
			shares_to_mint: U256,
		},
		/// Automatic mode: an investor claimed their settled redemption.
		/// Moved from `ClaimableRedeemOrders` to `ApprovedRedeemOrders`.
		/// The Gateway smart contract observes this event and sends the payout instruction outbound.
		RedeemClaimed {
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor_id: T::AccountId,
			shares_redeemed: U256,
			payout: U256,
		},
	}

	// -----------------------------------------------------------------------
	// Storage
	// -----------------------------------------------------------------------

	/// Pending deposit orders awaiting epoch settlement.
	/// (tranche_id, investor_id, epoch_id) → order
	/// Top-ups in the same epoch accumulate at the same key; orders opened in later epochs
	/// get separate keys. `iter_prefix((tranche_id,))` covers the full tranche for settlement;
	/// `iter_prefix((tranche_id, investor_id))` covers all pending epochs for one investor.
	#[pallet::storage]
	pub type PendingDepositOrders<T: Config> = StorageNMap<
		_,
		(
			NMapKey<Blake2_128Concat, TrancheId>,
			NMapKey<Blake2_128Concat, T::AccountId>,
			NMapKey<Blake2_128Concat, EpochId>,
		),
		PendingDepositOrder,
	>;

	/// Claimable deposit orders awaiting investor pull-claim (Automatic mode).
	/// (tranche_id, investor_id, settlement_epoch_id) → order
	/// Written by `settle_deposit_orders`; cleared by `claim_shares`.
	#[pallet::storage]
	pub type ClaimableDepositOrders<T: Config> = StorageNMap<
		_,
		(
			NMapKey<Blake2_128Concat, TrancheId>,
			NMapKey<Blake2_128Concat, T::AccountId>,
			NMapKey<Blake2_128Concat, EpochId>,
		),
		ClaimableDepositOrder,
	>;

	/// Approved deposit orders ready for outbound mint.
	/// (tranche_id, investor_id, approval_epoch_id) → order
	/// Written by `approve_deposit_orders` (Approval) or `claim_shares` (Automatic).
	/// Each epoch gets a separate entry — no cross-epoch accumulation.
	#[pallet::storage]
	pub type ApprovedDepositOrders<T: Config> = StorageNMap<
		_,
		(
			NMapKey<Blake2_128Concat, TrancheId>,
			NMapKey<Blake2_128Concat, T::AccountId>,
			NMapKey<Blake2_128Concat, EpochId>,
		),
		ApprovedDepositOrder,
	>;

	/// Pending redeem orders awaiting epoch settlement.
	/// (tranche_id, investor_id, epoch_id) → order
	#[pallet::storage]
	pub type PendingRedeemOrders<T: Config> = StorageNMap<
		_,
		(
			NMapKey<Blake2_128Concat, TrancheId>,
			NMapKey<Blake2_128Concat, T::AccountId>,
			NMapKey<Blake2_128Concat, EpochId>,
		),
		PendingRedeemOrder,
	>;

	/// Claimable redeem orders awaiting investor pull-claim (Automatic mode).
	/// (tranche_id, investor_id, settlement_epoch_id) → order
	/// Written by `settle_redeem_orders`; cleared by `claim_assets`.
	#[pallet::storage]
	pub type ClaimableRedeemOrders<T: Config> = StorageNMap<
		_,
		(
			NMapKey<Blake2_128Concat, TrancheId>,
			NMapKey<Blake2_128Concat, T::AccountId>,
			NMapKey<Blake2_128Concat, EpochId>,
		),
		ClaimableRedeemOrder,
	>;

	/// Approved redeem orders ready for outbound payout.
	/// (tranche_id, investor_id, approval_epoch_id) → order
	/// Written by `approve_redeem_orders` (Approval) or `claim_assets` (Automatic).
	/// Each epoch gets a separate entry — no cross-epoch accumulation.
	#[pallet::storage]
	pub type ApprovedRedeemOrders<T: Config> = StorageNMap<
		_,
		(
			NMapKey<Blake2_128Concat, TrancheId>,
			NMapKey<Blake2_128Concat, T::AccountId>,
			NMapKey<Blake2_128Concat, EpochId>,
		),
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
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn submit_deposit_order(
			origin: OriginFor<T>,
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor_id: T::AccountId,
			amount: U256,
		) -> DispatchResult {
			T::GatewayOrigin::ensure_origin(origin)?;
			ensure!(
				T::Pools::tranche_exists(pool_id, tranche_id.clone()),
				Error::<T>::PoolOrTrancheNotFound
			);
			ensure!(
				T::Permissions::is_tranche_investor(&tranche_id, &investor_id),
				Error::<T>::NotWhitelisted
			);
			ensure!(!amount.is_zero(), Error::<T>::ZeroAmount);
			ensure!(!T::Pools::in_settlement_window(pool_id), Error::<T>::PoolInSettlementWindow);
			ensure!(
				!T::Pools::deposit_cap_exceeded(pool_id, tranche_id.clone(), amount),
				Error::<T>::DepositCapExceeded
			);

			let now = Self::current_block();
			let epoch_id = T::Pools::current_epoch(pool_id).unwrap_or_default();

			PendingDepositOrders::<T>::mutate(
				(tranche_id.clone(), investor_id.clone(), epoch_id),
				|entry| match entry {
					Some(existing) => {
						existing.amount = existing.amount.saturating_add(amount);
					},
					None => {
						*entry = Some(PendingDepositOrder { amount, epoch_id, submitted_at: now });
					},
				},
			);

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
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn submit_redeem_order(
			origin: OriginFor<T>,
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor_id: T::AccountId,
			amount: U256,
		) -> DispatchResult {
			T::GatewayOrigin::ensure_origin(origin)?;
			ensure!(
				T::Pools::tranche_exists(pool_id, tranche_id.clone()),
				Error::<T>::PoolOrTrancheNotFound
			);
			ensure!(
				T::Permissions::is_tranche_investor(&tranche_id, &investor_id),
				Error::<T>::NotWhitelisted
			);
			ensure!(!amount.is_zero(), Error::<T>::ZeroAmount);
			ensure!(!T::Pools::in_settlement_window(pool_id), Error::<T>::PoolInSettlementWindow);

			let now = Self::current_block();
			let epoch_id = T::Pools::current_epoch(pool_id).unwrap_or_default();

			PendingRedeemOrders::<T>::mutate(
				(tranche_id.clone(), investor_id.clone(), epoch_id),
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

		/// Approval mode: pool borrower approves a batch of pending deposit orders during
		/// the settlement window.
		///
		/// `borrower` must hold the Borrower role for `pool_id`; the address is forwarded
		/// from the originating EVM call so the pallet can verify it independently of the
		/// Gateway caller.
		/// Moves each investor's entry from `PendingDepositOrders` to `ApprovedDepositOrders`.
		/// Returns `PendingOrderNotFound` if any (investor, epoch) key has no pending order.
		/// The Gateway smart contract observes the `DepositOrderApproved` events and sends
		/// mint instructions to the spoke chain.
		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn approve_deposit_orders(
			origin: OriginFor<T>,
			pool_id: PoolId,
			tranche_id: TrancheId,
			borrower: T::AccountId,
			orders: BoundedVec<OrderKey<T::AccountId>, ConstU32<MAX_INVESTORS_PER_APPROVAL>>,
		) -> DispatchResult {
			T::GatewayOrigin::ensure_origin(origin)?;
			ensure!(T::Permissions::is_borrower(pool_id, &borrower), Error::<T>::Unauthorized);
			ensure!(T::Pools::pool_exists(pool_id), Error::<T>::PoolOrTrancheNotFound);
			ensure!(
				T::Pools::deposit_settlement_mode(pool_id) == Some(SettlementMode::Approval),
				Error::<T>::WrongSettlementMode
			);
			ensure!(T::Pools::in_settlement_window(pool_id), Error::<T>::NotInSettlementWindow);

			let epoch_price = T::Pools::epoch_price(pool_id, tranche_id.clone())
				.ok_or(Error::<T>::EpochPriceNotSet)?;

			let now = Self::current_block();

			let mut seen: BTreeSet<OrderKey<T::AccountId>> = BTreeSet::new();
			for key in orders.iter() {
				ensure!(seen.insert(key.clone()), Error::<T>::DuplicateOrderKey);
			}

			let mut total_approved = U256::zero();
			let mut total_shares_minted = U256::zero();

			for OrderKey { investor_id, epoch_id } in orders {
				let pending = PendingDepositOrders::<T>::take((
					tranche_id.clone(),
					investor_id.clone(),
					epoch_id,
				))
				.ok_or(Error::<T>::PendingOrderNotFound)?;

				let shares_to_mint = Self::assets_to_shares(pending.amount, epoch_price);

				ApprovedDepositOrders::<T>::insert(
					(tranche_id.clone(), investor_id.clone(), epoch_id),
					ApprovedDepositOrder {
						amount: pending.amount,
						shares_to_mint,
						epoch_id,
						approved_at: now,
					},
				);

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
				T::Pools::add_reserve(pool_id, tranche_id.clone(), total_approved)?;
				T::Pools::add_token_supply(pool_id, tranche_id.clone(), total_shares_minted)?;
				T::Pools::add_accrued_nav(pool_id, tranche_id, total_approved)?;
			}

			Ok(())
		}

		/// Approval mode: pool borrower approves a batch of pending redeem orders during
		/// the settlement window.
		///
		/// `borrower` must hold the Borrower role for `pool_id`; the address is forwarded
		/// from the originating EVM call so the pallet can verify it independently of the
		/// Gateway caller.
		/// Moves each investor's entry from `PendingRedeemOrders` to `ApprovedRedeemOrders`.
		/// Returns `PendingOrderNotFound` if any (investor, epoch) key has no pending order.
		/// The Gateway smart contract observes the `RedeemOrderApproved` events and sends
		/// payout instructions to the spoke chain.
		#[pallet::call_index(3)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn approve_redeem_orders(
			origin: OriginFor<T>,
			pool_id: PoolId,
			tranche_id: TrancheId,
			borrower: T::AccountId,
			orders: BoundedVec<OrderKey<T::AccountId>, ConstU32<MAX_INVESTORS_PER_APPROVAL>>,
		) -> DispatchResult {
			T::GatewayOrigin::ensure_origin(origin)?;
			ensure!(T::Permissions::is_borrower(pool_id, &borrower), Error::<T>::Unauthorized);
			ensure!(T::Pools::pool_exists(pool_id), Error::<T>::PoolOrTrancheNotFound);
			ensure!(
				T::Pools::redeem_settlement_mode(pool_id) == Some(SettlementMode::Approval),
				Error::<T>::WrongSettlementMode
			);
			ensure!(T::Pools::in_settlement_window(pool_id), Error::<T>::NotInSettlementWindow);

			let epoch_price = T::Pools::epoch_price(pool_id, tranche_id.clone())
				.ok_or(Error::<T>::EpochPriceNotSet)?;

			// Duplicate check — read-only, fires before any state mutation.
			let mut seen: BTreeSet<OrderKey<T::AccountId>> = BTreeSet::new();
			for key in orders.iter() {
				ensure!(seen.insert(key.clone()), Error::<T>::DuplicateOrderKey);
			}

			// Pre-flight liquidity check — errors on any missing key before any state mutation.
			let expected_total_payout = orders.iter().try_fold(
				U256::zero(),
				|acc, OrderKey { investor_id, epoch_id }| {
					PendingRedeemOrders::<T>::get((
						tranche_id.clone(),
						investor_id.clone(),
						*epoch_id,
					))
					.ok_or(Error::<T>::PendingOrderNotFound)
					.map(|p| acc.saturating_add(Self::shares_to_assets(p.amount, epoch_price)))
				},
			)?;
			ensure!(
				expected_total_payout <= T::Pools::reserve(pool_id, tranche_id.clone()),
				Error::<T>::InsufficientLiquidity
			);

			let now = Self::current_block();

			let mut total_tokens_approved = U256::zero();
			let mut total_payout = U256::zero();

			for OrderKey { investor_id, epoch_id } in orders {
				let pending = PendingRedeemOrders::<T>::take((
					tranche_id.clone(),
					investor_id.clone(),
					epoch_id,
				))
				.ok_or(Error::<T>::PendingOrderNotFound)?;

				let payout = Self::shares_to_assets(pending.amount, epoch_price);

				ApprovedRedeemOrders::<T>::insert(
					(tranche_id.clone(), investor_id.clone(), epoch_id),
					ApprovedRedeemOrder {
						shares_redeemed: pending.amount,
						payout,
						epoch_id,
						approved_at: now,
					},
				);

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
				T::Pools::sub_reserve(pool_id, tranche_id.clone(), total_payout)?;
				T::Pools::sub_accrued_nav(pool_id, tranche_id, total_payout)?;
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
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn claim_shares(
			origin: OriginFor<T>,
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor_id: T::AccountId,
		) -> DispatchResult {
			T::GatewayOrigin::ensure_origin(origin)?;
			ensure!(
				T::Pools::tranche_exists(pool_id, tranche_id.clone()),
				Error::<T>::PoolOrTrancheNotFound
			);
			ensure!(
				T::Permissions::is_tranche_investor(&tranche_id, &investor_id),
				Error::<T>::NotWhitelisted
			);

			let entries: sp_std::vec::Vec<(EpochId, ClaimableDepositOrder)> =
				ClaimableDepositOrders::<T>::iter_prefix((&tranche_id, &investor_id)).collect();

			ensure!(!entries.is_empty(), Error::<T>::NoClaimableDeposit);

			let total_amount =
				entries.iter().fold(U256::zero(), |acc, (_, o)| acc.saturating_add(o.amount));
			let total_shares = entries
				.iter()
				.fold(U256::zero(), |acc, (_, o)| acc.saturating_add(o.shares_to_mint));

			let _ = ClaimableDepositOrders::<T>::clear_prefix(
				(&tranche_id, &investor_id),
				entries.len() as u32,
				None,
			);

			let now = Self::current_block();
			let epoch_id = T::Pools::current_epoch(pool_id).unwrap_or_default();

			ApprovedDepositOrders::<T>::insert(
				(tranche_id.clone(), investor_id.clone(), epoch_id),
				ApprovedDepositOrder {
					amount: total_amount,
					shares_to_mint: total_shares,
					epoch_id,
					approved_at: now,
				},
			);

			Self::deposit_event(Event::DepositClaimed {
				pool_id,
				tranche_id,
				investor_id,
				amount: total_amount,
				shares_to_mint: total_shares,
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
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn claim_assets(
			origin: OriginFor<T>,
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor_id: T::AccountId,
		) -> DispatchResult {
			T::GatewayOrigin::ensure_origin(origin)?;
			ensure!(
				T::Pools::tranche_exists(pool_id, tranche_id.clone()),
				Error::<T>::PoolOrTrancheNotFound
			);
			ensure!(
				T::Permissions::is_tranche_investor(&tranche_id, &investor_id),
				Error::<T>::NotWhitelisted
			);

			let entries: sp_std::vec::Vec<(EpochId, ClaimableRedeemOrder)> =
				ClaimableRedeemOrders::<T>::iter_prefix((&tranche_id, &investor_id)).collect();

			ensure!(!entries.is_empty(), Error::<T>::NoClaimableRedeem);

			let total_shares_redeemed = entries
				.iter()
				.fold(U256::zero(), |acc, (_, o)| acc.saturating_add(o.shares_redeemed));
			let total_payout =
				entries.iter().fold(U256::zero(), |acc, (_, o)| acc.saturating_add(o.payout));

			let _ = ClaimableRedeemOrders::<T>::clear_prefix(
				(&tranche_id, &investor_id),
				entries.len() as u32,
				None,
			);

			let now = Self::current_block();
			let epoch_id = T::Pools::current_epoch(pool_id).unwrap_or_default();

			ApprovedRedeemOrders::<T>::insert(
				(tranche_id.clone(), investor_id.clone(), epoch_id),
				ApprovedRedeemOrder {
					shares_redeemed: total_shares_redeemed,
					payout: total_payout,
					epoch_id,
					approved_at: now,
				},
			);

			Self::deposit_event(Event::RedeemClaimed {
				pool_id,
				tranche_id,
				investor_id,
				shares_redeemed: total_shares_redeemed,
				payout: total_payout,
			});
			Ok(())
		}
	}
}
