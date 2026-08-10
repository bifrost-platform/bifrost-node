#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]

use frame_support::dispatch::{GetDispatchInfo, PostDispatchInfo};
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_evm::AddressMapping;
use pallet_tranche_system::VaultId;
use pallet_tranche_tx_registry::{
	Call as TxRegistryCall, OrderType, ReceiveKind, RequestOpening, RequestStep,
	SettlementChainEntry, SettlementStep, TxRecord, MAX_SPOKE_CHAINS,
};
use precompile_utils::prelude::*;
use sp_core::{ConstU32, H160, H256, U256};
use sp_runtime::{traits::Dispatchable, BoundedVec};
use sp_std::{marker::PhantomData, vec::Vec};

// ---------------------------------------------------------------------------
// Event log selectors
// ---------------------------------------------------------------------------

pub(crate) const SELECTOR_LOG_REQUEST_TX_RECORDED: [u8; 32] = keccak256!(
	"RequestTxRecorded(uint256,bytes32,address,uint64,address,uint256,uint8,uint8,(uint64,bytes32))"
);
pub(crate) const SELECTOR_LOG_SETTLEMENT_TX_RECORDED: [u8; 32] =
	keccak256!("SettlementTxRecorded(uint256,uint256,uint64,uint8,uint64[],(uint64,bytes32))");
pub(crate) const SELECTOR_LOG_RECEIVE_TX_RECORDED: [u8; 32] =
	keccak256!("ReceiveTxRecorded(uint256,address,(uint64,address),uint8,(uint64,bytes32))");

// ---------------------------------------------------------------------------
// interface.sol struct <-> tuple mappings
// ---------------------------------------------------------------------------

/// `VaultInput` — (chain_id, vault_address)
type EvmVaultInput = (u64, Address);
/// `TxAttestation` — (chain_id, tx_hash)
type EvmTxAttestation = (u64, H256);
/// `TxRecord` — (chain_id, tx_hash, recorded_at)
type EvmTxRecord = (u64, H256, U256);
/// `SettlementChainRecord` — (spoke_chain_id, collect_bridge_tx, collect_hooks_tx,
/// response_bridge_tx, response_hooks_tx, finalize_bridge_tx, finalize_hooks_tx)
type EvmSettlementChainRecord =
	(u64, EvmTxRecord, EvmTxRecord, EvmTxRecord, EvmTxRecord, EvmTxRecord, EvmTxRecord);
/// `SettlementChainStatus` — (spoke_chain_id, current_step)
type EvmSettlementChainStatus = (u64, u8);
/// `InvestorRequest` — (product_id, request_id)
type EvmInvestorRequest = (U256, H256);

// ---------------------------------------------------------------------------
// Precompile
// ---------------------------------------------------------------------------

/// A precompile that wraps `pallet_tranche_tx_registry`'s `record_*` extrinsics and
/// exposes read-only visibility into its registry storage — plus, for
/// `get_request_status` only, a read into `pallet-tranche-investments`'
/// `ApprovedInvestments` to resolve a request's linked `settlement_id`/`receivable`
/// (see that function's doc comment for why this precompile, unlike the pallet it
/// wraps, is allowed to read across both pallets directly).
///
/// Called exclusively by the pallet-registered tx recorder account — not a Gateway,
/// not a product's Valuation contract. Every `record_*` function dispatches with a
/// plain `RawOrigin::Signed(caller_account)`, where `caller_account` is
/// `handle.context().caller` mapped through `AddressMapping`; `pallet_tranche_tx_registry`'s
/// own `RecorderOrigin` (`EnsureTxRecorder`) is what actually rejects the call if
/// `caller_account` isn't the registered recorder — this precompile does no
/// pre-check of its own, unlike e.g. `TrancheInvestmentsPrecompile`'s
/// `ensure_caller_is_valuation` (there is no cheaper local check to do here; the
/// recorder account is only known to the pallet's own storage).
pub struct TrancheTxRegistryPrecompile<Runtime>(PhantomData<Runtime>);

