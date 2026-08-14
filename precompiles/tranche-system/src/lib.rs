#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]

use frame_support::dispatch::{GetDispatchInfo, PostDispatchInfo};
use pallet_evm::AddressMapping;
use pallet_tranche_system::{
	AdapterInfo, AdapterKey, Call as TrancheSystemCall, CollateralAsset, CrudAction,
	MultichainAdapterInfo, ProductId, SourceType, TrancheInput, TrancheType, ValuationInfo,
	VaultId, MAX_ADAPTERS_PER_MULTICHAIN_ADAPTER, MAX_COLLATERALS, MAX_MULTICHAIN_ADAPTERS,
	MAX_TRANCHES, MAX_TRANCHE_MANAGERS,
};
use precompile_utils::prelude::*;
use sp_core::{ConstU32, H160, U256};
use sp_runtime::{traits::Dispatchable, BoundedBTreeMap, BoundedVec};
use sp_std::{collections::btree_map::BTreeMap, marker::PhantomData, vec::Vec};

// ---------------------------------------------------------------------------
// Event log selectors
// ---------------------------------------------------------------------------

pub(crate) const SELECTOR_LOG_PRODUCT_CREATED: [u8; 32] =
	keccak256!("ProductCreated(uint256,address,address,address,uint64,uint64,uint64)");
pub(crate) const SELECTOR_LOG_TRANCHE_SET: [u8; 32] =
	keccak256!("TrancheSet(uint256,uint8,uint8,uint256,uint64,address,address,address,uint8)");
pub(crate) const SELECTOR_LOG_ADAPTERS_SET: [u8; 32] = keccak256!(
	"AdaptersSet(uint256,address,uint64,(uint8,address,uint16,address,(address,uint256)[])[])"
);
pub(crate) const SELECTOR_LOG_MULTICHAIN_ADAPTERS_SET: [u8; 32] = keccak256!(
	"MultichainAdaptersSet(uint256,(address,uint64,uint16,(uint8,address,uint16,address,(address,uint256)[])[])[])"
);
pub(crate) const SELECTOR_LOG_MULTICHAIN_TRANCHE_MANAGERS_SET: [u8; 32] =
	keccak256!("MultichainTrancheManagersSet(uint256,(uint64,address)[])");

// ---------------------------------------------------------------------------
// interface.sol struct <-> tuple mappings
// ---------------------------------------------------------------------------

/// `ValuationInput` — (base_asset, valuation_address, settlement_start_timestamp,
/// settlement_length_secs, settlement_offset_secs)
type EvmValuationInput = (Address, Address, u64, u64, u64);
/// `VaultInput` — (chain_id, vault_address)
type EvmVaultInput = (u64, Address);
/// `TrancheInput` — (tranche_type, apr, vault, asset, shares, priority)
type EvmTrancheInput = (u8, U256, EvmVaultInput, Address, Address, u8);
/// `CollateralInput` — (nft_contract, nft_token_id)
type EvmCollateralInput = (Address, U256);
/// `AdapterInput` — (source_type, source_address, weightBps, borrower, collaterals)
type EvmAdapterInput = (u8, Address, u16, Address, Vec<EvmCollateralInput>);
/// `MultichainAdapterInput` — (adapter_address, chain_id, weightBps, adapters)
type EvmMultichainAdapterInput = (Address, u64, u16, Vec<EvmAdapterInput>);
/// `MultichainTrancheManagerInput` — (chain_id, tranche_manager_address)
type EvmMultichainTrancheManagerInput = (u64, Address);

// ---------------------------------------------------------------------------
// Precompile
// ---------------------------------------------------------------------------

