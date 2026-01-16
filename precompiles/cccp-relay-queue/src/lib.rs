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
	/// Get the oracle address for an asset by its asset index hash.
	///
	/// This function performs a two-step lookup:
	/// 1. Resolves the asset index hash to an asset ID using `AssetIndexes` storage
	/// 2. Retrieves the oracle address for that asset ID from `AssetOracles` storage
	///
	/// # Parameters
	/// - `asset_index_hash`: The H256 hash identifying the asset in CCCP protocol
	///
	/// # Returns
	/// - `Address`: The oracle address (H160) if found, zero address otherwise
	///
	/// # Gas Cost
	/// - 2 database reads (one for AssetIndexes, one for AssetOracles)
	#[precompile::public("getAssetOracleByHash(bytes32)")]
	#[precompile::public("get_asset_oracle_by_hash(bytes32)")]
	#[precompile::view]
	fn get_asset_oracle_by_hash(
		handle: &mut impl PrecompileHandle,
		asset_index_hash: H256,
	) -> EvmResult<Address> {
		// Step 1: Get asset_id from asset_index_hash
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let asset_id = if let Some(asset_id) =
			pallet_cccp_relay_queue::AssetIndexes::<Runtime>::get(asset_index_hash)
		{
			asset_id
		} else {
			// Asset index not found, return zero address
			return Ok(Address(H160::zero()));
		};

		// Step 2: Get oracle_id from asset_id
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let oracle_id = if let Some(oracle_id) =
			pallet_cccp_relay_queue::AssetOracles::<Runtime>::get(asset_id)
		{
			oracle_id
		} else {
			// Oracle not found for this asset, return zero address
			return Ok(Address(H160::zero()));
		};

		Ok(Address(oracle_id))
	}
}