#[precompile_utils::precompile]
impl<Runtime> TrancheTxRegistryPrecompile<Runtime>
where
	Runtime: pallet_tranche_tx_registry::Config
		+ pallet_tranche_investments::Config
		+ pallet_tranche_system::Config
		+ pallet_evm::Config
		+ frame_system::Config,
	Runtime::RuntimeCall: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	Runtime::RuntimeCall: From<TxRegistryCall<Runtime>>,
	BlockNumberFor<Runtime>: Into<U256>,
	<Runtime as pallet_evm::Config>::AddressMapping: AddressMapping<Runtime::AccountId>,
{
	/// Attest to one tx in a request's 3-tx pipeline. See
	/// `pallet_tranche_tx_registry::record_request_tx`'s doc comment for the full
	/// ordering/duplicate-recording contract this dispatches into; this function's own
	/// job is only translating interface.sol's flat, sentinel-gated calldata into the
	/// pallet's `Option<RequestOpening>` shape.
	///
	/// @param investor       Investor address — required iff step == Requested, else address(0)
	/// @param vault_chain_id EVM chain ID of the tranche vault — required iff step == Requested
	/// @param vault_address  ERC-7540 vault contract address — required iff step == Requested
	/// @param amount         Investor's full requested amount — required iff step == Requested
	/// @param order_type     0 = redeem, 1 = deposit — meaningful iff step == Requested
	/// @param step           0 = Requested, 1 = BridgeExecuted, 2 = HooksExecuted
	#[precompile::public(
		"record_request_tx(uint256,bytes32,address,uint64,address,uint256,uint8,uint8,(uint64,bytes32))"
	)]
	fn record_request_tx(
		handle: &mut impl PrecompileHandle,
		product_id: U256,
		request_id: H256,
		investor: Address,
		vault_chain_id: u64,
		vault_address: Address,
		amount: U256,
		order_type: u8,
		step: u8,
		attestation: EvmTxAttestation,
	) -> EvmResult {
		let decoded_step = decode_request_step(step)?;
		let opening = decode_request_opening(
			decoded_step,
			investor,
			vault_chain_id,
			vault_address,
			amount,
			order_type,
		)?;
		let (chain_id, tx_hash) = attestation;

		let caller_account = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = TxRegistryCall::<Runtime>::record_request_tx {
			product_id: to_product_id(product_id)?,
			request_id,
			opening,
			step: decoded_step,
			chain_id,
			tx_hash,
		};
		RuntimeHelper::<Runtime>::try_dispatch(
			handle,
			frame_system::RawOrigin::Signed(caller_account).into(),
			call,
			0,
		)?;

		let event = log4(
			handle.context().address,
			SELECTOR_LOG_REQUEST_TX_RECORDED,
			topic_u256(product_id),
			request_id,
			topic_h160(investor.0),
			solidity::encode_event_data((
				vault_chain_id,
				vault_address,
				amount,
				order_type,
				step,
				attestation,
			)),
		);
		handle.record_log_costs(&[&event])?;
		event.record(handle)?;

		Ok(())
	}

	/// Attest to one tx in a settlement's pipeline: either the single Trigger tx, or one
	/// bridge/hooks half of a Collect/Response/Finalize leg for one chain. See
	/// `pallet_tranche_tx_registry::record_settlement_tx`'s doc comment for the full
	/// ordering/duplicate-recording contract this dispatches into; this function's own
	/// job is only translating interface.sol's flat, sentinel-gated calldata into the
	/// pallet's `Option<ChainId>`/`Option<BoundedVec<..>>` shapes.
	///
	/// @param spoke_chain_id  The spoke chain this leg step is for — 0 if step == Triggered
	/// @param spoke_chain_ids Full spoke chain ID set — non-empty if step == Triggered
	/// @param step            0 = Queued (never valid here), 1 = Triggered, 2-7 = leg steps,
	/// 8 = Settled (never valid here)
	#[precompile::public(
		"record_settlement_tx(uint256,uint256,uint64,uint64[],uint8,(uint64,bytes32))"
	)]
	fn record_settlement_tx(
		handle: &mut impl PrecompileHandle,
		product_id: U256,
		settlement_id: U256,
		spoke_chain_id: u64,
		spoke_chain_ids: Vec<u64>,
		step: u8,
		attestation: EvmTxAttestation,
	) -> EvmResult {
		let decoded_step = decode_settlement_step(step)?;
		let (decoded_spoke_chain_id, decoded_spoke_chain_ids) =
			decode_settlement_spoke_chains(decoded_step, spoke_chain_id, &spoke_chain_ids)?;
		let (chain_id, tx_hash) = attestation;

		let caller_account = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = TxRegistryCall::<Runtime>::record_settlement_tx {
			product_id: to_product_id(product_id)?,
			settlement_id,
			spoke_chain_id: decoded_spoke_chain_id,
			spoke_chain_ids: decoded_spoke_chain_ids,
			step: decoded_step,
			chain_id,
			tx_hash,
		};
		RuntimeHelper::<Runtime>::try_dispatch(
			handle,
			frame_system::RawOrigin::Signed(caller_account).into(),
			call,
			0,
		)?;

		let event = log4(
			handle.context().address,
			SELECTOR_LOG_SETTLEMENT_TX_RECORDED,
			topic_u256(product_id),
			topic_u256(settlement_id),
			topic_u256(U256::from(spoke_chain_id)),
			solidity::encode_event_data((step, spoke_chain_ids, attestation)),
		);
		handle.record_log_costs(&[&event])?;
		event.record(handle)?;

		Ok(())
	}

	/// Attest to an investor's claim() tx on a vault. See
	/// `pallet_tranche_tx_registry::record_receive_tx`'s doc comment for the full
	/// contract this dispatches into.
	///
	/// @param kind 0 = Redeem, 1 = Deposit
	#[precompile::public(
		"record_receive_tx(uint256,(uint64,address),address,uint8,(uint64,bytes32))"
	)]
	fn record_receive_tx(
		handle: &mut impl PrecompileHandle,
		product_id: U256,
		vault: EvmVaultInput,
		investor: Address,
		kind: u8,
		attestation: EvmTxAttestation,
	) -> EvmResult {
		let (vault_chain_id, vault_address) = vault;
		let vault_id = VaultId { chain_id: vault_chain_id, vault_address: vault_address.0 };
		let decoded_kind = decode_receive_kind(kind)?;
		let (chain_id, tx_hash) = attestation;

		let caller_account = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = TxRegistryCall::<Runtime>::record_receive_tx {
			product_id: to_product_id(product_id)?,
			vault: vault_id,
			investor: investor.0,
			kind: decoded_kind,
			chain_id,
			tx_hash,
		};
		RuntimeHelper::<Runtime>::try_dispatch(
			handle,
			frame_system::RawOrigin::Signed(caller_account).into(),
			call,
			0,
		)?;

		let event = log3(
			handle.context().address,
			SELECTOR_LOG_RECEIVE_TX_RECORDED,
			topic_u256(product_id),
			topic_h160(investor.0),
			solidity::encode_event_data((vault, kind, attestation)),
		);
		handle.record_log_costs(&[&event])?;
		event.record(handle)?;

		Ok(())
	}

	/// Read a request's full 3-tx registry entry. Reverts if `record_request_tx` has
	/// never been called with `step == Requested` for this `request_id`.
	///
	/// @param product_id The product the request belongs to
	/// @param request_id The request to look up
	/// @return investor    Investor address the registry entry was opened with
	/// @return request_tx  Evidence for step 1 (Requested)
	/// @return bridge_tx   Evidence for step 2 (BridgeExecuted)
	/// @return hooks_tx    Evidence for step 3 (HooksExecuted)
	#[precompile::public("get_request_record(uint256,bytes32)")]
	#[precompile::view]
	fn get_request_record(
		handle: &mut impl PrecompileHandle,
		product_id: U256,
		request_id: H256,
	) -> EvmResult<(Address, EvmTxRecord, EvmTxRecord, EvmTxRecord)> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let product_id = to_product_id(product_id)?;
		let entry =
			pallet_tranche_tx_registry::RequestEntries::<Runtime>::get(product_id, request_id)
				.ok_or_else(|| revert("request not found"))?;
		Ok((
			Address(entry.investor),
			encode_tx_record(entry.request_tx),
			encode_tx_record(entry.bridge_tx),
			encode_tx_record(entry.hooks_tx),
		))
	}

	/// Read a settlement's Trigger evidence plus every registered spoke chain's full
	/// leg-by-leg registry entry. Reverts if `record_settlement_tx` has never been
	/// called with `step == Triggered` for this (product_id, settlement_id).
	///
	/// @param product_id    The product the settlement belongs to
	/// @param settlement_id The settlement to look up
	/// @return trigger_tx Evidence for the Trigger step
	/// @return chains     Per-spoke-chain leg registry entries
	#[precompile::public("get_settlement_record(uint256,uint256)")]
	#[precompile::view]
	fn get_settlement_record(
		handle: &mut impl PrecompileHandle,
		product_id: U256,
		settlement_id: U256,
	) -> EvmResult<(EvmTxRecord, Vec<EvmSettlementChainRecord>)> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let product_id = to_product_id(product_id)?;
		let trigger_tx = pallet_tranche_tx_registry::SettlementTriggers::<Runtime>::get(
			product_id,
			settlement_id,
		)
		.ok_or_else(|| revert("settlement not triggered"))?;
		let spoke_chain_ids = pallet_tranche_tx_registry::SettlementSpokeChains::<Runtime>::get(
			product_id,
			settlement_id,
		)
		.ok_or_else(|| revert("settlement not triggered"))?;

		let mut chains = Vec::with_capacity(spoke_chain_ids.len());
		for spoke_chain_id in spoke_chain_ids.iter() {
			let entry = pallet_tranche_tx_registry::SettlementChainEntries::<Runtime>::get((
				product_id,
				settlement_id,
				*spoke_chain_id,
			));
			chains.push((
				*spoke_chain_id,
				encode_tx_record(entry.collect_bridge_tx),
				encode_tx_record(entry.collect_hooks_tx),
				encode_tx_record(entry.response_bridge_tx),
				encode_tx_record(entry.response_hooks_tx),
				encode_tx_record(entry.finalize_bridge_tx),
				encode_tx_record(entry.finalize_hooks_tx),
			));
		}

		Ok((encode_tx_record(Some(trigger_tx)), chains))
	}

	/// Read a settlement's coarse status at two levels: `hub_status`, the overall
	/// hub-level state, and — per registered spoke chain — `spoke_statuses`, that
	/// chain's own current step. Does not revert for an untriggered
	/// (product_id, settlement_id) — returns `hub_status == Queued` (0) and an empty
	/// `spoke_statuses` instead.
	///
	/// @param product_id    The product the settlement belongs to
	/// @param settlement_id The settlement to check
	/// @return hub_status     0 = Queued, 1 = Triggered, 8 = Settled (see interface.sol)
	/// @return spoke_statuses Per-spoke-chain coarse status, ordered as registered at
	/// Trigger time; empty if hub_status == Queued
	#[precompile::public("get_settlement_status(uint256,uint256)")]
	#[precompile::view]
	fn get_settlement_status(
		handle: &mut impl PrecompileHandle,
		product_id: U256,
		settlement_id: U256,
	) -> EvmResult<(u8, Vec<EvmSettlementChainStatus>)> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let product_id = to_product_id(product_id)?;

		let Some(spoke_chain_ids) =
			pallet_tranche_tx_registry::SettlementSpokeChains::<Runtime>::get(
				product_id,
				settlement_id,
			)
		else {
			return Ok((encode_settlement_step(SettlementStep::Queued), Vec::new()));
		};

		let mut spoke_statuses = Vec::with_capacity(spoke_chain_ids.len());
		let mut all_finalized = true;
		for spoke_chain_id in spoke_chain_ids.iter() {
			let entry = pallet_tranche_tx_registry::SettlementChainEntries::<Runtime>::get((
				product_id,
				settlement_id,
				*spoke_chain_id,
			));
			let current_step = settlement_chain_current_step(&entry);
			if current_step != SettlementStep::FinalizeHooksExecuted {
				all_finalized = false;
			}
			spoke_statuses.push((*spoke_chain_id, encode_settlement_step(current_step)));
		}

		let hub_status =
			if all_finalized { SettlementStep::Settled } else { SettlementStep::Triggered };
		Ok((encode_settlement_step(hub_status), spoke_statuses))
	}

	/// Enumerate an investor's currently in-flight requests. An empty array means the
	/// investor has no in-flight request; this is not an error. See
	/// `InvestorActiveRequests`'s doc comment for the current append-only limitation
	/// (a request already Finalized still shows up here until the cross-pallet link
	/// this needs is designed).
	///
	/// @param investor The investor address to look up
	/// @return requests The investor's in-flight (product_id, request_id) pairs
	#[precompile::public("get_investor_active_requests(address)")]
	#[precompile::view]
	fn get_investor_active_requests(
		handle: &mut impl PrecompileHandle,
		investor: Address,
	) -> EvmResult<Vec<EvmInvestorRequest>> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let requests =
			pallet_tranche_tx_registry::InvestorActiveRequests::<Runtime>::get(investor.0);
		Ok(requests
			.into_iter()
			.map(|(product_id, request_id)| (U256::from(product_id), request_id))
			.collect())
	}

	/// Read one request's status. Reverts under the same condition as
	/// `get_request_record` (registry entry never opened).
	///
	/// Unlike every other function here, this one also reads
	/// `pallet-tranche-investments::ApprovedInvestments` directly to resolve
	/// `settlement_id`/`receivable` — `pallet_tranche_tx_registry` the pallet
	/// deliberately has no dependency on `pallet-tranche-investments` (see this
	/// pallet's module docs on why the two precompiles were split apart), but that
	/// decoupling is a pallet-level concern, not a precompile-level one: this
	/// precompile crate is the per-runtime aggregation layer, and
	/// `TrancheInvestmentsPrecompile` itself already sets the precedent of reading
	/// `pallet-tranche-system`'s storage directly despite `pallet-tranche-investments`
	/// not depending on that pallet either.
	///
	/// @param product_id The product the request belongs to
	/// @param request_id The request to look up
	/// @return investor       Investor address the registry entry was opened with
	/// @return vault_chain_id EVM chain ID of the tranche vault this request targets
	/// @return vault_address  ERC-7540 vault contract address this request targets
	/// @return amount         Investor's full requested amount, as submitted at Requested step
	/// @return order_type     0 = redeem, 1 = deposit
	/// @return request_step   0 = Requested, 1 = BridgeExecuted, 2 = HooksExecuted
	/// @return settlement_id  The settlement this request is linked to, 0 if not yet linked
	/// @return receivable     Whether the investor can now call claim() for this request
	#[precompile::public("get_request_status(uint256,bytes32)")]
	#[precompile::view]
	#[allow(clippy::type_complexity)]
	fn get_request_status(
		handle: &mut impl PrecompileHandle,
		product_id: U256,
		request_id: H256,
	) -> EvmResult<(Address, u64, Address, U256, u8, u8, U256, bool)> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let product_id = to_product_id(product_id)?;
		let entry =
			pallet_tranche_tx_registry::RequestEntries::<Runtime>::get(product_id, request_id)
				.ok_or_else(|| revert("request not found"))?;

		let investor = Address(entry.investor);
		let vault_chain_id = entry.vault.chain_id;
		let vault_address = Address(entry.vault.vault_address);
		let amount = entry.amount;
		let order_type = encode_request_order_type(entry.order_type);

		let request_step = if entry.hooks_tx.is_some() {
			RequestStep::HooksExecuted
		} else if entry.bridge_tx.is_some() {
			RequestStep::BridgeExecuted
		} else {
			RequestStep::Requested
		};

		let Some(approved) =
			pallet_tranche_investments::ApprovedInvestments::<Runtime>::get(product_id, request_id)
		else {
			return Ok((
				investor,
				vault_chain_id,
				vault_address,
				amount,
				order_type,
				encode_request_step(request_step),
				U256::zero(),
				false,
			));
		};
		let settlement_id = approved.settlement_id;

		let receivable = pallet_tranche_tx_registry::SettlementChainEntries::<Runtime>::get((
			product_id,
			settlement_id,
			entry.vault.chain_id,
		))
		.finalize_hooks_tx
		.is_some();

		Ok((
			investor,
			vault_chain_id,
			vault_address,
			amount,
			order_type,
			encode_request_step(request_step),
			settlement_id,
			receivable,
		))
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

