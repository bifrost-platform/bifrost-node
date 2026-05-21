mod impls;

use crate::{PoolId, PoolInspect, TrancheId, TrancheMutate};

use frame_support::{pallet_prelude::*, traits::StorageVersion};
use frame_system::pallet_prelude::*;
use sp_core::{H160, U256};

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	const MAX_INVESTORS_PER_APPROVAL: u32 = 100;

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
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
		/// Caller is not the pool's authorized borrower.
		NotBorrower,
		/// Deposit would push total invested + pending above the tranche's cap.
		DepositCapExceeded,
		/// Tranche treasury has no available liquidity to cover redemptions.
		InsufficientLiquidity,
		/// Settlement window is open but NAV has not been finalized for this epoch yet.
		EpochPriceNotSet,
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
		/// An investor's pending deposit order was moved to confirmed.
		/// Off-chain bot watches this event and mints `tokens_to_mint` tranche tokens on the external chain.
		DepositOrderApproved {
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor_id: H160,
			usdc_amount: U256,
			tokens_to_mint: U256,
		},
		/// An investor's pending redeem order was moved to confirmed.
		/// Off-chain bot watches this event and distributes `shares_to_distribute` to the investor.
		RedeemOrderApproved {
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor_id: H160,
			tokens: U256,
			shares_to_distribute: U256,
		},
	}

	// -----------------------------------------------------------------------
	// Storage
	// -----------------------------------------------------------------------

	/// Pending deposit orders awaiting epoch settlement.
	/// tranche_id → investor → cumulative USDC amount
	#[pallet::storage]
	pub type PendingDepositOrders<T: Config> =
		StorageDoubleMap<_, Blake2_128Concat, TrancheId, Blake2_128Concat, H160, U256>;

	/// Approved deposit orders ready for off-chain mint.
	/// Written by `approve_deposit_orders` (Approval) or `on_initialize` (Automatic).
	/// Cleared by the poll-based claim flow once tokens are minted on the Spoke chain.
	/// tranche_id → investor_id → tokens-to-mint (converted from USDC at epoch_price)
	#[pallet::storage]
	pub type ApprovedDepositOrders<T: Config> =
		StorageDoubleMap<_, Blake2_128Concat, TrancheId, Blake2_128Concat, H160, U256>;

	/// Pending redeem orders awaiting epoch settlement.
	/// tranche_id → investor → cumulative tranche token amount
	#[pallet::storage]
	pub type PendingRedeemOrders<T: Config> =
		StorageDoubleMap<_, Blake2_128Concat, TrancheId, Blake2_128Concat, H160, U256>;

	/// Approved redeem orders ready for settlement.
	/// Written by `approve_redeem_orders` (Approval mode).
	/// tranche_id → investor_id → USDC-to-distribute (converted from tokens at epoch_price)
	#[pallet::storage]
	pub type ApprovedRedeemOrders<T: Config> =
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
		/// Updates the tranche's aggregate pending deposit total
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::from_parts(10_000, 0))]
		pub fn submit_deposit_order(
			origin: OriginFor<T>,
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor_id: H160,
			amount: U256,
		) -> DispatchResult {
			ensure_signed(origin)?;
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

			PendingDepositOrders::<T>::mutate(tranche_id.clone(), investor_id, |entry| {
				*entry = Some(entry.unwrap_or_default().saturating_add(amount));
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
			ensure_signed(origin)?;
			ensure!(
				T::Pools::tranche_exists(pool_id, tranche_id.clone()),
				Error::<T>::PoolOrTrancheNotFound
			);
			ensure!(!amount.is_zero(), Error::<T>::ZeroAmount);
			ensure!(!T::Pools::in_settlement_window(pool_id), Error::<T>::PoolInSettlementWindow);

			PendingRedeemOrders::<T>::mutate(tranche_id.clone(), investor_id, |entry| {
				*entry = Some(entry.unwrap_or_default().saturating_add(amount));
			});

			// keep aggregate pending redeem total in sync.
			T::Pools::add_pending_redeem(pool_id, tranche_id.clone(), amount)?;

			// tokens were burned on the Spoke chain at request time.
			T::Pools::sub_token_supply(pool_id, tranche_id.clone(), amount)?;

			Self::deposit_event(Event::RedeemOrderSubmitted {
				pool_id,
				tranche_id,
				investor_id,
				amount,
			});
			Ok(())
		}

		/// Pool admin approves a selected set of investors' pending deposit orders during
		/// the settlement window (Approval mode).
		///
		/// For each investor in `investor_ids`: moves their entry from
		/// `PendingDepositOrders` to `ApprovedDepositOrders`. Investors without a
		/// pending order are silently skipped. The poll-based claim flow handles
		/// cross-chain token minting after this.
		#[pallet::call_index(2)]
		#[pallet::weight(Weight::from_parts(10_000, 0).saturating_mul(investor_ids.len() as u64))]
		pub fn approve_deposit_orders(
			origin: OriginFor<T>,
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor_ids: BoundedVec<H160, ConstU32<MAX_INVESTORS_PER_APPROVAL>>,
		) -> DispatchResult {
			let caller = ensure_signed(origin)?;
			let borrower =
				T::Pools::pool_borrower(pool_id).ok_or(Error::<T>::PoolOrTrancheNotFound)?;
			ensure!(caller == borrower, Error::<T>::NotBorrower);
			ensure!(T::Pools::in_settlement_window(pool_id), Error::<T>::NotInSettlementWindow);

			let epoch_price = T::Pools::epoch_price(pool_id, tranche_id.clone())
				.ok_or(Error::<T>::EpochPriceNotSet)?;

			let mut total_approved = U256::zero();

			for investor_id in investor_ids {
				let Some(amount) = PendingDepositOrders::<T>::take(tranche_id.clone(), investor_id)
				else {
					continue;
				};

				let tokens_to_mint = Self::assets_to_shares(amount, epoch_price);

				ApprovedDepositOrders::<T>::mutate(tranche_id.clone(), investor_id, |entry| {
					*entry = Some(entry.unwrap_or_default().saturating_add(tokens_to_mint));
				});

				total_approved = total_approved.saturating_add(amount);

				Self::deposit_event(Event::DepositOrderApproved {
					pool_id,
					tranche_id: tranche_id.clone(),
					investor_id,
					usdc_amount: amount,
					tokens_to_mint,
				});
			}

			if !total_approved.is_zero() {
				T::Pools::sub_pending_deposit(pool_id, tranche_id.clone(), total_approved)?;
				T::Pools::add_invested(pool_id, tranche_id, total_approved)?;
			}

			Ok(())
		}

		/// Pool admin approves a selected set of investors' pending redeem orders during
		/// the settlement window (Approval mode).
		///
		/// For each investor in `investor_ids`: moves their entry from
		/// `PendingRedeemOrders` to `ApprovedRedeemOrders`. Investors without a
		/// pending order are silently skipped. The borrower then calls
		/// `execute_redeem_orders` to settle them.
		#[pallet::call_index(3)]
		#[pallet::weight(Weight::from_parts(10_000, 0).saturating_mul(investor_ids.len() as u64))]
		pub fn approve_redeem_orders(
			origin: OriginFor<T>,
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor_ids: BoundedVec<H160, ConstU32<MAX_INVESTORS_PER_APPROVAL>>,
		) -> DispatchResult {
			let caller = ensure_signed(origin)?;
			let borrower =
				T::Pools::pool_borrower(pool_id).ok_or(Error::<T>::PoolOrTrancheNotFound)?;
			ensure!(caller == borrower, Error::<T>::NotBorrower);
			ensure!(T::Pools::in_settlement_window(pool_id), Error::<T>::NotInSettlementWindow);
			ensure!(
				!T::Pools::treasury_liquidity(pool_id, tranche_id.clone()).is_zero(),
				Error::<T>::InsufficientLiquidity
			);

			let epoch_price = T::Pools::epoch_price(pool_id, tranche_id.clone())
				.ok_or(Error::<T>::EpochPriceNotSet)?;

			let mut total_approved = U256::zero();

			for investor_id in investor_ids {
				let Some(tokens) = PendingRedeemOrders::<T>::take(tranche_id.clone(), investor_id)
				else {
					continue;
				};

				let shares_to_distribute = Self::shares_to_assets(tokens, epoch_price);

				ApprovedRedeemOrders::<T>::mutate(tranche_id.clone(), investor_id, |entry| {
					*entry = Some(entry.unwrap_or_default().saturating_add(shares_to_distribute));
				});

				total_approved = total_approved.saturating_add(tokens);

				Self::deposit_event(Event::RedeemOrderApproved {
					pool_id,
					tranche_id: tranche_id.clone(),
					investor_id,
					tokens,
					shares_to_distribute,
				});
			}

			if !total_approved.is_zero() {
				T::Pools::sub_pending_redeem(pool_id, tranche_id, total_approved)?;
			}

			Ok(())
		}
	}
}
