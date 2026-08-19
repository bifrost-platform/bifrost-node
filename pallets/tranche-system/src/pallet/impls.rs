use crate::{
	AdapterInspect, AdapterKey, MultichainAdapterInfo, ProductDetails, ProductId, ProductInspect,
	Tranche, TrancheType, VaultId, VaultInspect,
};

use super::pallet::*;
use frame_support::{ensure, pallet_prelude::DispatchResult};
use sp_core::H160;
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

	/// `create_single_chain_product`-only: checks none of the incoming flat
	/// Adapter addresses are already registered (on `chain_id`) to an
	/// existing product — single-chain adapters share the same `AdapterIndex`
	/// reverse-index as nested multichain ones (see `AdapterKey`'s doc
	/// comment), so an address already used by a multichain product's nested
	/// Adapter on the same chain collides here too. Intra-call duplicates are
	/// impossible to begin with — `adapters` is a `BoundedBTreeMap`, keyed by
	/// address.
	pub(crate) fn ensure_single_chain_adapters_are_unregistered<'a>(
		chain_id: u64,
		addresses: impl Iterator<Item = &'a H160>,
	) -> DispatchResult {
		for address in addresses {
			let key = AdapterKey { address: *address, chain_id };
			ensure!(!AdapterIndex::<T>::contains_key(&key), Error::<T>::AdapterAlreadyRegistered);
		}
		Ok(())
	}

	/// `set_adapters`-only: deep-replaces `AdapterIndex` entries for one flat
	/// Adapter address set on `chain_id` — drops every `old_address`'s entry
	/// first (so re-registering the same address, e.g. just to change its
	/// weightBps, isn't mistaken for a collision below), then checks every
	/// `new_address` is free, then writes them all to `product_id`. Shared by
	/// `set_adapters`'s Multichain branch (one parent's nested set, `chain_id`
	/// = that parent's own) and SingleChain branch (the product's whole flat
	/// set, `chain_id` = the product's own).
	pub(crate) fn replace_adapter_index<'a>(
		product_id: ProductId,
		chain_id: u64,
		old_addresses: impl Iterator<Item = &'a H160>,
		new_addresses: impl Iterator<Item = &'a H160> + Clone,
	) -> DispatchResult {
		for old_address in old_addresses {
			AdapterIndex::<T>::remove(&AdapterKey { address: *old_address, chain_id });
		}
		for address in new_addresses.clone() {
			let new_key = AdapterKey { address: *address, chain_id };
			ensure!(
				!AdapterIndex::<T>::contains_key(&new_key),
				Error::<T>::AdapterAlreadyRegistered
			);
		}
		for address in new_addresses {
			AdapterIndex::<T>::insert(&AdapterKey { address: *address, chain_id }, product_id);
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

impl<T: Config> VaultInspect for Pallet<T> {
	fn vault_belongs_to_product(product_id: ProductId, vault: &VaultId) -> bool {
		Vaults::<T>::get(vault) == Some(product_id)
	}

	fn vault_chains_belong_to_product(product_id: ProductId, chain_ids: &[u64]) -> bool {
		let product = Products::<T>::get(product_id);
		chain_ids.iter().all(|chain_id| {
			product.as_ref().is_some_and(|product| match product {
				ProductDetails::Multichain(product) => {
					product.tranches.iter().any(|tranche| tranche.vault.chain_id == *chain_id)
				},
				ProductDetails::SingleChain(product) => product.chain_id == *chain_id,
			})
		})
	}

	fn product_id_for_vault(vault: &VaultId) -> Option<ProductId> {
		Vaults::<T>::get(vault)
	}
}

impl<T: Config> AdapterInspect for Pallet<T> {
	fn multichain_adapter_belongs_to_product(product_id: ProductId, key: &AdapterKey) -> bool {
		MultichainAdapterIndex::<T>::get(key) == Some(product_id)
	}

	fn adapter_belongs_to_product(product_id: ProductId, key: &AdapterKey) -> bool {
		AdapterIndex::<T>::get(key) == Some(product_id)
	}

	fn adapter_chains_belong_to_product(product_id: ProductId, chain_ids: &[u64]) -> bool {
		let product = Products::<T>::get(product_id);
		chain_ids.iter().all(|chain_id| {
			product.as_ref().is_some_and(|product| match product {
				ProductDetails::Multichain(product) => {
					product.multichain_adapters.keys().any(|key| key.chain_id == *chain_id)
				},
				// `adapters` can never be empty for an existing single-chain
				// product — `create_single_chain_product` requires its
				// weights to sum to exactly 10_000, impossible for an empty
				// map, and there's no mutator that could empty it afterward.
				ProductDetails::SingleChain(product) => product.chain_id == *chain_id,
			})
		})
	}
}

impl<T: Config> ProductInspect for Pallet<T> {
	fn single_chain_id(product_id: ProductId) -> Option<u64> {
		match Products::<T>::get(product_id)? {
			ProductDetails::Multichain(_) => None,
			ProductDetails::SingleChain(product) => Some(product.chain_id),
		}
	}
}
