#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]

extern crate alloc;

use alloc::format;
use frame_support::dispatch::{GetDispatchInfo, PostDispatchInfo};
use pallet_evm::{AddressMapping, Context, ExitReason};
use pallet_tranche_permissions::{Call as TranchePermissionsCall, Role};
use pallet_tranche_system::{ProductId, VaultId};
use precompile_utils::prelude::*;
use sp_core::{H160, U256};
use sp_runtime::traits::Dispatchable;
use sp_std::marker::PhantomData;

// ---------------------------------------------------------------------------
// Event log selectors
// ---------------------------------------------------------------------------

pub(crate) const SELECTOR_LOG_PERMISSION_GRANTED: [u8; 32] =
	keccak256!("PermissionGranted(uint64,uint8,address,uint64,address)");
pub(crate) const SELECTOR_LOG_PERMISSION_REVOKED: [u8; 32] =
	keccak256!("PermissionRevoked(uint64,uint8,address,uint64,address)");

/// `Orchestrator.sendWhitelist(uint64,uint256,address,address,uint8)` selector
/// (`cast sig "sendWhitelist(uint64,uint256,address,address,uint8)"`).
const ORCHESTRATOR_SEND_WHITELIST_SELECTOR: [u8; 4] = [0xc8, 0x06, 0x2a, 0x4a];

/// Gas limit for the `Orchestrator.sendWhitelist` subcall.
const ORCHESTRATOR_CALL_GAS_LIMIT: u64 = 1_000_000;

/// `interface.sol`'s `VaultInput` struct, decoded positionally as a tuple —
/// `(chain_id, vault_address)`.
type EvmVaultInput = (u64, Address);

// ---------------------------------------------------------------------------
// Precompile
// ---------------------------------------------------------------------------

/// A precompile that wraps `pallet_tranche_permissions`'s `grant_permission`/
/// `revoke_permission` extrinsics.
///
/// Called directly by ProductAdmin EOAs — not by a Gateway — so origins are
/// resolved from `handle.context().caller` as signed substrate accounts.
/// `Role::ProductAdmin` grants/revokes always revert through this precompile:
/// the pallet requires a root origin for that role, and a precompile-dispatched
/// call can only ever construct a signed origin (see `grant_permission`'s
/// doc comment in `pallet_tranche_permissions`).
pub struct TranchePermissionsPrecompile<Runtime>(PhantomData<Runtime>);