/// `uint256` topic encoding — left-padded big-endian, matching Solidity's ABI
/// encoding of an indexed `uint256`/`uint64` event parameter.
fn topic_u256(value: U256) -> H256 {
	H256::from(value.to_big_endian())
}

/// `address` topic encoding — left-padded with zeros, matching Solidity's ABI
/// encoding of an indexed `address` event parameter.
fn topic_h160(value: H160) -> H256 {
	H256::from(value)
}

/// `Option<TxRecord<_>>` -> interface.sol's `TxRecord`, using `recorded_at == 0` as
/// the "not yet recorded" sentinel (see `pallet_tranche_tx_registry::TxRecord`'s doc
/// comment).
fn encode_tx_record<BlockNumber: Into<U256>>(record: Option<TxRecord<BlockNumber>>) -> EvmTxRecord {
	match record {
		Some(record) => (record.chain_id, record.tx_hash, record.recorded_at.into()),
		None => (0, H256::zero(), U256::zero()),
	}
}

/// The furthest `SettlementStep` leg value with a recorded `TxRecord` for one spoke
/// chain, checked in pipeline order — `Queued` if nothing has been recorded yet. See
/// `get_settlement_status`'s interface.sol dev notes for why this must walk forward
/// (not "most advanced field set") — legs progress independently, so e.g. a Response
/// leg field can be set before a Collect leg field is.
fn settlement_chain_current_step<BlockNumber>(
	entry: &SettlementChainEntry<BlockNumber>,
) -> SettlementStep {
	let mut current = SettlementStep::Queued;
	if entry.collect_bridge_tx.is_some() {
		current = SettlementStep::CollectBridgeExecuted;
	}
	if entry.collect_hooks_tx.is_some() {
		current = SettlementStep::CollectHooksExecuted;
	}
	if entry.response_bridge_tx.is_some() {
		current = SettlementStep::ResponseBridgeExecuted;
	}
	if entry.response_hooks_tx.is_some() {
		current = SettlementStep::ResponseHooksExecuted;
	}
	if entry.finalize_bridge_tx.is_some() {
		current = SettlementStep::FinalizeBridgeExecuted;
	}
	if entry.finalize_hooks_tx.is_some() {
		current = SettlementStep::FinalizeHooksExecuted;
	}
	current
}

