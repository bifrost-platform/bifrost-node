#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]

use frame_support::dispatch::{GetDispatchInfo, PostDispatchInfo};

use pallet_cccp_relay_queue::Call as CCCPRelayQueueCall;
use pallet_evm::AddressMapping;

use precompile_utils::prelude::*;

use fp_account::EthereumSignature;
use sp_core::{H160, H256, U256};
use sp_runtime::traits::Dispatchable;
use sp_std::marker::PhantomData;

/// A precompile to wrap the functionality from `pallet_cccp_relay_queue`.
///
/// This precompile exposes CCCP (Cross-Chain Communication Protocol) relay queue
/// functionality to EVM smart contracts, enabling cross-chain asset transfers
/// and oracle management through Solidity interfaces.
pub struct CCCPRelayQueuePrecompile<Runtime>(PhantomData<Runtime>);

#[precompile]
impl<Runtime> CCCPRelayQueuePrecompile<Runtime>
where
	Runtime: pallet_cccp_relay_queue::Config<Signature = EthereumSignature>
		+ pallet_evm::Config
		+ frame_system::Config,
	Runtime::AccountId: Into<H160> + From<H160>,
	Runtime::RuntimeCall: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	<Runtime::RuntimeCall as Dispatchable>::RuntimeOrigin: From<Option<Runtime::AccountId>>,
	Runtime::RuntimeCall: From<CCCPRelayQueueCall<Runtime>>,
	<Runtime as pallet_evm::Config>::AddressMapping: AddressMapping<Runtime::AccountId>,
	pallet_cccp_relay_queue::BalanceOf<Runtime>: TryFrom<U256> + Into<U256>,
{
	/// Get the oracle ID for an asset by its asset index hash.
	///
	/// This function performs a two-step lookup:
	/// 1. Resolves the asset index hash to an asset ID using `AssetIndexes` storage
	/// 2. Retrieves the oracle ID for that asset ID from `AssetOracles` storage
	///
	/// # Parameters
	/// - `asset_index_hash`: The H256 hash identifying the asset in CCCP protocol
	///
	/// # Returns
	/// - `H256`: The oracle ID (H256) if found, zero hash otherwise
	///
	/// # Gas Cost
	/// - 2 database reads (one for AssetIndexes, one for AssetOracles)
	#[precompile::public("getAssetOracleIdByHash(bytes32)")]
	#[precompile::public("get_asset_oracle_id_by_hash(bytes32)")]
	#[precompile::view]
	fn get_asset_oracle_id_by_hash(
		handle: &mut impl PrecompileHandle,
		asset_index_hash: H256,
	) -> EvmResult<H256> {
		// Step 1: Get asset_id from asset_index_hash
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let asset_id = if let Some(asset_id) =
			pallet_cccp_relay_queue::AssetIndexes::<Runtime>::get(asset_index_hash)
		{
			asset_id
		} else {
			// Asset index not found, return zero address
			return Ok(H256::zero());
		};

		// Step 2: Get oracle_id from asset_id
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let oracle_id = if let Some(oracle_id) =
			pallet_cccp_relay_queue::AssetOracles::<Runtime>::get(asset_id)
		{
			oracle_id
		} else {
			// Oracle not found for this asset, return zero address
			return Ok(H256::zero());
		};

		Ok(oracle_id)
	}

	/// Get the native currency oracle ID for a chain.
	///
	/// This function retrieves the native currency oracle ID for a given chain ID from the
	/// `NativeCurrencyOracles` storage.
	///
	/// # Parameters
	/// - `chain_id`: The chain ID (u32)
	///
	/// # Returns
	/// - `H256`: The native currency oracle ID (H256) if found, zero hash otherwise
	///
	/// # Gas Cost
	/// - 1 database read (for NativeCurrencyOracles)
	#[precompile::public("getNativeCurrencyOracleId(uint32)")]
	#[precompile::public("get_native_currency_oracle_id(uint32)")]
	#[precompile::view]
	fn get_native_currency_oracle_id(
		handle: &mut impl PrecompileHandle,
		chain_id: u32,
	) -> EvmResult<H256> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let native_currency_oracle = if let Some(native_currency_oracle) =
			pallet_cccp_relay_queue::NativeCurrencyOracles::<Runtime>::get(chain_id)
		{
			native_currency_oracle
		} else {
			return Ok(H256::zero());
		};
		Ok(native_currency_oracle)
	}
}
