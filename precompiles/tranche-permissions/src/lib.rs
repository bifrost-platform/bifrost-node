#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]

use frame_support::dispatch::{GetDispatchInfo, PostDispatchInfo};
use pallet_evm::AddressMapping;
use pallet_tranche_permissions::{Call as TranchePermissionsCall, Role};
use pallet_tranche_system::VaultId;
use precompile_utils::prelude::*;
use sp_core::{H160, U256};
use sp_runtime::traits::Dispatchable;
use sp_std::marker::PhantomData;

// ---------------------------------------------------------------------------
// Event log selectors
// ---------------------------------------------------------------------------

pub(crate) const SELECTOR_LOG_PERMISSION_GRANTED: [u8; 32] =
	keccak256!("PermissionGranted(uint256,uint8,address,uint64,address)");
pub(crate) const SELECTOR_LOG_PERMISSION_REVOKED: [u8; 32] =
	keccak256!("PermissionRevoked(uint256,uint8,address,uint64,address)");

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
	Runtime: pallet_tranche_permissions::Config + pallet_evm::Config + frame_system::Config,
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
	#[precompile::public("grant_permission(uint256,uint8,address,(uint64,address))")]
	fn grant_permission(
		handle: &mut impl PrecompileHandle,
		product_id: U256,
		role: u8,
		who: Address,
		vault: EvmVaultInput,
	) -> EvmResult {
		let caller = handle.context().caller;
		let caller_account = Runtime::AddressMapping::into_account_id(caller);
		let product_id = to_product_id(product_id)?;
		let who_account = Runtime::AddressMapping::into_account_id(who.0);
		let (decoded_role, vault_chain_id, vault_address) = decode_role(role, vault)?;

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
				U256::from(product_id),
				role,
				who,
				vault_chain_id,
				Address(vault_address),
			)),
		);
		handle.record_log_costs(&[&event])?;
		event.record(handle)?;

		Ok(())
	}

	/// Revoke `role` from `who` for `product_id`. Same authorization and
	/// `vault` usage rules as `grant_permission`.
	///
	/// @param product_id Hub product ID
	/// @param role       0 = ProductAdmin, 1 = OracleFeeder, 2 = TrancheInvestor
	/// @param who        EVM address losing the role
	/// @param vault      TrancheInvestor-only: (chain_id, vault_address) identifying the tranche
	#[precompile::public("revoke_permission(uint256,uint8,address,(uint64,address))")]
	fn revoke_permission(
		handle: &mut impl PrecompileHandle,
		product_id: U256,
		role: u8,
		who: Address,
		vault: EvmVaultInput,
	) -> EvmResult {
		let caller = handle.context().caller;
		let caller_account = Runtime::AddressMapping::into_account_id(caller);
		let product_id = to_product_id(product_id)?;
		let who_account = Runtime::AddressMapping::into_account_id(who.0);
		let (decoded_role, vault_chain_id, vault_address) = decode_role(role, vault)?;

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
				U256::from(product_id),
				role,
				who,
				vault_chain_id,
				Address(vault_address),
			)),
		);
		handle.record_log_costs(&[&event])?;
		event.record(handle)?;

		Ok(())
	}
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// `pallet_tranche_system::ProductId` is `u64`; `interface.sol` carries it as
/// `uint256`. Reverts rather than silently truncating if the caller passes a
/// value that doesn't fit.
fn to_product_id(product_id: U256) -> EvmResult<pallet_tranche_system::ProductId> {
	if product_id > U256::from(u64::MAX) {
		return Err(revert("product_id exceeds u64::MAX"));
	}
	Ok(product_id.as_u64())
}

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
