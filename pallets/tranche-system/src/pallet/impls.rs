use crate::{
	AdapterKey, MultichainAdapterInfo, NavSyncOutcome, NavSyncState, ProductId, Tranche,
	TrancheType,
};

use super::pallet::*;
use frame_support::{ensure, pallet_prelude::DispatchResult, weights::Weight};
use pallet_evm::{ExitReason, GasWeightMapping, Runner};
use sp_core::H160;
use sp_runtime::{DispatchError, SaturatedConversion};
use sp_std::collections::btree_set::BTreeSet;

/// `tryUpdateNav()` selector (`cast sig "tryUpdateNav()"`) — the function
/// takes no arguments, so this is the entire calldata.
const TRY_UPDATE_NAV_SELECTOR: [u8; 4] = [0x32, 0xc5, 0xaf, 0x98];

/// Gas limit for the `tryUpdateNav()` internal call — same order of
/// magnitude as this codebase's other fire-and-forget internal EVM calls
/// (see btc-socket-queue's `CALL_GAS_LIMIT`).
const NAV_SYNC_GAS_LIMIT: u64 = 1_000_000;

/// This pallet's own precompile address (see interface.sol's `Address:`
/// note) — used as `source`/`msg.sender` when `on_initialize` auto-triggers
/// `Valuation.tryUpdateNav()`, since there's no EOA caller for a
/// system-triggered call.
const TRANCHE_SYSTEM_PRECOMPILE_ADDRESS: H160 =
	H160([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0]);

/// Minimum gap, in seconds, between two `tryUpdateNav()` retry attempts for
/// the same product within one epoch's settlement window.
const NAV_SYNC_RETRY_INTERVAL_SECS: u64 = 300;

/// Maximum number of `tryUpdateNav()` attempts per product, per epoch.
const MAX_NAV_SYNC_ATTEMPTS: u8 = 10;

impl<T: Config> Pallet<T> {
	/// Scans every product for one whose settlement window
	/// (`[settlement_offset_secs, settlement_length_secs)` of the current
	/// epoch — see `ValuationInfo::settlement_offset_secs`'s doc comment) is
	/// open, and retries `Valuation.tryUpdateNav()` every
	/// `NAV_SYNC_RETRY_INTERVAL_SECS` until it succeeds or
	/// `MAX_NAV_SYNC_ATTEMPTS` is reached. Called from `on_initialize`.
	///
	/// Iterates the full `Products` map every block — fine at this pallet's
	/// current scale, but will need bounding (or moving to an
	/// offchain-triggered design) once there are enough products for this to
	/// matter for block weight.
	pub(crate) fn sync_navs() -> Weight {
		let now_secs = <pallet_timestamp::Pallet<T>>::get() / 1000;
		let mut weight = Weight::zero();

		for (product_id, product) in Products::<T>::iter() {
			let length = product.valuation.settlement_length_secs;
			let offset = product.valuation.settlement_offset_secs;
			if length == 0 {
				continue;
			}

			let window_index = now_secs / length;
			let elapsed_in_window = now_secs % length;
			if elapsed_in_window < offset {
				continue; // market still open — not yet time to settle
			}

			let mut state = NavSyncStates::<T>::get(product_id)
				.filter(|s| s.window_index == window_index)
				.unwrap_or(NavSyncState {
					window_index,
					attempts: 0,
					last_attempt_secs: 0,
					succeeded: false,
				});

			if state.succeeded || state.attempts >= MAX_NAV_SYNC_ATTEMPTS {
				continue;
			}
			if state.attempts > 0
				&& now_secs.saturating_sub(state.last_attempt_secs) < NAV_SYNC_RETRY_INTERVAL_SECS
			{
				continue; // too soon since the last attempt
			}

			state.attempts += 1;
			state.last_attempt_secs = now_secs;
			let (succeeded, call_weight) = Self::try_update_nav(
				product_id,
				product.valuation.valuation_address,
				state.attempts,
			);
			state.succeeded = succeeded;
			weight = weight.saturating_add(call_weight);
			NavSyncStates::<T>::insert(product_id, state);
		}

		weight
	}