fn decode_request_step(step: u8) -> EvmResult<RequestStep> {
	match step {
		0 => Ok(RequestStep::Requested),
		1 => Ok(RequestStep::BridgeExecuted),
		2 => Ok(RequestStep::HooksExecuted),
		_ => Err(revert("invalid step")),
	}
}

fn encode_request_step(step: RequestStep) -> u8 {
	match step {
		RequestStep::Requested => 0,
		RequestStep::BridgeExecuted => 1,
		RequestStep::HooksExecuted => 2,
	}
}

fn decode_settlement_step(step: u8) -> EvmResult<SettlementStep> {
	match step {
		0 => Ok(SettlementStep::Queued),
		1 => Ok(SettlementStep::Triggered),
		2 => Ok(SettlementStep::CollectBridgeExecuted),
		3 => Ok(SettlementStep::CollectHooksExecuted),
		4 => Ok(SettlementStep::ResponseBridgeExecuted),
		5 => Ok(SettlementStep::ResponseHooksExecuted),
		6 => Ok(SettlementStep::FinalizeBridgeExecuted),
		7 => Ok(SettlementStep::FinalizeHooksExecuted),
		8 => Ok(SettlementStep::Settled),
		_ => Err(revert("invalid step")),
	}
}

fn encode_settlement_step(step: SettlementStep) -> u8 {
	match step {
		SettlementStep::Queued => 0,
		SettlementStep::Triggered => 1,
		SettlementStep::CollectBridgeExecuted => 2,
		SettlementStep::CollectHooksExecuted => 3,
		SettlementStep::ResponseBridgeExecuted => 4,
		SettlementStep::ResponseHooksExecuted => 5,
		SettlementStep::FinalizeBridgeExecuted => 6,
		SettlementStep::FinalizeHooksExecuted => 7,
		SettlementStep::Settled => 8,
	}
}