#[precompile_utils::precompile]
impl<Runtime> TranchePermissionsPrecompile<Runtime>
where
	Runtime: pallet_tranche_permissions::Config
		+ pallet_tranche_system::Config
		+ pallet_evm::Config
		+ frame_system::Config,
	Runtime::RuntimeCall: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	Runtime::RuntimeCall: From<TranchePermissionsCall<Runtime>>,
	<Runtime as pallet_evm::Config>::AddressMapping: AddressMapping<Runtime::AccountId>,
{
	/// Grant `role` to `who` for `product_id`. See `Role`'s encoding below and
	/// `pallet_tranche_permissions::grant_permission`'s doc comment for full
	/// authorization rules.
	///
	/// `vault` is only used when `role == TrancheInvestor` (`role == 2`); pass
	/// zero/default otherwise.
	///
	/// @param product_id Hub product ID
	/// @param role       0 = ProductAdmin, 1 = OracleFeeder, 2 = TrancheInvestor
	/// @param who        EVM address receiving the role
	/// @param vault      TrancheInvestor-only: (chain_id, vault_address) identifying the tranche
	#[precompile::public("grant_permission(uint64,uint8,address,(uint64,address))")]
	fn grant_permission(
		handle: &mut impl PrecompileHandle,
		product_id: ProductId,
		role: u8,
		who: Address,
		vault: EvmVaultInput,
	) -> EvmResult {
		let caller = handle.context().caller;
		let caller_account = Runtime::AddressMapping::into_account_id(caller);
		let who_account = Runtime::AddressMapping::into_account_id(who.0);
		let (decoded_role, vault_chain_id, vault_address) = decode_role(role, vault)?;
		let propagate_vault = match &decoded_role {
			Role::TrancheInvestor(vault) => Some(vault.clone()),
			_ => None,
		};

		let call = TranchePermissionsCall::<Runtime>::grant_permission {
			product_id,
			role: decoded_role,
			who: who_account,
		};
		RuntimeHelper::<Runtime>::try_dispatch(
			handle,
			frame_system::RawOrigin::Signed(caller_account).into(),
			call,
			0,
		)?;

		let event = log1(
			handle.context().address,
			SELECTOR_LOG_PERMISSION_GRANTED,
			solidity::encode_event_data((
				product_id,
				role,
				who,
				vault_chain_id,
				Address(vault_address),
			)),
		);
		handle.record_log_costs(&[&event])?;
		event.record(handle)?;

		if let Some(vault) = propagate_vault {
			propagate_whitelist_change::<Runtime>(handle, product_id, &vault, who.0, 1)?;
		}

		Ok(())
	}

	/// Revoke `role` from `who` for `product_id`. Same authorization and
	/// `vault` usage rules as `grant_permission`.
	///
	/// @param product_id Hub product ID
	/// @param role       0 = ProductAdmin, 1 = OracleFeeder, 2 = TrancheInvestor
	/// @param who        EVM address losing the role
	/// @param vault      TrancheInvestor-only: (chain_id, vault_address) identifying the tranche
	#[precompile::public("revoke_permission(uint64,uint8,address,(uint64,address))")]
	fn revoke_permission(
		handle: &mut impl PrecompileHandle,
		product_id: ProductId,
		role: u8,
		who: Address,
		vault: EvmVaultInput,
	) -> EvmResult {
		let caller = handle.context().caller;
		let caller_account = Runtime::AddressMapping::into_account_id(caller);
		let who_account = Runtime::AddressMapping::into_account_id(who.0);
		let (decoded_role, vault_chain_id, vault_address) = decode_role(role, vault)?;
		let propagate_vault = match &decoded_role {
			Role::TrancheInvestor(vault) => Some(vault.clone()),
			_ => None,
		};

		let call = TranchePermissionsCall::<Runtime>::revoke_permission {
			product_id,
			role: decoded_role,
			who: who_account,
		};
		RuntimeHelper::<Runtime>::try_dispatch(
			handle,
			frame_system::RawOrigin::Signed(caller_account).into(),
			call,
			0,
		)?;

		let event = log1(
			handle.context().address,
			SELECTOR_LOG_PERMISSION_REVOKED,
			solidity::encode_event_data((
				product_id,
				role,
				who,
				vault_chain_id,
				Address(vault_address),
			)),
		);
		handle.record_log_costs(&[&event])?;
		event.record(handle)?;

		if let Some(vault) = propagate_vault {
			propagate_whitelist_change::<Runtime>(handle, product_id, &vault, who.0, 0)?;
		}

		Ok(())
	}

	/// Read whether `who` currently holds the TrancheInvestor whitelist for `vault`.
	///
	/// `product_id` is accepted for signature symmetry with the rest of this
	/// interface (mirrors `grant_permission`) but isn't part of the actual
	/// check — `TrancheInvestors` is keyed by `vault` alone (globally unique,
	/// enforced by pallet-tranche-system), same as
	/// `pallet_tranche_permissions`'s own `has_role`'s TrancheInvestor arm.
	///
	/// @param product_id Accepted for signature symmetry; not used in the lookup itself
	/// @param vault      (chain_id, vault_address) identifying the tranche whose whitelist
	/// is being checked
	/// @param who        EVM address to check
	#[precompile::public("is_tranche_investor(uint64,(uint64,address),address)")]
	#[precompile::view]
	fn is_tranche_investor(
		handle: &mut impl PrecompileHandle,
		_product_id: ProductId,
		vault: EvmVaultInput,
		who: Address,
	) -> EvmResult<bool> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let (vault_chain_id, vault_address) = vault;
		let vault = VaultId { chain_id: vault_chain_id, vault_address: vault_address.0 };
		let who_account = Runtime::AddressMapping::into_account_id(who.0);
		Ok(pallet_tranche_permissions::TrancheInvestors::<Runtime>::contains_key(
			vault,
			who_account,
		))
	}

	/// Read whether `who` holds `role` for `product_id`. Only `ProductAdmin` (0) and
	/// `OracleFeeder` (1) are supported here — `TrancheInvestor` (2) has no `vault`
	/// parameter on this signature, so use `is_tranche_investor` instead.
	///
	/// @param product_id The product to check
	/// @param role       0 = ProductAdmin, 1 = OracleFeeder (2 = TrancheInvestor reverts)
	/// @param who        EVM address to check
	#[precompile::public("has_role(uint64,uint8,address)")]
	#[precompile::view]
	fn has_role(
		handle: &mut impl PrecompileHandle,
		product_id: ProductId,
		role: u8,
		who: Address,
	) -> EvmResult<bool> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let who_account = Runtime::AddressMapping::into_account_id(who.0);
		match role {
			0 => Ok(pallet_tranche_permissions::ProductAdmins::<Runtime>::get(product_id).as_ref()
				== Some(&who_account)),
			1 => Ok(pallet_tranche_permissions::OracleFeeders::<Runtime>::contains_key(
				product_id,
				&who_account,
			)),
			2 => Err(revert("role TrancheInvestor requires a vault — use is_tranche_investor")),
			_ => Err(revert("invalid role")),
		}
	}
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Decodes the raw `role` discriminant + `vault` tuple into a `Role`, plus the
/// `(chain_id, vault_address)` pair to echo back in the emitted event —
/// `(0, H160::zero())` for non-`TrancheInvestor` roles, regardless of what the
/// caller passed in `vault` (interface.sol only asks for zero/default there;
/// this guarantees the emitted event reflects that even if a caller doesn't).
fn decode_role(role: u8, vault: EvmVaultInput) -> EvmResult<(Role, u64, H160)> {
	match role {
		0 => Ok((Role::ProductAdmin, 0, H160::zero())),
		1 => Ok((Role::OracleFeeder, 0, H160::zero())),
		2 => {
			let (chain_id, vault_address) = vault;
			let vault_id = VaultId { chain_id, vault_address: vault_address.0 };
			Ok((Role::TrancheInvestor(vault_id), chain_id, vault_address.0))
		},
		_ => Err(revert("invalid role")),
	}
}