	/// Calls `Valuation.tryUpdateNav()` on `valuation_address` as an internal
	/// (nonce-less) EVM call from this pallet's own precompile address,
	/// records the outcome into `NavSyncLogs` (see `NavSyncOutcome`'s doc
	/// comment for why), deposits `NavSyncAttempted`, and returns whether it
	/// succeeded, plus the `Weight` actually consumed by the EVM call (from
	/// `info.used_gas`, or `RunnerError::weight` if it didn't even execute) —
	/// `sync_navs` folds this into its own returned weight so `on_initialize`
	/// reflects the real cost of whatever EVM work happened this block.
	///
	/// A `tx` that executes without reverting is treated as success — this
	/// pallet does not decode `tryUpdateNav()`'s own return value (if any);
	/// it trusts the EVM-level `exit_reason` alone. Never returns an error
	/// itself — a failed attempt is only logged/evented/recorded;
	/// `sync_navs` decides whether/when to retry.
	fn try_update_nav(
		product_id: ProductId,
		valuation_address: H160,
		attempt: u8,
	) -> (bool, Weight) {
		let block_number = frame_system::Pallet::<T>::block_number();

		let (succeeded, outcome, weight) =
			match <T as pallet_evm::Config>::Runner::call_as_internal_call(
				TRANCHE_SYSTEM_PRECOMPILE_ADDRESS,
				valuation_address,
				TRY_UPDATE_NAV_SELECTOR.to_vec(),
				NAV_SYNC_GAS_LIMIT,
				<T as pallet_evm::Config>::config(),
			) {
				Ok(info) => {
					let succeeded = matches!(info.exit_reason, ExitReason::Succeed(_));
					if !succeeded {
						log::warn!(
							target: "tranche-system",
							"sync_navs: tryUpdateNav reverted for product {} (attempt {}) at {:?}: {:?}",
							product_id, attempt, valuation_address, info.exit_reason
						);
					}
					let weight = <T as pallet_evm::Config>::GasWeightMapping::gas_to_weight(
						info.used_gas.standard.saturated_into::<u64>(),
						true,
					);
					(succeeded, NavSyncOutcome::Executed(info), weight)
				},
				Err(e) => {
					let weight = e.weight;
					let error: DispatchError = e.error.into();
					log::warn!(
						target: "tranche-system",
						"sync_navs: tryUpdateNav call failed for product {} (attempt {}) at {:?}: {:?}",
						product_id, attempt, valuation_address, error
					);
					(false, NavSyncOutcome::Failed(error), weight)
				},
			};

		NavSyncLogs::<T>::insert(product_id, block_number, outcome);
		Self::deposit_event(Event::NavSyncAttempted {
			product_id,
			valuation_address,
			attempt,
			succeeded,
		});

		(succeeded, weight)
	}

	/// Checks a `weightBps` set sums to exactly 10_000 (100%) — shared by
	/// `create_product`/`set_multichain_adapters` (top-level and, per parent,
	/// nested) and `set_adapters` (one parent's nested set). Accumulates as
	/// `u32` since up to `MAX_MULTICHAIN_ADAPTERS`/`MAX_ADAPTERS_PER_MULTICHAIN_ADAPTER`
	/// entries of up to `u16::MAX` each could otherwise overflow a `u16` sum.
	pub(crate) fn ensure_weights_sum_to_10000(
		weights_bps: impl Iterator<Item = u16>,
	) -> DispatchResult {
		let sum: u32 = weights_bps.map(u32::from).sum();
		ensure!(sum == 10_000, Error::<T>::WeightsMustSumTo10000);
		Ok(())
	}

