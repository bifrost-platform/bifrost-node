mod impls;

use crate::{PoolId, PoolInspect, TrancheId};

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
		/// Pool inspection — implemented by pallet-pools.
		type Pools: PoolInspect<Self::AccountId>;
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
		/// Caller is not the pool admin.
		NotPoolAdmin,
		/// No pending order found for this investor.
		NoPendingOrder,
	}

	// -----------------------------------------------------------------------
	// Events
	// -----------------------------------------------------------------------

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An invest order was submitted and is pending epoch settlement.
		InvestOrderSubmitted {
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor: H160,
			amount: U256,
		},
		/// A redeem order was submitted and is pending epoch settlement.
		RedeemOrderSubmitted {
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor: H160,
			amount: U256,
		},
		/// An investor's pending invest order was moved to confirmed.
		/// Off-chain bot watches this and mints tranche tokens on the external chain.
		InvestOrderConfirmed {
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor: H160,
			amount: U256,
		},
		/// An investor's pending redeem order was moved to confirmed.
		RedeemOrderConfirmed {
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor: H160,
			tokens: U256,
		},
		/// Confirmed redeem orders were executed.
		/// Off-chain bot reads this and distributes `usdc_amount` from the Spoke
		/// Treasury to each investor proportional to their confirmed token amount.
		RedeemOrdersExecuted {
			pool_id: PoolId,
			tranche_id: TrancheId,
			/// Total tranche tokens redeemed.
			total_tokens: U256,
			/// USDC amount the borrower deposited to the Spoke Treasury.
			usdc_amount: U256,
		},
	}

	// -----------------------------------------------------------------------
	// Storage
	// -----------------------------------------------------------------------

	/// Pending invest orders awaiting epoch settlement.
	/// tranche_id → investor → cumulative USDC amount
	#[pallet::storage]
	pub type PendingInvestOrders<T: Config> =
		StorageDoubleMap<_, Blake2_128Concat, TrancheId, Blake2_128Concat, H160, U256>;

	/// Confirmed invest orders ready for off-chain mint.
	/// Written by `approve_invest_order` (Approval) or `on_initialize` (Automatic).
	/// Cleared by the off-chain settlement bot once tokens are minted.
	/// tranche_id → investor → USDC amount
	#[pallet::storage]
	pub type ConfirmedInvestOrders<T: Config> =
		StorageDoubleMap<_, Blake2_128Concat, TrancheId, Blake2_128Concat, H160, U256>;

	/// Pending redeem orders awaiting epoch settlement.
	/// tranche_id → investor → cumulative tranche token amount
	#[pallet::storage]
	pub type PendingRedeemOrders<T: Config> =
		StorageDoubleMap<_, Blake2_128Concat, TrancheId, Blake2_128Concat, H160, U256>;

	/// Confirmed redeem orders ready for `execute_redeem_orders`.
	/// Written by `approve_redeem_order` (Approval mode).
	/// Cleared when the borrower calls `execute_redeem_orders`.
	/// tranche_id → investor → tranche token amount
	#[pallet::storage]
	pub type ConfirmedRedeemOrders<T: Config> =
		StorageDoubleMap<_, Blake2_128Concat, TrancheId, Blake2_128Concat, H160, U256>;

	// -----------------------------------------------------------------------
	// Extrinsics
	// -----------------------------------------------------------------------

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Entry point called by the investments precompile when a `requestDeposit`
		/// message arrives on Bifrost via CCCP.
		///
		/// Rejected during the pool's settlement window.
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::from_parts(10_000, 0))]
		pub fn submit_invest_order(
			origin: OriginFor<T>,
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor: H160,
			amount: U256,
		) -> DispatchResult {
			ensure_signed(origin)?;
			ensure!(
				T::Pools::tranche_exists(pool_id, tranche_id.clone()),
				Error::<T>::PoolOrTrancheNotFound
			);
			ensure!(!amount.is_zero(), Error::<T>::ZeroAmount);
			ensure!(!T::Pools::in_settlement_window(pool_id), Error::<T>::PoolInSettlementWindow);

			PendingInvestOrders::<T>::mutate(tranche_id.clone(), investor, |entry| {
				*entry = Some(entry.unwrap_or_default().saturating_add(amount));
			});

			Self::deposit_event(Event::InvestOrderSubmitted {
				pool_id,
				tranche_id,
				investor,
				amount,
			});
			Ok(())
		}

		/// Entry point called by the investments precompile when a `requestRedeem`
		/// message arrives on Bifrost via CCCP.
		///
		/// Rejected during the pool's settlement window.
		#[pallet::call_index(1)]
		#[pallet::weight(Weight::from_parts(10_000, 0))]
		pub fn submit_redeem_order(
			origin: OriginFor<T>,
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor: H160,
			amount: U256,
		) -> DispatchResult {
			ensure_signed(origin)?;
			ensure!(
				T::Pools::tranche_exists(pool_id, tranche_id.clone()),
				Error::<T>::PoolOrTrancheNotFound
			);
			ensure!(!amount.is_zero(), Error::<T>::ZeroAmount);
			ensure!(!T::Pools::in_settlement_window(pool_id), Error::<T>::PoolInSettlementWindow);

			PendingRedeemOrders::<T>::mutate(tranche_id.clone(), investor, |entry| {
				*entry = Some(entry.unwrap_or_default().saturating_add(amount));
			});

			Self::deposit_event(Event::RedeemOrderSubmitted {
				pool_id,
				tranche_id,
				investor,
				amount,
			});
			Ok(())
		}

		/// Pool admin approves a specific investor's pending invest order during the
		/// settlement window (Approval mode).
		///
		/// Moves the investor's entry from `PendingInvestOrders` to
		/// `ConfirmedInvestOrders`. The off-chain settlement bot watches for
		/// `InvestOrderConfirmed` and mints tranche tokens on the external chain.
		#[pallet::call_index(2)]
		#[pallet::weight(Weight::from_parts(10_000, 0))]
		pub fn approve_invest_order(
			origin: OriginFor<T>,
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor: H160,
		) -> DispatchResult {
			let caller = ensure_signed(origin)?;
			let admin = T::Pools::pool_admin(pool_id).ok_or(Error::<T>::PoolOrTrancheNotFound)?;
			ensure!(caller == admin, Error::<T>::NotPoolAdmin);
			ensure!(T::Pools::in_settlement_window(pool_id), Error::<T>::NotInSettlementWindow);

			let amount = PendingInvestOrders::<T>::take(tranche_id.clone(), investor)
				.ok_or(Error::<T>::NoPendingOrder)?;

			ConfirmedInvestOrders::<T>::mutate(tranche_id.clone(), investor, |entry| {
				*entry = Some(entry.unwrap_or_default().saturating_add(amount));
			});

			Self::deposit_event(Event::InvestOrderConfirmed {
				pool_id,
				tranche_id,
				investor,
				amount,
			});
			Ok(())
		}

		/// Pool admin approves a specific investor's pending redeem order during the
		/// settlement window (Approval mode).
		///
		/// Moves the investor's entry from `PendingRedeemOrders` to
		/// `ConfirmedRedeemOrders`. The borrower then calls `execute_redeem_orders`
		/// to settle them.
		#[pallet::call_index(3)]
		#[pallet::weight(Weight::from_parts(10_000, 0))]
		pub fn approve_redeem_order(
			origin: OriginFor<T>,
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor: H160,
		) -> DispatchResult {
			let caller = ensure_signed(origin)?;
			let admin = T::Pools::pool_admin(pool_id).ok_or(Error::<T>::PoolOrTrancheNotFound)?;
			ensure!(caller == admin, Error::<T>::NotPoolAdmin);
			ensure!(T::Pools::in_settlement_window(pool_id), Error::<T>::NotInSettlementWindow);

			let tokens = PendingRedeemOrders::<T>::take(tranche_id.clone(), investor)
				.ok_or(Error::<T>::NoPendingOrder)?;

			ConfirmedRedeemOrders::<T>::mutate(tranche_id.clone(), investor, |entry| {
				*entry = Some(entry.unwrap_or_default().saturating_add(tokens));
			});

			Self::deposit_event(Event::RedeemOrderConfirmed {
				pool_id,
				tranche_id,
				investor,
				tokens,
			});
			Ok(())
		}

		/// Called by the borrower (via precompile) during the settlement window to
		/// settle all confirmed redeem orders for a tranche.
		///
		/// `usdc_amount` is the USDC the borrower deposited to the Spoke Treasury
		/// to cover these redemptions. The off-chain bot reads the
		/// `RedeemOrdersExecuted` event and distributes USDC from the Spoke Treasury
		/// to each investor proportional to their confirmed token balance.
		///
		/// Drains all `ConfirmedRedeemOrders` for the tranche.
		#[pallet::call_index(4)]
		#[pallet::weight(Weight::from_parts(20_000, 0))]
		pub fn execute_redeem_orders(
			origin: OriginFor<T>,
			pool_id: PoolId,
			tranche_id: TrancheId,
			usdc_amount: U256,
		) -> DispatchResult {
			ensure_signed(origin)?;
			ensure!(
				T::Pools::tranche_exists(pool_id, tranche_id.clone()),
				Error::<T>::PoolOrTrancheNotFound
			);
			ensure!(T::Pools::in_settlement_window(pool_id), Error::<T>::NotInSettlementWindow);
			ensure!(!usdc_amount.is_zero(), Error::<T>::ZeroAmount);

			let total_tokens = Self::drain_confirmed_redeem(tranche_id.clone());

			Self::deposit_event(Event::RedeemOrdersExecuted {
				pool_id,
				tranche_id,
				total_tokens,
				usdc_amount,
			});
			Ok(())
		}
	}
}
