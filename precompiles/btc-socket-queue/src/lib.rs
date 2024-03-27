#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::dispatch::{GetDispatchInfo, PostDispatchInfo};

use pallet_btc_socket_queue::Call as BtcSocketQueueCall;

use precompile_utils::prelude::*;

use fp_account::EthereumSignature;
use sp_core::H160;
use sp_runtime::traits::Dispatchable;
use sp_std::{marker::PhantomData, vec, vec::Vec};

/// A precompile to wrap the functionality from `pallet_btc_socket_queue`.
pub struct BtcSocketQueuePrecompile<Runtime>(PhantomData<Runtime>);

#[precompile_utils::precompile]
impl<Runtime> BtcSocketQueuePrecompile<Runtime>
where
	Runtime: pallet_btc_socket_queue::Config<Signature = EthereumSignature>
		+ pallet_evm::Config
		+ frame_system::Config,
	Runtime::AccountId: Into<H160> + From<H160>,
	Runtime::RuntimeCall: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	<Runtime::RuntimeCall as Dispatchable>::RuntimeOrigin: From<Option<Runtime::AccountId>>,
	Runtime::RuntimeCall: From<BtcSocketQueueCall<Runtime>>,
{
	#[precompile::public("getUnsignedPsbts()")]
	#[precompile::public("get_unsigned_psbts()")]
	#[precompile::view]
	fn get_unsigned_psbts(handle: &mut impl PrecompileHandle) -> EvmResult<Vec<UnboundedBytes>> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let mut psbts = vec![];
		pallet_btc_socket_queue::PendingRequests::<Runtime>::iter().for_each(|(_, request)| {
			psbts.push(UnboundedBytes::from(request.unsigned_psbt));
		});

		Ok(psbts)
	}
}