/// Propagates a `TrancheInvestor` grant/revoke to the Spoke chain `vault`
/// lives on, by calling `Orchestrator.sendWhitelist(chainId, productId,
/// vaultAddress, who, action)` as a subcall from this precompile's own
/// address. Uses `handle.call`, never `Runner::call` — this precompile's own
/// EVM execution is already inside pallet-evm's `forbid-evm-reentrancy`
/// guard, so a fresh top-level `Runner::call` would trip
/// `pallet_evm::Error::Reentrancy`. Requires this precompile's runtime
/// checks tuple to include `SubcallWithMaxNesting`, or the subcall is
/// rejected before it ever reaches Orchestrator.
///
/// Skipped entirely if `vault`'s chain is this Hub chain's own EVM chain ID —
/// a Hub-issued product's tranche needs no cross-chain propagation; it's
/// registered locally by `grant_permission`/`revoke_permission` alone.
///
/// Reverts (rolling back the permission grant/revoke too, atomically) if
/// `OrchestratorAddress` isn't configured yet, or if the subcall itself
/// fails — propagation *triggering* is meant to be atomic with the on-chain
/// grant; only what happens after the Orchestrator call (Spoke-side relay)
/// is safe to retry independently of this transaction.
fn propagate_whitelist_change<Runtime>(
	handle: &mut impl PrecompileHandle,
	product_id: pallet_tranche_system::ProductId,
	vault: &VaultId,
	who: H160,
	action: u8,
) -> EvmResult
where
	Runtime: pallet_tranche_system::Config + pallet_evm::Config,
{
	let orchestrator = pallet_tranche_system::OrchestratorAddress::<Runtime>::get();
	if orchestrator == H160::zero() {
		return Err(revert("orchestrator address not configured"));
	}

	let calldata =
		encode_send_whitelist(vault.chain_id, product_id, vault.vault_address, who, action);
	let context = Context {
		address: orchestrator,
		caller: handle.context().address,
		apparent_value: U256::zero(),
	};
	let (exit_reason, _) = handle.call(
		orchestrator,
		None,
		calldata,
		Some(ORCHESTRATOR_CALL_GAS_LIMIT),
		false,
		&context,
	);
	match exit_reason {
		ExitReason::Succeed(_) => Ok(()),
		other => Err(revert(format!("orchestrator sendWhitelist call failed: {other:?}"))),
	}
}

/// ABI-encodes `Orchestrator.sendWhitelist(uint64,uint256,address,address,uint8)`'s
/// calldata — five statically-sized parameters, so a flat selector + five
/// 32-byte left-padded slots, no dynamic offsets needed.
fn encode_send_whitelist(
	chain_id: u64,
	product_id: pallet_tranche_system::ProductId,
	vault_address: H160,
	who: H160,
	action: u8,
) -> sp_std::vec::Vec<u8> {
	let mut calldata = sp_std::vec::Vec::with_capacity(4 + 32 * 5);
	calldata.extend_from_slice(&ORCHESTRATOR_SEND_WHITELIST_SELECTOR);

	calldata.extend_from_slice(&[0u8; 24]);
	calldata.extend_from_slice(&chain_id.to_be_bytes());

	let product_id_bytes: [u8; 32] = U256::from(product_id).to_big_endian();
	calldata.extend_from_slice(&product_id_bytes);

	calldata.extend_from_slice(&[0u8; 12]);
	calldata.extend_from_slice(vault_address.as_bytes());

	calldata.extend_from_slice(&[0u8; 12]);
	calldata.extend_from_slice(who.as_bytes());

	calldata.extend_from_slice(&[0u8; 31]);
	calldata.push(action);

	calldata
}
