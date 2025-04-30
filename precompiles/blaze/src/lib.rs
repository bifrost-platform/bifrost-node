#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::dispatch::{GetDispatchInfo, PostDispatchInfo};

use pallet_blaze::{Call as BlazeCall, UtxoStatus};
use pallet_evm::AddressMapping;
use precompile_utils::prelude::*;

use sp_core::{H160, H256, U256};
use sp_io::hashing::keccak_256;
use sp_runtime::traits::Dispatchable;
use sp_std::marker::PhantomData;

use parity_scale_codec::Encode;

/// A precompile to wrap the functionality from `pallet_blaze`.
pub struct BlazePrecompile<Runtime>(PhantomData<Runtime>);

#[precompile]
impl<Runtime> BlazePrecompile<Runtime>
where
	Runtime: pallet_blaze::Config + pallet_evm::Config + frame_system::Config,
	Runtime::AccountId: Into<H160> + From<H160>,
	Runtime::RuntimeCall: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	<Runtime::RuntimeCall as Dispatchable>::RuntimeOrigin: From<Option<Runtime::AccountId>>,
	Runtime::RuntimeCall: From<BlazeCall<Runtime>>,
{
	#[precompile::public("isActivated()")]
	#[precompile::public("is_activated()")]
	#[precompile::view]
	fn is_activated(handle: &mut impl PrecompileHandle) -> EvmResult<bool> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		Ok(pallet_blaze::IsActivated::<Runtime>::get())
	}

	#[precompile::public("isSubmittableUtxo(bytes32,uint256,uint256,address)")]
	#[precompile::public("is_submittable_utxo(bytes32,uint256,uint256,address)")]
	#[precompile::view]
	fn is_submittable_utxo(
		handle: &mut impl PrecompileHandle,
		txid: H256,
		vout: U256,
		amount: U256,
		authority_id: Address,
	) -> EvmResult<bool> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let authority_id = Runtime::AddressMapping::into_account_id(authority_id.0);
		let utxo_hash =
			H256::from_slice(keccak_256(&Encode::encode(&(txid, vout, amount))).as_ref());

		Ok(match pallet_blaze::Utxos::<Runtime>::get(&utxo_hash) {
			Some(utxo) => {
				utxo.status == UtxoStatus::Unconfirmed && !utxo.voters.contains(&authority_id)
			},
			None => true,
		})
	}
}