fn decode_receive_kind(kind: u8) -> EvmResult<ReceiveKind> {
	match kind {
		0 => Ok(ReceiveKind::Redeem),
		1 => Ok(ReceiveKind::Deposit),
		_ => Err(revert("invalid kind")),
	}
}

fn decode_request_order_type(order_type: u8) -> EvmResult<OrderType> {
	match order_type {
		0 => Ok(OrderType::Redeem),
		1 => Ok(OrderType::Deposit),
		_ => Err(revert("invalid order_type")),
	}
}

fn encode_request_order_type(order_type: OrderType) -> u8 {
	match order_type {
		OrderType::Redeem => 0,
		OrderType::Deposit => 1,
	}
}

/// Translates `record_request_tx`'s flat, sentinel-gated calldata
/// (investor/vault_chain_id/vault_address/amount/order_type, all required together
/// iff `step == Requested`, all zero/empty otherwise) into the pallet's
/// `Option<RequestOpening>`. Reverts on any partial/inconsistent combination rather
/// than silently ignoring a stray value, matching interface.sol's documented
/// contract.
fn decode_request_opening(
	step: RequestStep,
	investor: Address,
	vault_chain_id: u64,
	vault_address: Address,
	amount: U256,
	order_type: u8,
) -> EvmResult<Option<RequestOpening>> {
	if step == RequestStep::Requested {
		if investor.0.is_zero()
			|| vault_chain_id == 0
			|| vault_address.0.is_zero()
			|| amount.is_zero()
		{
			return Err(revert(
				"investor/vault_chain_id/vault_address/amount required when step == Requested",
			));
		}
		let decoded_order_type = decode_request_order_type(order_type)?;
		Ok(Some(RequestOpening {
			investor: investor.0,
			vault: VaultId { chain_id: vault_chain_id, vault_address: vault_address.0 },
			amount,
			order_type: decoded_order_type,
		}))
	} else {
		if !investor.0.is_zero()
			|| vault_chain_id != 0
			|| !vault_address.0.is_zero()
			|| !amount.is_zero()
			|| order_type != 0
		{
			return Err(revert(
				"investor/vault_chain_id/vault_address/amount/order_type must be zero/empty unless step == Requested",
			));
		}
		Ok(None)
	}
}

