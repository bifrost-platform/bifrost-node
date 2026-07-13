#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]

use frame_support::dispatch::{GetDispatchInfo, PostDispatchInfo};
use pallet_evm::AddressMapping;
use pallet_rwa_permissions::{Call as PermissionsCall, Role};
use pallet_rwa_pools::{PoolInspect, TrancheId};
use precompile_utils::prelude::*;
use sp_core::{H160, U256};
use sp_runtime::traits::Dispatchable;
use sp_std::{marker::PhantomData, vec::Vec};

// ---------------------------------------------------------------------------
// Event log selectors
// ---------------------------------------------------------------------------

pub(crate) const SELECTOR_LOG_TRANCHE_INVESTOR_ADDED: [u8; 32] =
	keccak256!("TrancheInvestorAdded(uint64,uint64,address,address)");
pub(crate) const SELECTOR_LOG_TRANCHE_INVESTOR_REMOVED: [u8; 32] =
	keccak256!("TrancheInvestorRemoved(uint64,uint64,address,address)");

// ---------------------------------------------------------------------------
// Gateway call selectors
//
// These encode the provisional Gateway interface used to propagate whitelist
// changes cross-chain via CCCP-v2.  Only the first 4 bytes (keccak256 selector)
// are sent on-chain, so the signatures can be updated when the Gateway contract
// is finalised without changing stored data.
// ---------------------------------------------------------------------------

const GATEWAY_GRANT_TRANCHE_INVESTOR: [u8; 32] =
	keccak256!("grantTrancheInvestor(uint64,address,address)");
const GATEWAY_REVOKE_TRANCHE_INVESTOR: [u8; 32] =
	keccak256!("revokeTrancheInvestor(uint64,address,address)");

// ---------------------------------------------------------------------------
// Precompile
// ---------------------------------------------------------------------------

/// A precompile that manages the TrancheInvestor whitelist on the Hub chain
/// and propagates changes to the Spoke chain via the Gateway contract.
///
/// Called directly by Pool Admin EOAs — not by the Gateway — so origins are
/// resolved from `handle.context().caller` as signed substrate accounts.
pub struct PermissionsPrecompile<Runtime>(PhantomData<Runtime>);

