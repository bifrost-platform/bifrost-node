#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::dispatch::{GetDispatchInfo, PostDispatchInfo};

use pallet_btc_socket_queue::Call as BtcSocketQueueCall;

use precompile_utils::prelude::*;

use fp_account::EthereumSignature;
use sp_core::{H160, H256, U256};
use sp_runtime::traits::Dispatchable;
use sp_std::{marker::PhantomData, vec, vec::Vec};

type BtcSocketQueueOf<Runtime> = pallet_btc_socket_queue::Pallet<Runtime>;

/// A precompile to wrap the functionality from `pallet_btc_socket_queue`.
pub struct BtcSocketQueuePrecompile<Runtime>(PhantomData<Runtime>);

#[precompile]
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
	#[precompile::public("unsignedPsbts()")]
	#[precompile::public("unsigned_psbts()")]
	#[precompile::view]
	fn unsigned_psbts(handle: &mut impl PrecompileHandle) -> EvmResult<Vec<UnboundedBytes>> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let psbts: Vec<UnboundedBytes> =
			pallet_btc_socket_queue::PendingRequests::<Runtime>::iter()
				.map(|(_, request)| UnboundedBytes::from(request.unsigned_psbt))
				.collect();
		Ok(psbts)
	}

	#[precompile::public("finalizedPsbts()")]
	#[precompile::public("finalized_psbts()")]
	#[precompile::view]
	fn finalized_psbts(handle: &mut impl PrecompileHandle) -> EvmResult<Vec<UnboundedBytes>> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let psbts: Vec<UnboundedBytes> =
			pallet_btc_socket_queue::FinalizedRequests::<Runtime>::iter()
				.map(|(_, request)| UnboundedBytes::from(request.finalized_psbt))
				.collect();
		Ok(psbts)
	}

	#[precompile::public("rollbackPsbts()")]
	#[precompile::public("rollback_psbts()")]
	#[precompile::view]
	fn rollback_psbts(handle: &mut impl PrecompileHandle) -> EvmResult<Vec<UnboundedBytes>> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let psbts: Vec<UnboundedBytes> =
			pallet_btc_socket_queue::RollbackRequests::<Runtime>::iter()
				.filter_map(|(_, request)| match request.is_approved {
					true => None,
					false => Some(UnboundedBytes::from(request.unsigned_psbt)),
				})
				.collect();
		Ok(psbts)
	}

	#[precompile::public("outboundTx(bytes32)")]
	#[precompile::public("outbound_tx(bytes32)")]
	#[precompile::view]
	fn outbound_tx(
		handle: &mut impl PrecompileHandle,
		txid: H256,
	) -> EvmResult<Vec<UnboundedBytes>> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		Ok(match BtcSocketQueueOf::<Runtime>::bonded_outbound_tx(txid) {
			Some(socket_messages) => {
				socket_messages.into_iter().map(|m| UnboundedBytes::from(m)).collect()
			},
			None => vec![],
		})
	}

	#[precompile::public("filterExecutableMsgs(uint256[])")]
	#[precompile::public("filter_executable_msgs(uint256[])")]
	#[precompile::view]
	fn filter_executable_msgs(
		handle: &mut impl PrecompileHandle,
		sequences: Vec<U256>,
	) -> EvmResult<Vec<U256>> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		Ok(sequences
			.into_iter()
			.filter(|seq| BtcSocketQueueOf::<Runtime>::socket_messages(seq).is_none())
			.collect())
	}
}