/// `record_settlement_tx`'s decoded `spoke_chain_id`/`spoke_chain_ids` pair — exactly
/// one of the two is `Some`, matching `pallet_tranche_tx_registry::record_settlement_tx`'s
/// own parameter shapes.
type DecodedSpokeChains = (
	Option<pallet_tranche_tx_registry::ChainId>,
	Option<BoundedVec<pallet_tranche_tx_registry::ChainId, ConstU32<MAX_SPOKE_CHAINS>>>,
);

/// Translates `record_settlement_tx`'s flat, sentinel-gated calldata
/// (`spoke_chain_id` == 0 and `spoke_chain_ids` non-empty iff `step == Triggered`,
/// the reverse otherwise) into the pallet's `Option<ChainId>`/
/// `Option<BoundedVec<..>>` pair. Reverts on any partial/inconsistent combination,
/// matching interface.sol's documented contract.
fn decode_settlement_spoke_chains(
	step: SettlementStep,
	spoke_chain_id: u64,
	spoke_chain_ids: &[u64],
) -> EvmResult<DecodedSpokeChains> {
	if step == SettlementStep::Triggered {
		if spoke_chain_id != 0 {
			return Err(revert("spoke_chain_id must be 0 when step == Triggered"));
		}
		if spoke_chain_ids.is_empty() {
			return Err(revert("spoke_chain_ids required when step == Triggered"));
		}
		let bounded = BoundedVec::try_from(spoke_chain_ids.to_vec())
			.map_err(|_| revert("too many spoke chains"))?;
		Ok((None, Some(bounded)))
	} else {
		if !spoke_chain_ids.is_empty() {
			return Err(revert("spoke_chain_ids must be empty unless step == Triggered"));
		}
		if spoke_chain_id == 0 {
			return Err(revert("spoke_chain_id required unless step == Triggered"));
		}
		Ok((Some(spoke_chain_id), None))
	}
}
