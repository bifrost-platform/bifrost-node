use crate::{
	AdapterInspect, AdapterKey, ChainTranches, CrudAction, FlowVersion, MultichainAdapterInfo,
	ProductDetails, ProductId, ProductInspect, Tranche, TrancheType, VaultId, VaultInspect,
	VaultRegistration,
};

use super::pallet::*;
use frame_support::{ensure, pallet_prelude::DispatchResult};
use sp_core::H160;
use sp_runtime::DispatchError;
use sp_std::{collections::btree_set::BTreeSet, vec::Vec};

/// One vault `set_tranche`'s `Update` cascade renamed alongside the vault the
/// caller directly targeted — `(vault, new_type, asset, shares, priority)`.
/// See `cascade_tranche_type_rename`'s own doc comment.
pub(crate) type CascadedTrancheUpdate = (VaultId, TrancheType, H160, H160, u8);

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

	/// `create_product`/`create_single_chain_product`-only: checks none of the
	/// incoming tranches' vaults are already registered — either to an
	/// existing product, or duplicated within this same call (impossible for
	/// `multichain_adapters`/`adapters`, since those are `BoundedBTreeMap`s
	/// and can't hold duplicate keys, but a chain's own tranche list is a
	/// plain `ChainTranches`). Callers pass a flattened iterator across every
	/// chain (e.g. `tranches_map.values().flat_map(|chain| chain.iter())`)
	/// since vault uniqueness must hold across the whole product, not just
	/// within one chain's own group.
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
	/// tranche precedes every `Junior` one. Deliberately takes one chain's own
	/// slice at a time (`ChainTranches`, or `SingleChainProductDetails::tranches`
	/// directly) — this invariant is per-chain, not product-wide (see
	/// `TrancheInput`'s doc comment for why cross-chain tranche ordering isn't
	/// meaningful). Shared by `create_product`/`create_single_chain_product`
	/// (on each chain's freshly-sorted input group) and
	/// `apply_add_tranche`/`remove_tranche_from_chain`/`update_tranche_in_chain`
	/// (re-checked on the resulting one-chain list after the mutation, since
	/// `Add`/`Remove`/`Update` can all change relative order within it).
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

	/// Checks a chain's own tranche list holds at most one `Junior` (the
	/// residual slot is singular) and, if the list is non-empty at all, at
	/// least one `Senior` (a chain can't have a Junior with nothing to claim
	/// the residual *of*) — same per-chain scope as
	/// `ensure_senior_precedes_junior`, and shared by the exact same call
	/// sites (`create_product`/`create_single_chain_product` on each chain's
	/// freshly-sorted group,
	/// `apply_add_tranche`/`remove_tranche_from_chain`/`update_tranche_in_chain`
	/// on the resulting list after every mutation). Vacuously satisfied by an empty list —
	/// `set_tranche`'s `Remove` is allowed to empty a chain's list out
	/// entirely (e.g. retiring a Hub-deployed vault while keeping Spoke ones,
	/// or vice versa); the *product-wide* "at least one tranche somewhere"
	/// floor is enforced separately, once, by `set_tranche` itself — see
	/// `Error::ProductMustHaveAtLeastOneTranche`.
	///
	/// Also checks no two tranches *on this one chain* share the exact same
	/// `tranche_type` value (2026-08-26) — e.g. two `Senior { apr: 5% }`
	/// vaults can't coexist on the same chain (`Junior` is already capped at
	/// one above, so this only has real bite for `Senior`). This is
	/// deliberately per-chain, not product-wide: the *same* type existing on
	/// *different* chains is exactly the normal case `set_tranche`'s
	/// `Update` cascade and `Error::TrancheTypeMustExistSomewhere` are built
	/// around (see `set_tranche`'s own doc comment) — e.g. `Senior { apr: 5%
	/// }` on both a Hub vault and a Spoke vault is fine, two `Senior { apr:
	/// 5% }` vaults on the *same* chain is not.
	pub(crate) fn ensure_valid_tranche_composition(tranches: &[Tranche]) -> DispatchResult {
		if tranches.is_empty() {
			return Ok(());
		}
		let junior_count = tranches
			.iter()
			.filter(|t| matches!(t.tranche_type, TrancheType::Junior))
			.count();
		let senior_count = tranches
			.iter()
			.filter(|t| matches!(t.tranche_type, TrancheType::Senior { .. }))
			.count();
		ensure!(junior_count <= 1, Error::<T>::TooManyJuniorTranches);
		ensure!(senior_count >= 1, Error::<T>::AtLeastOneSeniorTrancheRequired);
		for (i, a) in tranches.iter().enumerate() {
			for b in &tranches[i + 1..] {
				ensure!(a.tranche_type != b.tranche_type, Error::<T>::DuplicateTrancheTypeOnChain);
			}
		}
		Ok(())
	}

	/// After `set_tranche`'s `Update` changes `vault`'s own `tranche_type`
	/// from `old_type` to `new_type`, renames every *other* tranche in this
	/// one chain's own list that still carries `old_type` to `new_type` too
	/// — a `tranche_type` (e.g. "Senior at 5%") is a class shared across the
	/// whole product, not scoped to one vault, so an `apr` change moves every
	/// vault at the old rate together. Only the `tranche_type` field changes
	/// for each one — its own `asset`/`shares`/position are left exactly as
	/// they were. Called once per chain by `apply_update_tranche` (looping
	/// over every chain for a `Multichain` product, once for the product's
	/// own flat list for `SingleChain`) — `old_type != new_type` is checked
	/// by the caller before calling this at all. Returns `(vault, new_type,
	/// asset, shares, priority)` for each tranche actually renamed, so
	/// `set_tranche` can emit one `Event::TrancheSet` per affected vault (see
	/// that event's own doc comment for why each gets a separate event
	/// rather than being folded into the one for `vault` itself).
	pub(crate) fn cascade_tranche_type_rename(
		chain_tranches: &mut ChainTranches,
		vault: &VaultId,
		old_type: &TrancheType,
		new_type: &TrancheType,
	) -> Vec<CascadedTrancheUpdate> {
		let mut affected = Vec::new();
		for (idx, t) in chain_tranches.iter_mut().enumerate() {
			if &t.vault != vault && &t.tranche_type == old_type {
				t.tranche_type = new_type.clone();
				affected.push((t.vault.clone(), new_type.clone(), t.asset, t.shares, idx as u8));
			}
		}
		affected
	}

	/// `set_tranche`'s single entry point into `product` — dispatches by
	/// `CrudAction` first (`Add`/`Remove`/`Update` below), each of which
	/// handles both product topologies (Multichain/SingleChain) internally.
	/// Split this way rather than by topology: the three actions differ far
	/// more from each other (`Add` has no prior state to read at all;
	/// `Remove` checks the two product-wide floors in `ensure_product_minimums_after_remove`;
	/// `Update` can cascade a rename across every other chain) than
	/// Multichain differs from SingleChain *within* one action (only how the
	/// target chain's own list is located) — so following one action
	/// straight through, across both topologies, reads more linearly than
	/// following one topology across three unrelated actions.
	pub(crate) fn apply_set_tranche(
		product_id: ProductId,
		product: &mut ProductDetails<T::AccountId>,
		action: CrudAction,
		vault: &VaultId,
		tranche_type: &TrancheType,
		asset: H160,
		shares: H160,
		priority: u8,
	) -> Result<Vec<CascadedTrancheUpdate>, DispatchError> {
		match action {
			CrudAction::Add => {
				Self::apply_add_tranche(
					product_id,
					product,
					vault,
					tranche_type,
					asset,
					shares,
					priority,
				)?;
				Ok(Vec::new())
			},
			CrudAction::Remove => {
				Self::apply_remove_tranche(product, vault)?;
				Ok(Vec::new())
			},
			CrudAction::Update => {
				Self::apply_update_tranche(product, vault, tranche_type, asset, shares, priority)
			},
		}
	}

	/// `set_tranche`'s `Add` — locates `vault.chain_id`'s own list (creating
	/// a fresh per-chain entry on first use, Multichain only), then inserts
	/// at `priority`, shifting everything at or after it down by one.
	/// Registers `vault` in `Vaults` — see `VaultRegistration`'s doc comment
	/// for the permanent-binding rule this enforces: a key already present
	/// there only ever lets this succeed again for the SAME `product_id`,
	/// re-adding a vault it previously removed.
	fn apply_add_tranche(
		product_id: ProductId,
		product: &mut ProductDetails<T::AccountId>,
		vault: &VaultId,
		tranche_type: &TrancheType,
		asset: H160,
		shares: H160,
		priority: u8,
	) -> DispatchResult {
		let chain_tranches = match product {
			ProductDetails::Multichain(product) => {
				let chain_id = vault.chain_id;
				if !product.tranches.contains_key(&chain_id) {
					product
						.tranches
						.try_insert(chain_id, ChainTranches::default())
						.map_err(|_| Error::<T>::TooManyTranches)?;
				}
				product.tranches.get_mut(&chain_id).ok_or(Error::<T>::VaultNotFound)?
			},
			ProductDetails::SingleChain(product) => {
				// A single-chain product has exactly one chain — every
				// tranche's vault must live on it, same constraint
				// `create_single_chain_product` enforces at creation time.
				ensure!(
					vault.chain_id == product.chain_id,
					Error::<T>::SingleChainTranchesMustShareChain
				);
				&mut product.tranches
			},
		};

		if let Some(reg) = Vaults::<T>::get(vault) {
			ensure!(reg.product_id == product_id, Error::<T>::VaultBoundToDifferentProduct);
			ensure!(reg.removed, Error::<T>::VaultAlreadyRegistered);
		}
		let idx = priority as usize;
		ensure!(idx <= chain_tranches.len(), Error::<T>::InvalidPriority);
		chain_tranches
			.try_insert(
				idx,
				Tranche { tranche_type: tranche_type.clone(), vault: vault.clone(), asset, shares },
			)
			.map_err(|_| Error::<T>::TooManyTranches)?;
		Self::ensure_senior_precedes_junior(chain_tranches)?;
		Self::ensure_valid_tranche_composition(chain_tranches)?;
		Vaults::<T>::insert(vault, VaultRegistration { product_id, removed: false });
		Ok(())
	}

	/// `set_tranche`'s `Remove` — locates `vault.chain_id`'s own list, finds
	/// and removes `vault`'s tranche from it (shifting everything after it
	/// up by one), drops that chain's entry if now empty (Multichain only),
	/// tombstones (never deletes — see `Vaults`' own storage doc comment)
	/// its `Vaults` entry, then checks the two product-wide floors this
	/// action alone can violate (see `ensure_product_minimums_after_remove`).
	fn apply_remove_tranche(
		product: &mut ProductDetails<T::AccountId>,
		vault: &VaultId,
	) -> DispatchResult {
		let removed_type = match product {
			ProductDetails::Multichain(product) => {
				let chain_id = vault.chain_id;
				let chain_tranches =
					product.tranches.get_mut(&chain_id).ok_or(Error::<T>::VaultNotFound)?;
				let removed_type = Self::remove_tranche_from_chain(chain_tranches, vault)?;
				if chain_tranches.is_empty() {
					product.tranches.remove(&chain_id);
				}
				removed_type
			},
			ProductDetails::SingleChain(product) => {
				Self::remove_tranche_from_chain(&mut product.tranches, vault)?
			},
		};
		Self::ensure_product_minimums_after_remove(product, removed_type)
	}

	/// `apply_remove_tranche`'s per-chain half: finds `vault` by position,
	/// removes it, re-validates the resulting list, then tombstones its
	/// `Vaults` entry. Returns the removed tranche's own `tranche_type`, for
	/// `apply_remove_tranche`'s product-wide floor checks.
	fn remove_tranche_from_chain(
		chain_tranches: &mut ChainTranches,
		vault: &VaultId,
	) -> Result<TrancheType, DispatchError> {
		let idx = chain_tranches
			.iter()
			.position(|t| &t.vault == vault)
			.ok_or(Error::<T>::VaultNotFound)?;
		let removed_type = chain_tranches[idx].tranche_type.clone();
		chain_tranches.remove(idx);
		Self::ensure_senior_precedes_junior(chain_tranches)?;
		Self::ensure_valid_tranche_composition(chain_tranches)?;
		Vaults::<T>::try_mutate(vault, |maybe_reg| -> DispatchResult {
			let reg = maybe_reg.as_mut().ok_or(Error::<T>::VaultNotFound)?;
			reg.removed = true;
			Ok(())
		})?;
		Ok(removed_type)
	}

	/// `set_tranche`'s `Remove`-only product-wide floors, checked once here
	/// rather than per-chain (`ensure_valid_tranche_composition` only ever
	/// checks the one chain a mutation touched): a product must always
	/// retain at least one tranche *somewhere*, and every `tranche_type` it
	/// has ever carried must keep at least one live vault *somewhere*. `Add`
	/// only ever grows the total, and `Update` renames a `tranche_type` as
	/// one atomic product-wide group (see `cascade_tranche_type_rename`), so
	/// neither can ever trip this — only `Remove` needs it.
	fn ensure_product_minimums_after_remove(
		product: &ProductDetails<T::AccountId>,
		removed_type: TrancheType,
	) -> DispatchResult {
		let total_tranches: usize = match product {
			ProductDetails::Multichain(product) => {
				product.tranches.values().map(|chain| chain.len()).sum()
			},
			ProductDetails::SingleChain(product) => product.tranches.len(),
		};
		ensure!(total_tranches > 0, Error::<T>::ProductMustHaveAtLeastOneTranche);

		let type_still_exists = match product {
			ProductDetails::Multichain(product) => product
				.tranches
				.values()
				.flat_map(|chain| chain.iter())
				.any(|t| t.tranche_type == removed_type),
			ProductDetails::SingleChain(product) => {
				product.tranches.iter().any(|t| t.tranche_type == removed_type)
			},
		};
		ensure!(type_still_exists, Error::<T>::TrancheTypeMustExistSomewhere);
		Ok(())
	}

	/// `set_tranche`'s `Update` — locates `vault.chain_id`'s own list,
	/// re-inserts `vault`'s tranche at `priority` with the new
	/// `asset`/`shares`/`tranche_type`, then — only if `tranche_type`
	/// actually changed — cascades that rename to every *other* chain still
	/// carrying the OLD type (see `cascade_tranche_type_rename`),
	/// re-validating any chain the cascade actually touched (a rename can
	/// collide with a tranche that already carried the NEW type on that
	/// chain for an unrelated reason — see `Error::DuplicateTrancheTypeOnChain`).
	/// Returns every OTHER vault the cascade renamed, for `set_tranche` to
	/// emit one `Event::TrancheSet` per affected vault.
	fn apply_update_tranche(
		product: &mut ProductDetails<T::AccountId>,
		vault: &VaultId,
		tranche_type: &TrancheType,
		asset: H160,
		shares: H160,
		priority: u8,
	) -> Result<Vec<CascadedTrancheUpdate>, DispatchError> {
		match product {
			ProductDetails::Multichain(product) => {
				let chain_id = vault.chain_id;
				let chain_tranches =
					product.tranches.get_mut(&chain_id).ok_or(Error::<T>::VaultNotFound)?;
				let old_type = Self::update_tranche_in_chain(
					chain_tranches,
					vault,
					tranche_type,
					asset,
					shares,
					priority,
				)?;

				let mut cascaded = Vec::new();
				if &old_type != tranche_type {
					for (_, chain_tranches) in product.tranches.iter_mut() {
						let renamed = Self::cascade_tranche_type_rename(
							chain_tranches,
							vault,
							&old_type,
							tranche_type,
						);
						// A rename can collide with a tranche that already
						// carried `tranche_type` on this same chain, for an
						// entirely unrelated reason — re-check this chain's
						// own composition rather than assuming a cascaded
						// write is always safe.
						if !renamed.is_empty() {
							Self::ensure_valid_tranche_composition(chain_tranches)?;
						}
						cascaded.extend(renamed);
					}
				}
				Ok(cascaded)
			},
			ProductDetails::SingleChain(product) => {
				// Same chain-match constraint as `apply_add_tranche` — see
				// its own comment.
				ensure!(
					vault.chain_id == product.chain_id,
					Error::<T>::SingleChainTranchesMustShareChain
				);
				// No cascade here, unlike the Multichain arm above: a
				// `SingleChain` product has exactly one chain, and that one
				// chain can never hold a second live tranche at `old_type` to
				// begin with (`Error::DuplicateTrancheTypeOnChain`) — there is
				// structurally nothing else for a rename to ever propagate to.
				Self::update_tranche_in_chain(
					&mut product.tranches,
					vault,
					tranche_type,
					asset,
					shares,
					priority,
				)?;
				Ok(Vec::new())
			},
		}
	}

	/// `apply_update_tranche`'s per-chain half: finds `vault` by position,
	/// checks the Junior/Senior discriminant hasn't changed (`apr` may
	/// still, for `Senior`), then re-inserts at `priority` with the new
	/// `asset`/`shares`/`tranche_type`. Returns the tranche's own
	/// `tranche_type` from *before* this update, for
	/// `apply_update_tranche`'s cross-chain cascade.
	fn update_tranche_in_chain(
		chain_tranches: &mut ChainTranches,
		vault: &VaultId,
		tranche_type: &TrancheType,
		asset: H160,
		shares: H160,
		priority: u8,
	) -> Result<TrancheType, DispatchError> {
		let idx = chain_tranches
			.iter()
			.position(|t| &t.vault == vault)
			.ok_or(Error::<T>::VaultNotFound)?;
		let previous_type = chain_tranches[idx].tranche_type.clone();
		let type_matches = matches!(
			(&previous_type, tranche_type),
			(TrancheType::Junior, TrancheType::Junior)
				| (TrancheType::Senior { .. }, TrancheType::Senior { .. })
		);
		ensure!(type_matches, Error::<T>::TrancheTypeImmutable);

		chain_tranches.remove(idx);
		let new_idx = priority as usize;
		ensure!(new_idx <= chain_tranches.len(), Error::<T>::InvalidPriority);
		chain_tranches
			.try_insert(
				new_idx,
				Tranche { tranche_type: tranche_type.clone(), vault: vault.clone(), asset, shares },
			)
			.map_err(|_| Error::<T>::TooManyTranches)?;
		Self::ensure_senior_precedes_junior(chain_tranches)?;
		Self::ensure_valid_tranche_composition(chain_tranches)?;
		Ok(previous_type)
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
		Vaults::<T>::get(vault).is_some_and(|reg| reg.product_id == product_id)
	}

	fn vault_chains_belong_to_product(product_id: ProductId, chain_ids: &[u64]) -> bool {
		let product = Products::<T>::get(product_id);
		chain_ids.iter().all(|chain_id| {
			product.as_ref().is_some_and(|product| match product {
				// `chain_id` is now the map's own key — a direct lookup rather
				// than a linear scan, now that `tranches` is chain-keyed.
				ProductDetails::Multichain(product) => product.tranches.contains_key(chain_id),
				ProductDetails::SingleChain(product) => product.chain_id == *chain_id,
			})
		})
	}

	fn product_id_for_vault(vault: &VaultId) -> Option<ProductId> {
		Vaults::<T>::get(vault).map(|reg| reg.product_id)
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

	fn request_flow_version(product_id: ProductId) -> Option<FlowVersion> {
		RequestFlowVersion::<T>::get(product_id)
	}

	fn settlement_flow_version(product_id: ProductId) -> Option<FlowVersion> {
		SettlementFlowVersion::<T>::get(product_id)
	}
}