/// A precompile that wraps `pallet_tranche_system`'s `create_product`/
/// `set_tranche`/`set_adapters`/`set_multichain_adapters` extrinsics.
///
/// Called directly by ProductAdmin EOAs — not by a Gateway — so origins are
/// resolved from `handle.context().caller`. Every one of these four functions
/// is gated by `pallet_tranche_system::Origin::ProductAdmin`, constructed here
/// only after reading `pallet-tranche-permissions`' `ProductAdmins` storage
/// directly to confirm the caller holds the role for `product_id` — the
/// pallet itself has no other way to verify this, since that custom origin
/// carries an already-authenticated account rather than re-deriving it. This
/// is deliberately the *only* way into any of these four extrinsics: none of
/// them accept a plain signed origin, so calling pallet-tranche-system
/// directly (bypassing this precompile) is impossible regardless of role.
pub struct TrancheSystemPrecompile<Runtime>(PhantomData<Runtime>);

#[precompile_utils::precompile]
impl<Runtime> TrancheSystemPrecompile<Runtime>
where
	Runtime: pallet_tranche_system::Config
		+ pallet_tranche_permissions::Config
		+ pallet_evm::Config
		+ frame_system::Config,
	Runtime::RuntimeCall: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	Runtime::RuntimeCall: From<TrancheSystemCall<Runtime>>,
	Runtime::RuntimeOrigin: From<pallet_tranche_system::Origin<Runtime>>,
	<Runtime as pallet_evm::Config>::AddressMapping: AddressMapping<Runtime::AccountId>,
	Runtime::AccountId: Into<H160>,
{
	/// Create a new tranche-system product. See `pallet_tranche_system::create_product`'s
	/// doc comment for full semantics.
	///
	/// @param product_id Hub product ID (already granted to the caller via ProductAdmin)
	/// @param valuation (base_asset, valuation_address, settlement_start_timestamp,
	/// settlement_length_secs, settlement_offset_secs)
	/// @param tranches Tranche configs; each entry's `priority` (0 = highest) determines the
	/// stored order, not array position — reverts if two entries share a `priority`, or if
	/// sorting by `priority` doesn't put every Senior tranche before every Junior one
	/// @param multichain_adapters MultichainAdapter routing entries, each carrying its own
	/// nested individual-Adapter registrations
	/// @param multichain_tranche_managers This product's per-chain TrancheManager
	/// bindings (chain_id, tranche_manager_address); Hub included, if the product has a
	/// Hub-deployed vault
	#[precompile::public(
		"create_product(uint256,(address,address,uint64,uint64,uint64),(uint8,uint256,(uint64,address),address,address,uint8)[],(address,uint64,uint16,(uint8,address,uint16,address,(address,uint256)[])[])[],(uint64,address)[])"
	)]
	fn create_product(
		handle: &mut impl PrecompileHandle,
		product_id: U256,
		valuation: EvmValuationInput,
		tranches: Vec<EvmTrancheInput>,
		multichain_adapters: Vec<EvmMultichainAdapterInput>,
		multichain_tranche_managers: Vec<EvmMultichainTrancheManagerInput>,
	) -> EvmResult {
		let caller = handle.context().caller;
		let caller_account = Runtime::AddressMapping::into_account_id(caller);
		let product_id = to_product_id(product_id)?;

		ensure_caller_is_product_admin::<Runtime>(product_id, &caller_account)?;

		let (
			base_asset,
			valuation_address,
			settlement_start_timestamp,
			settlement_length_secs,
			settlement_offset_secs,
		) = valuation;
		let valuation_info = ValuationInfo {
			base_asset: base_asset.0,
			valuation_address: valuation_address.0,
			settlement_start_timestamp,
			settlement_length_secs,
			settlement_offset_secs,
		};

		let bounded_tranches = decode_tranches(&tranches)?;
		let bounded_multichain_adapters =
			decode_multichain_adapters::<Runtime>(&multichain_adapters)?;
		let bounded_multichain_tranche_managers =
			decode_multichain_tranche_managers(&multichain_tranche_managers)?;

		let call = TrancheSystemCall::<Runtime>::create_product {
			product_id,
			valuation: valuation_info,
			tranches: bounded_tranches,
			multichain_adapters: bounded_multichain_adapters,
			multichain_tranche_managers: bounded_multichain_tranche_managers,
		};
		RuntimeHelper::<Runtime>::try_dispatch(
			handle,
			pallet_tranche_system::Origin::<Runtime>::ProductAdmin(caller_account).into(),
			call,
			0,
		)?;

		let event = log1(
			handle.context().address,
			SELECTOR_LOG_PRODUCT_CREATED,
			solidity::encode_event_data((
				U256::from(product_id),
				Address(caller),
				base_asset,
				valuation_address,
				settlement_start_timestamp,
				settlement_length_secs,
				settlement_offset_secs,
			)),
		);
		handle.record_log_costs(&[&event])?;
		event.record(handle)?;

		Ok(())
	}

	/// Add, remove, or update a tranche. See
	/// `pallet_tranche_system::set_tranche`'s doc comment for full semantics.
	///
	/// @param product_id The product whose tranche is being mutated
	/// @param action     0 = Add, 1 = Remove, 2 = Update
	/// @param tranche    (tranche_type, apr, vault, asset, shares, priority); field usage
	/// differs by `action`
	#[precompile::public(
		"set_tranche(uint256,uint8,(uint8,uint256,(uint64,address),address,address,uint8))"
	)]
	fn set_tranche(
		handle: &mut impl PrecompileHandle,
		product_id: U256,
		action: u8,
		tranche: EvmTrancheInput,
	) -> EvmResult {
		let caller = handle.context().caller;
		let caller_account = Runtime::AddressMapping::into_account_id(caller);
		let product_id = to_product_id(product_id)?;
		ensure_caller_is_product_admin::<Runtime>(product_id, &caller_account)?;
		let decoded_action = decode_crud_action(action)?;
		let (tranche_type_byte, apr, vault, asset, shares, priority) = tranche;
		let (vault_chain_id, vault_address) = vault;
		let tranche_type = decode_tranche_type(tranche_type_byte, apr)?;
		let vault_id = VaultId { chain_id: vault_chain_id, vault_address: vault_address.0 };

		let call = TrancheSystemCall::<Runtime>::set_tranche {
			product_id,
			action: decoded_action,
			vault: vault_id,
			tranche_type,
			asset: asset.0,
			shares: shares.0,
			priority,
		};
		RuntimeHelper::<Runtime>::try_dispatch(
			handle,
			pallet_tranche_system::Origin::<Runtime>::ProductAdmin(caller_account).into(),
			call,
			0,
		)?;

		let event = log1(
			handle.context().address,
			SELECTOR_LOG_TRANCHE_SET,
			solidity::encode_event_data((
				U256::from(product_id),
				action,
				tranche_type_byte,
				apr,
				vault_chain_id,
				vault_address,
				asset,
				shares,
				priority,
			)),
		);
		handle.record_log_costs(&[&event])?;
		event.record(handle)?;

		Ok(())
	}

	/// Replace, atomically, one MultichainAdapter's nested Adapters. See
	/// `pallet_tranche_system::set_adapters`'s doc comment for full semantics.
	///
	/// @param product_id             The product whose adapters are being replaced
	/// @param parent_adapter_address The parent MultichainAdapter's contract address
	/// @param parent_chain_id        The parent MultichainAdapter's chain ID
	/// @param adapters               The full intended end-state list of nested adapters
	#[precompile::public(
		"set_adapters(uint256,address,uint64,(uint8,address,uint16,address,(address,uint256)[])[])"
	)]
	fn set_adapters(
		handle: &mut impl PrecompileHandle,
		product_id: U256,
		parent_adapter_address: Address,
		parent_chain_id: u64,
		adapters: Vec<EvmAdapterInput>,
	) -> EvmResult {
		let caller = handle.context().caller;
		let caller_account = Runtime::AddressMapping::into_account_id(caller);
		let product_id = to_product_id(product_id)?;
		ensure_caller_is_product_admin::<Runtime>(product_id, &caller_account)?;
		let bounded_adapters = decode_adapters::<Runtime>(&adapters)?;

		let call = TrancheSystemCall::<Runtime>::set_adapters {
			product_id,
			parent_adapter_address: parent_adapter_address.0,
			parent_chain_id,
			adapters: bounded_adapters,
		};
		RuntimeHelper::<Runtime>::try_dispatch(
			handle,
			pallet_tranche_system::Origin::<Runtime>::ProductAdmin(caller_account).into(),
			call,
			0,
		)?;

		let event = log1(
			handle.context().address,
			SELECTOR_LOG_ADAPTERS_SET,
			solidity::encode_event_data((
				U256::from(product_id),
				parent_adapter_address,
				parent_chain_id,
				adapters,
			)),
		);
		handle.record_log_costs(&[&event])?;
		event.record(handle)?;

		Ok(())
	}

	/// Replace a product's entire MultichainAdapter routing table atomically.
	/// See `pallet_tranche_system::set_multichain_adapters`'s doc comment for
	/// full semantics.
	///
	/// @param product_id          The product whose MultichainAdapter table is being replaced
	/// @param multichain_adapters The full intended end-state list of routing entries
	#[precompile::public(
		"set_multichain_adapters(uint256,(address,uint64,uint16,(uint8,address,uint16,address,(address,uint256)[])[])[])"
	)]
	fn set_multichain_adapters(
		handle: &mut impl PrecompileHandle,
		product_id: U256,
		multichain_adapters: Vec<EvmMultichainAdapterInput>,
	) -> EvmResult {
		let caller = handle.context().caller;
		let caller_account = Runtime::AddressMapping::into_account_id(caller);
		let product_id = to_product_id(product_id)?;
		ensure_caller_is_product_admin::<Runtime>(product_id, &caller_account)?;
		let bounded_multichain_adapters =
			decode_multichain_adapters::<Runtime>(&multichain_adapters)?;

		let call = TrancheSystemCall::<Runtime>::set_multichain_adapters {
			product_id,
			multichain_adapters: bounded_multichain_adapters,
		};
		RuntimeHelper::<Runtime>::try_dispatch(
			handle,
			pallet_tranche_system::Origin::<Runtime>::ProductAdmin(caller_account).into(),
			call,
			0,
		)?;

		let event = log1(
			handle.context().address,
			SELECTOR_LOG_MULTICHAIN_ADAPTERS_SET,
			solidity::encode_event_data((U256::from(product_id), multichain_adapters)),
		);
		handle.record_log_costs(&[&event])?;
		event.record(handle)?;

		Ok(())
	}

	/// Replace a product's entire per-chain TrancheManager table atomically. See
	/// `pallet_tranche_system::set_multichain_tranche_managers`'s doc comment for full
	/// semantics.
	///
	/// @param product_id                  The product whose TrancheManager table is being replaced
	/// @param multichain_tranche_managers The full intended end-state list of per-chain
	/// bindings
	#[precompile::public("set_multichain_tranche_managers(uint256,(uint64,address)[])")]
	fn set_multichain_tranche_managers(
		handle: &mut impl PrecompileHandle,
		product_id: U256,
		multichain_tranche_managers: Vec<EvmMultichainTrancheManagerInput>,
	) -> EvmResult {
		let caller = handle.context().caller;
		let caller_account = Runtime::AddressMapping::into_account_id(caller);
		let product_id = to_product_id(product_id)?;
		ensure_caller_is_product_admin::<Runtime>(product_id, &caller_account)?;
		let bounded_multichain_tranche_managers =
			decode_multichain_tranche_managers(&multichain_tranche_managers)?;

		let call = TrancheSystemCall::<Runtime>::set_multichain_tranche_managers {
			product_id,
			multichain_tranche_managers: bounded_multichain_tranche_managers,
		};
		RuntimeHelper::<Runtime>::try_dispatch(
			handle,
			pallet_tranche_system::Origin::<Runtime>::ProductAdmin(caller_account).into(),
			call,
			0,
		)?;

		let event = log1(
			handle.context().address,
			SELECTOR_LOG_MULTICHAIN_TRANCHE_MANAGERS_SET,
			solidity::encode_event_data((U256::from(product_id), multichain_tranche_managers)),
		);
		handle.record_log_costs(&[&event])?;
		event.record(handle)?;

		Ok(())
	}

	/// Read a product's Valuation binding and settlement-cadence config.
	///
	/// @param product_id The product to look up
	/// @return base_asset, valuation_address, settlement_start_timestamp, settlement_length_secs,
	/// settlement_offset_secs
	#[precompile::public("get_product(uint256)")]
	#[precompile::view]
	fn get_product(
		handle: &mut impl PrecompileHandle,
		product_id: U256,
	) -> EvmResult<(Address, Address, u64, u64, u64)> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let product_id = to_product_id(product_id)?;
		let product = pallet_tranche_system::Products::<Runtime>::get(product_id)
			.ok_or_else(|| revert("product not found"))?;
		Ok((
			Address(product.valuation.base_asset),
			Address(product.valuation.valuation_address),
			product.valuation.settlement_start_timestamp,
			product.valuation.settlement_length_secs,
			product.valuation.settlement_offset_secs,
		))
	}

	/// Read a product's tranches, in waterfall priority order (index 0 = highest priority).
	///
	/// @param product_id The product to look up
	/// @return Tranche configs; `priority` in each entry reflects stored order, not
	/// the original create_product input
	#[precompile::public("get_tranches(uint256)")]
	#[precompile::view]
	fn get_tranches(
		handle: &mut impl PrecompileHandle,
		product_id: U256,
	) -> EvmResult<Vec<EvmTrancheInput>> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let product_id = to_product_id(product_id)?;
		let product = pallet_tranche_system::Products::<Runtime>::get(product_id)
			.ok_or_else(|| revert("product not found"))?;
		Ok(product
			.tranches
			.iter()
			.enumerate()
			.map(|(idx, tranche)| {
				let (tranche_type, apr) = match &tranche.tranche_type {
					TrancheType::Junior => (0u8, U256::zero()),
					TrancheType::Senior { apr } => (1u8, *apr),
				};
				(
					tranche_type,
					apr,
					(tranche.vault.chain_id, Address(tranche.vault.vault_address)),
					Address(tranche.asset),
					Address(tranche.shares),
					idx as u8,
				)
			})
			.collect())
	}

	/// Read a product's MultichainAdapter routing table, each entry carrying its own
	/// nested individual-Adapter registrations.
	///
	/// @param product_id The product to look up
	#[precompile::public("get_multichain_adapters(uint256)")]
	#[precompile::view]
	fn get_multichain_adapters(
		handle: &mut impl PrecompileHandle,
		product_id: U256,
	) -> EvmResult<Vec<EvmMultichainAdapterInput>> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let product_id = to_product_id(product_id)?;
		let product = pallet_tranche_system::Products::<Runtime>::get(product_id)
			.ok_or_else(|| revert("product not found"))?;
		Ok(product
			.multichain_adapters
			.iter()
			.map(|(key, info)| {
				let adapters: Vec<EvmAdapterInput> = info
					.adapters
					.iter()
					.map(|(address, adapter_info)| {
						let (source_type, borrower, collaterals) = match &adapter_info.source_type {
							SourceType::OffchainSource { borrower, collaterals } => (
								0u8,
								Address(borrower.clone().into()),
								collaterals
									.iter()
									.map(|c| (Address(c.nft_contract), c.nft_token_id))
									.collect::<Vec<EvmCollateralInput>>(),
							),
							SourceType::OnchainSource => (1u8, Address(H160::zero()), Vec::new()),
						};
						(
							source_type,
							Address(*address),
							adapter_info.weight_bps,
							borrower,
							collaterals,
						)
					})
					.collect();
				(Address(key.address), key.chain_id, info.weight_bps, adapters)
			})
			.collect())
	}

	/// Read a product's per-chain TrancheManager bindings — see
	/// `ProductDetails::multichain_tranche_managers`'s doc comment.
	///
	/// @param product_id The product to look up
	#[precompile::public("get_multichain_tranche_managers(uint256)")]
	#[precompile::view]
	fn get_multichain_tranche_managers(
		handle: &mut impl PrecompileHandle,
		product_id: U256,
	) -> EvmResult<Vec<EvmMultichainTrancheManagerInput>> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let product_id = to_product_id(product_id)?;
		let product = pallet_tranche_system::Products::<Runtime>::get(product_id)
			.ok_or_else(|| revert("product not found"))?;
		Ok(product
			.multichain_tranche_managers
			.iter()
			.map(|(chain_id, address)| (*chain_id, Address(*address)))
			.collect())
	}
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// `pallet_tranche_system::ProductId` is `u64`; `interface.sol` carries it as
/// `uint256`. Reverts rather than silently truncating if the caller passes a
/// value that doesn't fit.
fn to_product_id(product_id: U256) -> EvmResult<ProductId> {
	if product_id > U256::from(u64::MAX) {
		return Err(revert("product_id exceeds u64::MAX"));
	}
	Ok(product_id.as_u64())
}