#[precompile_utils::precompile]
impl<Runtime> PermissionsPrecompile<Runtime>
where
	Runtime: pallet_rwa_permissions::Config + pallet_evm::Config + frame_system::Config,
	Runtime::RuntimeCall: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	Runtime::RuntimeCall: From<PermissionsCall<Runtime>>,
	<Runtime as pallet_evm::Config>::AddressMapping: AddressMapping<Runtime::AccountId>,
	<Runtime as pallet_rwa_permissions::Config>::Pools: PoolInspect,
{
	/// Whitelist `investor` as a TrancheInvestor on the Hub, then propagate
	/// the grant to the Spoke chain via the Gateway.
	///
	/// Caller must hold the PoolAdmin role for `pool_id`.
	///
	/// @param pool_id       Hub pool ID
	/// @param chain_id      EVM chain ID where the vault is deployed
	/// @param vault_address ERC-7540 vault contract address on the Spoke chain
	/// @param investor_id   Investor address to whitelist
	#[precompile::public("add_tranche_investor(uint64,uint64,address,address)")]
	fn add_tranche_investor(
		handle: &mut impl PrecompileHandle,
		pool_id: u64,
		chain_id: u64,
		vault_address: Address,
		investor_id: Address,
	) -> EvmResult {
		let caller = handle.context().caller;
		let caller_account = Runtime::AddressMapping::into_account_id(caller);
		let investor_id: H160 = investor_id.0;
		let investor_account = Runtime::AddressMapping::into_account_id(investor_id);
		let tranche_id = TrancheId { chain_id, vault_address: vault_address.0 };

		let call = PermissionsCall::<Runtime>::grant_permission {
			pool_id,
			role: Role::TrancheInvestor(tranche_id),
			who: investor_account,
		};
		RuntimeHelper::<Runtime>::try_dispatch(
			handle,
			frame_system::RawOrigin::Signed(caller_account).into(),
			call,
			0,
		)?;

		Self::gateway_subcall(
			handle,
			&GATEWAY_GRANT_TRANCHE_INVESTOR,
			chain_id,
			vault_address.0,
			investor_id,
			"gateway: grantTrancheInvestor failed",
		)?;

		let event = log1(
			handle.context().address,
			SELECTOR_LOG_TRANCHE_INVESTOR_ADDED,
			solidity::encode_event_data((
				U256::from(pool_id),
				U256::from(chain_id),
				vault_address,
				Address(investor_id),
			)),
		);
		handle.record_log_costs(&[&event])?;
		event.record(handle)?;

		Ok(())
	}

	/// Remove `investor` from the TrancheInvestor whitelist on the Hub, then
	/// propagate the revocation to the Spoke chain via the Gateway.
	///
	/// Caller must hold the PoolAdmin role for `pool_id`.
	///
	/// @param pool_id       Hub pool ID
	/// @param chain_id      EVM chain ID where the vault is deployed
	/// @param vault_address ERC-7540 vault contract address on the Spoke chain
	/// @param investor_id   Investor address to remove
	#[precompile::public("remove_tranche_investor(uint64,uint64,address,address)")]
	fn remove_tranche_investor(
		handle: &mut impl PrecompileHandle,
		pool_id: u64,
		chain_id: u64,
		vault_address: Address,
		investor_id: Address,
	) -> EvmResult {
		let caller = handle.context().caller;
		let caller_account = Runtime::AddressMapping::into_account_id(caller);
		let investor_id: H160 = investor_id.0;
		let investor_account = Runtime::AddressMapping::into_account_id(investor_id);
		let tranche_id = TrancheId { chain_id, vault_address: vault_address.0 };

		let call = PermissionsCall::<Runtime>::revoke_permission {
			pool_id,
			role: Role::TrancheInvestor(tranche_id),
			who: investor_account,
		};
		RuntimeHelper::<Runtime>::try_dispatch(
			handle,
			frame_system::RawOrigin::Signed(caller_account).into(),
			call,
			0,
		)?;

		Self::gateway_subcall(
			handle,
			&GATEWAY_REVOKE_TRANCHE_INVESTOR,
			chain_id,
			vault_address.0,
			investor_id,
			"gateway: revokeTrancheInvestor failed",
		)?;

		let event = log1(
			handle.context().address,
			SELECTOR_LOG_TRANCHE_INVESTOR_REMOVED,
			solidity::encode_event_data((
				U256::from(pool_id),
				U256::from(chain_id),
				vault_address,
				Address(investor_id),
			)),
		);
		handle.record_log_costs(&[&event])?;
		event.record(handle)?;

		Ok(())
	}

	// -------------------------------------------------------------------------
	// Helpers
	// -------------------------------------------------------------------------

	/// ABI-encode and dispatch a call to the Bifrost Gateway contract as a nested EVM
	/// sub-call.  Skipped silently when the Gateway address is zero (not yet configured),
	/// allowing Hub-side logic to be exercised independently.
	///
	/// This must go through `handle.call(..)` (a sub-call within the current EVM execution)
	/// rather than `Runner::call(..)`, which starts a brand-new top-level EVM execution and
	/// trips pallet-evm's `forbid-evm-reentrancy` guard since we're already inside one.
	fn gateway_subcall(
		handle: &mut impl PrecompileHandle,
		selector_full: &[u8; 32],
		chain_id: u64,
		vault_address: H160,
		investor_id: H160,
		revert_msg: &'static str,
	) -> EvmResult {
		let gateway = <Runtime as pallet_rwa_permissions::Config>::Pools::gateway_address();
		if gateway == H160::zero() {
			return Ok(());
		}

		let mut input: Vec<u8> = Vec::with_capacity(4 + 96);
		input.extend_from_slice(&selector_full[..4]);
		input.extend_from_slice(&solidity::encode_arguments((
			U256::from(chain_id),
			Address(vault_address),
			Address(investor_id),
		)));

		let context = pallet_evm::Context {
			address: gateway,
			caller: handle.context().address,
			apparent_value: U256::zero(),
		};
		let gas_limit = handle.remaining_gas();

		let (exit_reason, _output) =
			handle.call(gateway, None, input, Some(gas_limit), false, &context);

		if !matches!(exit_reason, pallet_evm::ExitReason::Succeed(_)) {
			return Err(revert(revert_msg));
		}

		Ok(())
	}
}
