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
		type Pools: PoolInspect;
	}

	// -----------------------------------------------------------------------
	// Errors
	// -----------------------------------------------------------------------

	#[pallet::error]
	pub enum Error<T> {
		/// No pool exists with this ID, or the vault address is not registered to it.
		PoolOrTrancheNotFound,
		/// Invest amount must be greater than zero.
		ZeroAmount,
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
			/// Globally unique tranche identifier (chain_id + vault_address).
			tranche_id: TrancheId,
			/// Investor address on the external chain.
			investor: H160,
			/// USDC amount to invest.
			amount: U256,
		},
		/// A redeem order was submitted and is pending epoch settlement.
		RedeemOrderSubmitted {
			pool_id: PoolId,
			/// Globally unique tranche identifier (chain_id + vault_address).
			tranche_id: TrancheId,
			/// Investor address on the external chain.
			investor: H160,
			/// Number of tranche tokens to redeem.
			amount: U256,
		},
	}

	// -----------------------------------------------------------------------
	// Storage
	// -----------------------------------------------------------------------

	/// Pending invest orders awaiting epoch settlement.
	///
	/// tranche_id -> investor -> cumulative USDC amount to invest
	#[pallet::storage]
	pub type PendingInvestOrders<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		TrancheId,
		Blake2_128Concat,
		H160, // investor address
		U256,
	>;

	/// Pending redeem orders awaiting epoch settlement.
	///
	/// tranche_id -> investor -> cumulative tranche token amount to redeem
	#[pallet::storage]
	pub type PendingRedeemOrders<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		TrancheId,
		Blake2_128Concat,
		H160, // investor address
		U256,
	>;

	// -----------------------------------------------------------------------
	// Extrinsics
	// -----------------------------------------------------------------------

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Entry point called by the investments precompile when a `requestDeposit`
		/// message arrives on Bifrost via CCCP.
		///
		/// Validates that `vault_address` is a registered tranche on `pool_id`, then
		/// accumulates the order into `PendingInvestOrders` for settlement at epoch end.
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::from_parts(10_000, 0))]
		pub fn submit_invest_order(
			origin: OriginFor<T>,
			pool_id: PoolId,
			tranche_id: TrancheId,
			investor: H160,
			amount: U256,
		) -> DispatchResult {
			// TODO: Add authorization check.
			ensure_signed(origin)?;

			ensure!(
				T::Pools::tranche_exists(pool_id, tranche_id.clone()),
				Error::<T>::PoolOrTrancheNotFound
			);
			ensure!(!amount.is_zero(), Error::<T>::ZeroAmount);

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
		/// Accumulates tranche token redemption amount into `PendingRedeemOrders`
		/// for settlement at epoch end.
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
	}
}
