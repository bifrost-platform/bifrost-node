use crate::{AdapterKey, MultichainAdapterInfo, ProductId, Tranche};

use super::pallet::*;
use frame_support::{ensure, pallet_prelude::DispatchResult};
use sp_std::collections::btree_set::BTreeSet;

impl<T: Config> Pallet<T> {
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
	pub(crate) fn ensure_multichain_adapters_are_unregistered<'a, AccountId: 'a>(
		multichain_adapters: impl Iterator<
			Item = (&'a AdapterKey, &'a MultichainAdapterInfo<AccountId>),
		>,
	) -> DispatchResult {
		for (key, info) in multichain_adapters {
			ensure!(
				!MultichainAdapterIndex::<T>::contains_key(key),
				Error::<T>::MultichainAdapterAlreadyRegistered
			);
			for address in info.adapters.keys() {
				let adapter_key = AdapterKey { address: *address, chain_id: key.chain_id };
				ensure!(
					!AdapterIndex::<T>::contains_key(&adapter_key),
					Error::<T>::AdapterAlreadyRegistered
				);
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