/// Reads `pallet-tranche-permissions`' storage directly to confirm the caller
/// holds `ProductAdmin` for `product_id`, before constructing
/// `Origin::ProductAdmin` — shared by all four extrinsics in this precompile.
fn ensure_caller_is_product_admin<Runtime>(
	product_id: ProductId,
	caller_account: &Runtime::AccountId,
) -> EvmResult
where
	Runtime: pallet_tranche_permissions::Config,
{
	let is_admin = pallet_tranche_permissions::ProductAdmins::<Runtime>::get(product_id).as_ref()
		== Some(caller_account);
	if !is_admin {
		return Err(revert("caller does not hold ProductAdmin for product_id"));
	}
	Ok(())
}

fn decode_crud_action(action: u8) -> EvmResult<CrudAction> {
	match action {
		0 => Ok(CrudAction::Add),
		1 => Ok(CrudAction::Remove),
		2 => Ok(CrudAction::Update),
		_ => Err(revert("invalid action")),
	}
}

fn decode_tranche_type(tranche_type: u8, apr: U256) -> EvmResult<TrancheType> {
	match tranche_type {
		0 => Ok(TrancheType::Junior),
		1 => Ok(TrancheType::Senior { apr }),
		_ => Err(revert("invalid tranche_type")),
	}
}

