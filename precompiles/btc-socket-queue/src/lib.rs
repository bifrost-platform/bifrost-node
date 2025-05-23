#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::dispatch::{GetDispatchInfo, PostDispatchInfo};

use pallet_btc_socket_queue::Call as BtcSocketQueueCall;
use pallet_evm::AddressMapping;

use precompile_utils::prelude::*;

use fp_account::EthereumSignature;
use sp_core::{H160, H256, U256};
use sp_runtime::traits::Dispatchable;
use sp_std::{marker::PhantomData, vec, vec::Vec};

mod types;
use types::{BitcoinAddressString, EvmRollbackRequestOf, RollbackRequest};

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
	#[precompile::public("isSignedPsbtSubmitted(bytes32,bytes,address)")]
	#[precompile::public("is_signed_psbt_submitted(bytes32,bytes,address)")]
	#[precompile::view]
	fn is_signed_psbt_submitted(
		handle: &mut impl PrecompileHandle,
		txid: H256,
		signed_psbt: UnboundedBytes,
		authority_id: Address,
	) -> EvmResult<bool> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let authority_id = Runtime::AddressMapping::into_account_id(authority_id.0);

		if let Some(pending_request) =
			pallet_btc_socket_queue::PendingRequests::<Runtime>::get(txid)
		{
			if let Some(submitted) = pending_request.signed_psbts.get(&authority_id) {
				Ok(UnboundedBytes::from(submitted.clone()).as_bytes() == signed_psbt.as_bytes())
			} else {
				Ok(false)
			}
		} else {
			Ok(false)
		}
	}

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

	#[precompile::public("rollbackRequest(bytes32)")]
	#[precompile::public("rollback_request(bytes32)")]
	#[precompile::view]
	fn rollback_request(
		handle: &mut impl PrecompileHandle,
		txid: H256,
	) -> EvmResult<EvmRollbackRequestOf> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		let mut result = RollbackRequest::default();

		if let Some(request) = pallet_btc_socket_queue::RollbackRequests::<Runtime>::get(txid) {
			result.unsigned_psbt = request.unsigned_psbt.into();
			result.who = Address(request.who.into());
			result.txid = request.txid;
			result.vout = request.vout.into();
			result.to = BitcoinAddressString::from(request.to.into_inner());
			result.amount = request.amount;

			for (authority_id, vote) in request.votes.iter() {
				result.voted_authorities.push(Address(authority_id.clone().into()));
				result.votes.push(*vote);
			}

			result.is_approved = request.is_approved;
		}
		Ok(result.into())
	}

	#[precompile::public("outboundTx(bytes32)")]
	#[precompile::public("outbound_tx(bytes32)")]
	#[precompile::view]
	fn outbound_tx(
		handle: &mut impl PrecompileHandle,
		txid: H256,
	) -> EvmResult<Vec<UnboundedBytes>> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		Ok(match pallet_btc_socket_queue::BondedOutboundTx::<Runtime>::get(txid) {
			Some(socket_messages) => {
				socket_messages.into_iter().map(|m| UnboundedBytes::from(m)).collect()
			},
			None => vec![],
		})
	}

	#[precompile::public("sequenceToTxId(uint256)")]
	#[precompile::public("sequence_to_tx_id(uint256)")]
	#[precompile::view]
	fn sequence_to_tx_id(handle: &mut impl PrecompileHandle, sequence: U256) -> EvmResult<H256> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		Ok(match pallet_btc_socket_queue::SocketMessages::<Runtime>::get(sequence) {
			Some((txid, _)) => txid,
			None => H256::zero(),
		})
	}

	#[precompile::public("rollbackOutput(bytes32,uint256)")]
	#[precompile::public("rollback_output(bytes32,uint256)")]
	#[precompile::view]
	fn rollback_output(
		handle: &mut impl PrecompileHandle,
		txid: H256,
		vout: U256,
	) -> EvmResult<H256> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;

		Ok(match pallet_btc_socket_queue::BondedRollbackOutputs::<Runtime>::get(txid, vout) {
			Some(psbt_txid) => psbt_txid,
			None => H256::zero(),
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
			.filter(|seq| pallet_btc_socket_queue::SocketMessages::<Runtime>::get(seq).is_none())
			.collect())
	}
}