	/// `create_product`-only: checks none of the incoming tranches' vaults are
	/// already registered — either to an existing product, or duplicated
	/// within this same call (impossible for `multichain_adapters`/`adapters`,
	/// since those are `BoundedBTreeMap`s and can't hold duplicate keys, but
	/// `tranches` is a plain `BoundedVec`).
	pub(crate) fn ensure_tranches_are_unregistered<'a>(
		tranches: impl Iterator<Item = &'a Tranche>,
	) -> DispatchResult {
		let mut seen = BTreeSet::new();
		for tranche in tranches {
			ensure!(seen.insert(tranche.vault.clone()), Error::<T>::VaultAlreadyRegistered);
			ensure!(!Vaults::<T>::contains_key(&tranche.vault), Error::<T>::VaultAlreadyRegistered);
		}
		Ok(())
	}

	/// `create_product`/`set_multichain_adapters`-only: checks none of the
	/// incoming MultichainAdapters (or their nested Adapters) are already
	/// registered to an existing product. Callers that are *replacing* an
	/// existing product's table must remove its old reverse-index entries
	/// first (see `set_multichain_adapters`), so re-registering the same
	/// (address, chain_id) isn't mistaken for a collision here.
	///
	/// Also enforces, via a local `seen` set, that an Adapter belongs to at
	/// most one MultichainAdapter: two different parents in the *same*
	/// incoming call can't nest the same (address, chain_id) — a check the
	/// per-entry storage lookup alone can't catch, since neither write has
	/// happened yet at validation time (mirrors `ensure_tranches_are_unregistered`'s
	/// same intra-call-duplicate guard for `tranches`).
	pub(crate) fn ensure_multichain_adapters_are_unregistered<'a, AccountId: 'a>(
		multichain_adapters: impl Iterator<
			Item = (&'a AdapterKey, &'a MultichainAdapterInfo<AccountId>),
		>,
	) -> DispatchResult {
		let mut seen = BTreeSet::new();
		for (key, info) in multichain_adapters {
			ensure!(
				!MultichainAdapterIndex::<T>::contains_key(key),
				Error::<T>::MultichainAdapterAlreadyRegistered
			);
			for address in info.adapters.keys() {
				let adapter_key = AdapterKey { address: *address, chain_id: key.chain_id };
				ensure!(seen.insert(adapter_key.clone()), Error::<T>::AdapterAlreadyRegistered);
				ensure!(
					!AdapterIndex::<T>::contains_key(&adapter_key),
					Error::<T>::AdapterAlreadyRegistered
				);
			}
		}
		Ok(())
	}

	/// Checks that, in priority order (index 0 = highest — i.e. array order,
	/// since `Tranche` carries no separate priority field), every `Senior`
	/// tranche precedes every `Junior` one. Shared by `create_product` (on the
	/// freshly-sorted input) and every `set_tranche` branch (re-checked on the
	/// resulting full list after the mutation, since `Add`/`Remove`/`Update`
	/// can all change relative order).
	pub(crate) fn ensure_senior_precedes_junior(tranches: &[Tranche]) -> DispatchResult {
		let mut seen_junior = false;
		for tranche in tranches {
			match tranche.tranche_type {
				TrancheType::Junior => seen_junior = true,
				TrancheType::Senior { .. } => {
					ensure!(!seen_junior, Error::<T>::SeniorMustPrecedeJunior);
				},
			}
		}
		Ok(())
	}

	/// Writes `MultichainAdapterIndex`/`AdapterIndex` reverse-index entries for
	/// every MultichainAdapter (and its nested Adapters) in the given set.
	/// Callers must have already validated uniqueness (see
	/// `ensure_multichain_adapters_are_unregistered`).
	pub(crate) fn insert_multichain_adapter_index<'a, AccountId: 'a>(
		product_id: ProductId,
		multichain_adapters: impl Iterator<
			Item = (&'a AdapterKey, &'a MultichainAdapterInfo<AccountId>),
		>,
	) {
		for (key, info) in multichain_adapters {
			MultichainAdapterIndex::<T>::insert(key, product_id);
			for address in info.adapters.keys() {
				let adapter_key = AdapterKey { address: *address, chain_id: key.chain_id };
				AdapterIndex::<T>::insert(&adapter_key, product_id);
			}
		}
	}
}
