mod impls;

use crate::{LoanDetails, LoanId, LoanStatus, PoolId, PoolInspect, PoolReserve, Rate};

use frame_support::{pallet_prelude::*, traits::StorageVersion};
use frame_system::pallet_prelude::*;
use sp_core::{H256, U256};
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
		/// Pool inspection + reserve accounting — implemented by pallet-pools.
		type Pools: PoolInspect<Self::AccountId> + PoolReserve<U256>;
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
		/// Loan does not exist for the given (pool_id, loan_id).
		LoanNotFound,
		/// Caller is not the borrower on this loan.
		NotBorrower,
		/// Loan is not in the Active status.
		LoanNotActive,
		/// Borrow would exceed the loan's lifetime ceiling.
		CeilingExceeded,
		/// Cannot close a loan that still has outstanding debt.
		OutstandingDebt,
		/// Amount must be greater than zero.
		ZeroAmount,
	}

	// -----------------------------------------------------------------------
	// Events
	// -----------------------------------------------------------------------

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new loan was created by the pool admin.
		LoanCreated { pool_id: PoolId, loan_id: LoanId, borrower: T::AccountId, ceiling: U256 },
		/// Borrower drew `amount` from the pool reserve.
		/// Off-chain bot watches this event to physically disburse USDC.
		LoanBorrowed { pool_id: PoolId, loan_id: LoanId, amount: U256 },
		/// Borrower repaid `amount` to the pool reserve.
		/// Off-chain bot must have moved USDC back to the pool before this is called.
		LoanRepaid { pool_id: PoolId, loan_id: LoanId, amount: U256 },
		/// Loan was closed by the pool admin (debt must be zero).
		LoanClosed { pool_id: PoolId, loan_id: LoanId },
		/// NAV was recomputed for a pool.
		NavUpdated { pool_id: PoolId, nav: U256 },
	}

	// -----------------------------------------------------------------------
	// Storage
	// -----------------------------------------------------------------------

	/// Monotonically increasing loan ID counter per pool.
	#[pallet::storage]
	pub type NextLoanId<T: Config> = StorageMap<_, Blake2_128Concat, PoolId, LoanId, ValueQuery>;

	/// Active and closed loans, keyed by (pool_id, loan_id).
	#[pallet::storage]
	pub type Loans<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		PoolId,
		Blake2_128Concat,
		LoanId,
		LoanDetails<T::AccountId>,
	>;

	/// Cached NAV per pool: (nav, block_number_when_computed).
	#[pallet::storage]
	pub type LastNav<T: Config> = StorageMap<_, Blake2_128Concat, PoolId, (U256, u32)>;

	// -----------------------------------------------------------------------
	// Extrinsics
	// -----------------------------------------------------------------------

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Pool admin opens a new loan for a borrower.
		///
		/// The loan starts at zero principal; the borrower draws on it via `borrow`.
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::from_parts(10_000, 0))]
		pub fn create_loan(
			origin: OriginFor<T>,
			pool_id: PoolId,
			borrower: T::AccountId,
			collateral: H256,
			ceiling: U256,
			rate_per_sec: Rate,
		) -> DispatchResult {
			let caller = ensure_signed(origin)?;
			let admin = T::Pools::pool_admin(pool_id).ok_or(Error::<T>::PoolNotFound)?;
			ensure!(caller == admin, Error::<T>::NotPoolAdmin);

			let loan_id = NextLoanId::<T>::get(pool_id);
			let now = Self::current_block();

			let loan = LoanDetails {
				borrower: borrower.clone(),
				collateral,
				ceiling,
				rate_per_sec,
				principal: U256::zero(),
				interest: U256::zero(),
				total_borrowed: U256::zero(),
				total_repaid: U256::zero(),
				last_accrued: now,
				status: LoanStatus::Active,
			};

			Loans::<T>::insert(pool_id, loan_id, loan);
			NextLoanId::<T>::insert(pool_id, loan_id.saturating_add(1));

			Self::deposit_event(Event::LoanCreated { pool_id, loan_id, borrower, ceiling });
			Ok(())
		}

		/// Borrower draws `amount` from the pool reserve against an active loan.
		///
		/// Substrate-side: debits pool reserve and grows the loan's outstanding principal.
		/// Off-chain bot listens to `LoanBorrowed` and physically transfers USDC to the
		/// borrower on the destination chain.
		#[pallet::call_index(1)]
		#[pallet::weight(Weight::from_parts(10_000, 0))]
		pub fn borrow(
			origin: OriginFor<T>,
			pool_id: PoolId,
			loan_id: LoanId,
			amount: U256,
		) -> DispatchResult {
			let caller = ensure_signed(origin)?;
			ensure!(!amount.is_zero(), Error::<T>::ZeroAmount);

			Loans::<T>::try_mutate(pool_id, loan_id, |maybe_loan| -> Result<(), DispatchError> {
				let loan = maybe_loan.as_mut().ok_or(Error::<T>::LoanNotFound)?;
				ensure!(loan.borrower == caller, Error::<T>::NotBorrower);
				ensure!(loan.status == LoanStatus::Active, Error::<T>::LoanNotActive);

				let new_total = loan.total_borrowed.saturating_add(amount);
				ensure!(new_total <= loan.ceiling, Error::<T>::CeilingExceeded);

				loan.accrue(Self::current_block());
				loan.principal = loan.principal.saturating_add(amount);
				loan.total_borrowed = new_total;
				Ok(())
			})?;

			T::Pools::withdraw(pool_id, amount)?;

			Self::deposit_event(Event::LoanBorrowed { pool_id, loan_id, amount });
			Ok(())
		}

		/// Borrower repays `amount` against the loan.
		///
		/// Substrate-side: credits pool reserve and reduces the loan's outstanding debt.
		/// Repayment is applied to accrued interest first, then to principal.
		/// Off-chain bot must have physically returned USDC to the pool before this is called.
		#[pallet::call_index(2)]
		#[pallet::weight(Weight::from_parts(10_000, 0))]
		pub fn repay(
			origin: OriginFor<T>,
			pool_id: PoolId,
			loan_id: LoanId,
			amount: U256,
		) -> DispatchResult {
			let caller = ensure_signed(origin)?;
			ensure!(!amount.is_zero(), Error::<T>::ZeroAmount);

			let applied = Loans::<T>::try_mutate(
				pool_id,
				loan_id,
				|maybe_loan| -> Result<U256, DispatchError> {
					let loan = maybe_loan.as_mut().ok_or(Error::<T>::LoanNotFound)?;
					ensure!(loan.borrower == caller, Error::<T>::NotBorrower);
					ensure!(loan.status == LoanStatus::Active, Error::<T>::LoanNotActive);

					loan.accrue(Self::current_block());

					// Cap repayment at outstanding debt — no overpayment.
					let outstanding = loan.debt();
					let applied = amount.min(outstanding);

					// Interest first, then principal.
					if applied <= loan.interest {
						loan.interest = loan.interest.saturating_sub(applied);
					} else {
						let principal_part = applied.saturating_sub(loan.interest);
						loan.interest = U256::zero();
						loan.principal = loan.principal.saturating_sub(principal_part);
					}
					loan.total_repaid = loan.total_repaid.saturating_add(applied);
					Ok(applied)
				},
			)?;

			T::Pools::deposit(pool_id, applied)?;

			Self::deposit_event(Event::LoanRepaid { pool_id, loan_id, amount: applied });
			Ok(())
		}

		/// Pool admin closes a fully repaid loan.
		#[pallet::call_index(3)]
		#[pallet::weight(Weight::from_parts(5_000, 0))]
		pub fn close_loan(
			origin: OriginFor<T>,
			pool_id: PoolId,
			loan_id: LoanId,
		) -> DispatchResult {
			let caller = ensure_signed(origin)?;
			let admin = T::Pools::pool_admin(pool_id).ok_or(Error::<T>::PoolNotFound)?;
			ensure!(caller == admin, Error::<T>::NotPoolAdmin);

			Loans::<T>::try_mutate(pool_id, loan_id, |maybe_loan| -> Result<(), DispatchError> {
				let loan = maybe_loan.as_mut().ok_or(Error::<T>::LoanNotFound)?;
				ensure!(loan.status == LoanStatus::Active, Error::<T>::LoanNotActive);
				loan.accrue(Self::current_block());
				ensure!(loan.debt().is_zero(), Error::<T>::OutstandingDebt);
				loan.status = LoanStatus::Closed;
				Ok(())
			})?;

			Self::deposit_event(Event::LoanClosed { pool_id, loan_id });
			Ok(())
		}

		/// Permissionless: recompute NAV by accruing every active loan in the pool
		/// and summing their outstanding debt. Caches the result for pallet-pools.
		#[pallet::call_index(4)]
		#[pallet::weight(Weight::from_parts(20_000, 0))]
		pub fn update_nav(origin: OriginFor<T>, pool_id: PoolId) -> DispatchResult {
			ensure_signed(origin)?;
			let nav = Self::do_update_nav(pool_id)?;
			Self::deposit_event(Event::NavUpdated { pool_id, nav });
			Ok(())
		}
	}
}