/// `create_product`-only: each entry's `priority` is passed straight through
/// (not derived from array position) — the pallet sorts by it and reverts if
/// two entries share a `priority` or if the sorted order doesn't put every
/// Senior tranche before every Junior one. See `TrancheInput`'s doc comment.
fn decode_tranches(
	tranches: &[EvmTrancheInput],
) -> EvmResult<BoundedVec<TrancheInput, ConstU32<MAX_TRANCHES>>> {
	let mut bounded = BoundedVec::<TrancheInput, ConstU32<MAX_TRANCHES>>::default();
	for (tranche_type, apr, vault, asset, shares, priority) in tranches.iter().cloned() {
		let (chain_id, vault_address) = vault;
		let tranche_type = decode_tranche_type(tranche_type, apr)?;
		let vault_id = VaultId { chain_id, vault_address: vault_address.0 };
		bounded
			.try_push(TrancheInput {
				priority,
				tranche_type,
				vault: vault_id,
				asset: asset.0,
				shares: shares.0,
			})
			.map_err(|_| revert("too many tranches"))?;
	}
	Ok(bounded)
}

/// Decodes `SourceType` + `borrower`/`collaterals` (only meaningful when
/// `source_type == OffchainSource`, per interface.sol).
fn decode_source_type<Runtime>(
	source_type: u8,
	borrower: Address,
	collaterals: &[EvmCollateralInput],
) -> EvmResult<SourceType<Runtime::AccountId>>
where
	Runtime: pallet_evm::Config,
	<Runtime as pallet_evm::Config>::AddressMapping: AddressMapping<Runtime::AccountId>,
{
	match source_type {
		0 => {
			let mut bounded_collaterals =
				BoundedVec::<CollateralAsset, ConstU32<MAX_COLLATERALS>>::default();
			for (nft_contract, nft_token_id) in collaterals.iter().cloned() {
				bounded_collaterals
					.try_push(CollateralAsset { nft_contract: nft_contract.0, nft_token_id })
					.map_err(|_| revert("too many collaterals"))?;
			}
			let borrower_account = Runtime::AddressMapping::into_account_id(borrower.0);
			Ok(SourceType::OffchainSource {
				borrower: borrower_account,
				collaterals: bounded_collaterals,
			})
		},
		1 => Ok(SourceType::OnchainSource),
		_ => Err(revert("invalid source_type")),
	}
}

/// Decodes one parent MultichainAdapter's nested `adapters` array — shared by
/// `set_adapters` (standalone) and `decode_multichain_adapters` (nested
/// within `create_product`/`set_multichain_adapters`). Reverts on a duplicate
/// `source_address` — the incoming array has no uniqueness guarantee the way
/// a `BoundedBTreeMap` would, unlike `pallet_tranche_system`'s own storage.
fn decode_adapters<Runtime>(
	adapters: &[EvmAdapterInput],
) -> EvmResult<
	BoundedBTreeMap<
		H160,
		AdapterInfo<Runtime::AccountId>,
		ConstU32<MAX_ADAPTERS_PER_MULTICHAIN_ADAPTER>,
	>,
>
where
	Runtime: pallet_evm::Config,
	<Runtime as pallet_evm::Config>::AddressMapping: AddressMapping<Runtime::AccountId>,
{
	let mut map = BTreeMap::new();
	for (source_type, source_address, weight_bps, borrower, collaterals) in adapters.iter().cloned()
	{
		let decoded_source_type =
			decode_source_type::<Runtime>(source_type, borrower, &collaterals)?;
		let info = AdapterInfo { source_type: decoded_source_type, weight_bps };
		if map.insert(source_address.0, info).is_some() {
			return Err(revert("duplicate source_address in adapters"));
		}
	}
	BoundedBTreeMap::try_from(map).map_err(|_| revert("too many adapters"))
}

/// Decodes a `multichain_adapters` array. Reverts on a duplicate
/// `(adapter_address, chain_id)` pair, same reasoning as `decode_adapters`.
fn decode_multichain_adapters<Runtime>(
	multichain_adapters: &[EvmMultichainAdapterInput],
) -> EvmResult<
	BoundedBTreeMap<
		AdapterKey,
		MultichainAdapterInfo<Runtime::AccountId>,
		ConstU32<MAX_MULTICHAIN_ADAPTERS>,
	>,
>
where
	Runtime: pallet_evm::Config,
	<Runtime as pallet_evm::Config>::AddressMapping: AddressMapping<Runtime::AccountId>,
{
	let mut map = BTreeMap::new();
	for (adapter_address, chain_id, weight_bps, adapters) in multichain_adapters.iter().cloned() {
		let key = AdapterKey { address: adapter_address.0, chain_id };
		let decoded_adapters = decode_adapters::<Runtime>(&adapters)?;
		let info = MultichainAdapterInfo { weight_bps, adapters: decoded_adapters };
		if map.insert(key, info).is_some() {
			return Err(revert("duplicate (adapter_address, chain_id) in multichain_adapters"));
		}
	}
	BoundedBTreeMap::try_from(map).map_err(|_| revert("too many multichain_adapters"))
}

/// Decodes a `multichain_tranche_managers` array. Reverts on a duplicate
/// `chain_id`, same reasoning as `decode_multichain_adapters` — the incoming
/// array has no uniqueness guarantee the way a `BoundedBTreeMap` would.
/// Hub-chain exclusion is validated pallet-side (`create_product` reverts on
/// it), not here.
fn decode_multichain_tranche_managers(
	multichain_tranche_managers: &[EvmMultichainTrancheManagerInput],
) -> EvmResult<BoundedBTreeMap<u64, H160, ConstU32<MAX_TRANCHE_MANAGERS>>> {
	let mut map = BTreeMap::new();
	for (chain_id, tranche_manager_address) in multichain_tranche_managers.iter().cloned() {
		if map.insert(chain_id, tranche_manager_address.0).is_some() {
			return Err(revert("duplicate chain_id in multichain_tranche_managers"));
		}
	}
	BoundedBTreeMap::try_from(map).map_err(|_| revert("too many multichain_tranche_managers"))
}
