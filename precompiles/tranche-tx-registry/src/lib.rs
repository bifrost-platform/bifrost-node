#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]

use frame_support::dispatch::{GetDispatchInfo, PostDispatchInfo};
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_evm::AddressMapping;
use pallet_tranche_system::{ProductId, ProductInspect, VaultId};
use pallet_tranche_tx_registry::{
	BridgeAttempts, BridgeStatus, Call as TxRegistryCall, OrderType, ReceiveKind, RequestOpening,
	RequestStep, SettlementStep, TxRecord, WhitelistStep, MAX_SETTLEMENT_REQUESTS,
	MAX_SPOKE_CHAINS,
};
use precompile_utils::prelude::*;
use sp_core::{ConstU32, Get, H160, H256, U256};
use sp_runtime::{traits::Dispatchable, BoundedVec};
use sp_std::{marker::PhantomData, vec, vec::Vec};

// ---------------------------------------------------------------------------
// Event log selectors
// ---------------------------------------------------------------------------

pub(crate) const SELECTOR_LOG_REQUEST_TX_RECORDED: [u8; 32] = keccak256!(
	"RequestTxRecorded(uint64,bytes32,address,uint64,address,uint256,uint8,uint8,uint64[],(uint64,bytes32),uint8)"
);
pub(crate) const SELECTOR_LOG_SETTLEMENT_TX_RECORDED: [u8; 32] = keccak256!(
	"SettlementTxRecorded(uint64,uint256,uint64,uint8,uint64[],uint64[],bytes32[],(uint64,bytes32),uint8)"
);
pub(crate) const SELECTOR_LOG_RECEIVE_TX_RECORDED: [u8; 32] = keccak256!(
	"ReceiveTxRecorded(uint64,address,(uint64,address),address,uint256,uint8,(uint64,bytes32))"
);
pub(crate) const SELECTOR_LOG_WHITELIST_TX_RECORDED: [u8; 32] = keccak256!(
	"WhitelistTxRecorded(address,(uint64,address),bool,uint256,uint8,(uint64,bytes32),uint8)"
);

// ---------------------------------------------------------------------------
// interface.sol struct <-> tuple mappings
// ---------------------------------------------------------------------------

/// `VaultInput` — (chain_id, vault_address)
type EvmVaultInput = (u64, Address);
/// `TxAttestation` — (chain_id, tx_hash)
type EvmTxAttestation = (u64, H256);
/// `TxRecord` — (chain_id, tx_hash, recorded_at)
type EvmTxRecord = (u64, H256, U256);
/// `SettlementTxStep` — (step, tx)
type EvmSettlementTxStep = (u8, EvmTxRecord);
/// `SettlementChainSteps` — (spoke_chain_id, steps)
type EvmSettlementChainSteps = (u64, Vec<EvmSettlementTxStep>);
/// `RequestTxStep` — (step, tx)
type EvmRequestTxStep = (u8, EvmTxRecord);
/// `AdapterLeg` — (chain_id, steps)
type EvmAdapterLeg = (u64, Vec<EvmRequestTxStep>);
/// `RequestInfo` — (investor, vault, amount, order_type)
type EvmRequestInfo = (Address, EvmVaultInput, U256, u8);
/// `InvestorRequest` — (product_id, request_id)
type EvmInvestorRequest = (u64, H256);
/// `ReceiveHistoryEntry` — (vault, tx_hash)
type EvmReceiveHistoryEntry = (EvmVaultInput, H256);
/// `WhitelistTxStep` — (step, tx)
type EvmWhitelistTxStep = (u8, EvmTxRecord);
/// `BridgeAttempt` — (status, tx)
type EvmBridgeAttempt = (u8, EvmTxRecord);
/// `ChainBridgeAttempts` — (chain_id, attempts)
type EvmChainBridgeAttempts = (u64, Vec<EvmBridgeAttempt>);
/// `SettlementChainBridgeAttempts` — (spoke_chain_id, collect_attempts, response_attempts,
/// finalize_attempts)
type EvmSettlementChainBridgeAttempts =
	(u64, Vec<EvmBridgeAttempt>, Vec<EvmBridgeAttempt>, Vec<EvmBridgeAttempt>);

/// Upper bound on `get_investor_request_history`'s `limit` — caps the page size
/// so a single `eth_call` can't be asked to serialize an unbounded response,
/// independent of how large the underlying `InvestorRequestHistory` entry has
/// grown. Rejected (not silently clamped) if exceeded, same "catch caller bugs
/// early" convention as every other sentinel-gated parameter in this file.
const MAX_HISTORY_PAGE_SIZE: usize = 50;

// ---------------------------------------------------------------------------
// Precompile
// ---------------------------------------------------------------------------

/// A precompile that wraps `pallet_tranche_tx_registry`'s `record_*` extrinsics and
/// exposes read-only visibility into its registry storage. `get_request`'s
/// `settlement_id`/`settled` are resolved entirely from this pallet's own
/// `RequestEntries`/`SettlementChainEntries`/`SettlementCollectResponseChains` —
/// no cross-pallet read into `pallet-tranche-investments` is needed here (unlike
/// an earlier version of this precompile, before this pallet started tracking
/// its own copy of the request<->settlement linkage — see
/// `pallet_tranche_tx_registry::SettlementRequests`'s doc comment).
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
		+ pallet_tranche_system::Config
		+ pallet_evm::Config
		+ frame_system::Config,
	Runtime::RuntimeCall: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
	Runtime::RuntimeCall: From<TxRegistryCall<Runtime>>,
	BlockNumberFor<Runtime>: Into<U256>,
	<Runtime as pallet_evm::Config>::AddressMapping: AddressMapping<Runtime::AccountId>,
{
	/// Attest to one tx in a request's pipeline — the single Requested tx, the
	/// single RequestQueued tx, one Bridge half of the Inbound leg (Spoke-vault
	/// requests only), or one Bridge/Applied half of a per-chain Adapter leg. A
	/// request's link to a settlement is recorded separately, via
	/// `record_settlement_tx`'s `RequestsApproved` step (batched across every request
	/// approved into one settlement — see that function's doc comment). See
	/// `pallet_tranche_tx_registry::record_request_tx`'s doc comment for the full
	/// ordering/duplicate-recording contract this dispatches into; this function's
	/// own job is only translating interface.sol's flat, sentinel-gated calldata
	/// into the pallet's `Option<RequestOpening>`/`Option<BoundedVec<..>>` shapes.
	///
	/// @param investor              Investor address — required iff step == Requested
	/// @param vault_chain_id        EVM chain ID of the tranche vault — required iff
	/// step == Requested
	/// @param vault_address         ERC-7540 vault contract address — required iff
	/// step == Requested
	/// @param amount                Investor's full requested amount — required iff
	/// step == Requested
	/// @param order_type            0 = redeem, 1 = deposit — meaningful iff step == Requested
	/// @param adapter_chain_ids Every chain needing its own Adapter leg — meaningful
	/// (and may be empty) iff step == RequestQueued (Hub-vault or Spoke-vault alike),
	/// empty otherwise. Not the only way a chain ends up declared — see
	/// `pallet_tranche_tx_registry::record_request_tx`'s doc comment on self-declaration
	/// via AdapterBridgeExecuted/AdapterApplied
	/// @param step                  0 = None (never valid here), 1 = Requested,
	/// 2 = RequestBridgeExecuted, 3 = RequestQueued,
	/// 4 = AdapterBridgeExecuted, 5 = AdapterApplied,
	/// 6 = RequestCompleted (never valid here)
	/// @param bridge_status         3 = Executed, 4 = Reverted — meaningful iff step ==
	/// RequestBridgeExecuted or AdapterBridgeExecuted, MUST be 0 otherwise
	#[precompile::public(
		"record_request_tx(uint64,bytes32,address,uint64,address,uint256,uint8,uint64[],uint8,(uint64,bytes32),uint8)"
	)]
	fn record_request_tx(
		handle: &mut impl PrecompileHandle,
		product_id: ProductId,
		request_id: H256,
		investor: Address,
		vault_chain_id: u64,
		vault_address: Address,
		amount: U256,
		order_type: u8,
		adapter_chain_ids: Vec<u64>,
		step: u8,
		attestation: EvmTxAttestation,
		bridge_status: u8,
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
		let decoded_adapter_chains =
			decode_request_adapter_chains(decoded_step, &adapter_chain_ids)?;
		let is_bridge_step = matches!(
			decoded_step,
			RequestStep::RequestBridgeExecuted | RequestStep::AdapterBridgeExecuted
		);
		let decoded_bridge_status = decode_gated_bridge_status(is_bridge_step, bridge_status)?;
		let (chain_id, tx_hash) = attestation;

		let caller_account = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = TxRegistryCall::<Runtime>::record_request_tx {
			product_id,
			request_id,
			opening,
			adapter_chain_ids: decoded_adapter_chains,
			step: decoded_step,
			chain_id,
			tx_hash,
			bridge_status: decoded_bridge_status,
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
			topic_u256(U256::from(product_id)),
			request_id,
			topic_h160(investor.0),
			solidity::encode_event_data((
				vault_chain_id,
				vault_address,
				amount,
				order_type,
				step,
				adapter_chain_ids,
				attestation,
				bridge_status,
			)),
		);
		handle.record_log_costs(&[&event])?;
		event.record(handle)?;

		Ok(())
	}

	/// Attest to one tx in a settlement's pipeline: the single Trigger tx, the
	/// (possibly batched) RequestsApproved tx, or one bridge/hooks half of a
	/// Collect/Response/Finalize leg for one chain. A settlement needing no
	/// cross-chain action at all is recorded as `Triggered` with both chain sets
	/// empty. See `pallet_tranche_tx_registry::record_settlement_tx`'s doc comment
	/// for the full ordering/duplicate-recording contract this dispatches into;
	/// this function's own job is only translating interface.sol's flat,
	/// sentinel-gated calldata into the pallet's
	/// `Option<ChainId>`/`Option<BoundedVec<..>>` shapes.
	///
	/// @param spoke_chain_id  The spoke chain this leg step is for — 0 if step ==
	/// Triggered or RequestsApproved (both settlement-wide, not chain-scoped)
	/// @param collect_response_chain_ids Chains needing a Collect/Response leg (have a
	/// registered Adapter) — meaningful (and may be empty) iff step == Triggered
	/// @param finalize_chain_ids Chains needing a Finalize leg (have a registered vault) —
	/// meaningful (and may be empty) iff step == Triggered
	/// @param request_ids Every request_id Valuation approved into this settlement —
	/// required (non-empty) iff step == RequestsApproved, empty otherwise
	/// @param step            0 = Queued (never valid here), 1 = Triggered,
	/// 2 = CollectBridgeExecuted, 3 = NavReported, 4 = ResponseBridgeExecuted,
	/// 5 = NavReceived, 6 = RequestsApproved, 7 = FinalizeBridgeExecuted, 8 = SettleApplied,
	/// 9 = Settled (never valid here)
	/// @param bridge_status   3 = Executed, 4 = Reverted — meaningful iff step is one of
	/// the three Bridge-phase leg steps, MUST be 0 otherwise
	#[precompile::public(
		"record_settlement_tx(uint64,uint256,uint64,uint64[],uint64[],bytes32[],uint8,(uint64,bytes32),uint8)"
	)]
	fn record_settlement_tx(
		handle: &mut impl PrecompileHandle,
		product_id: ProductId,
		settlement_id: U256,
		spoke_chain_id: u64,
		collect_response_chain_ids: Vec<u64>,
		finalize_chain_ids: Vec<u64>,
		request_ids: Vec<H256>,
		step: u8,
		attestation: EvmTxAttestation,
		bridge_status: u8,
	) -> EvmResult {
		let decoded_step = decode_settlement_step(step)?;
		let (
			decoded_spoke_chain_id,
			decoded_collect_response_chain_ids,
			decoded_finalize_chain_ids,
		) = decode_settlement_spoke_chains(
			decoded_step,
			spoke_chain_id,
			&collect_response_chain_ids,
			&finalize_chain_ids,
		)?;
		let decoded_request_ids = decode_settlement_request_ids(decoded_step, &request_ids)?;
		let is_bridge_step = matches!(
			decoded_step,
			SettlementStep::CollectBridgeExecuted
				| SettlementStep::ResponseBridgeExecuted
				| SettlementStep::FinalizeBridgeExecuted
		);
		let decoded_bridge_status = decode_gated_bridge_status(is_bridge_step, bridge_status)?;
		let (chain_id, tx_hash) = attestation;

		let caller_account = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = TxRegistryCall::<Runtime>::record_settlement_tx {
			product_id,
			settlement_id,
			spoke_chain_id: decoded_spoke_chain_id,
			collect_response_chain_ids: decoded_collect_response_chain_ids,
			finalize_chain_ids: decoded_finalize_chain_ids,
			request_ids: decoded_request_ids,
			step: decoded_step,
			chain_id,
			tx_hash,
			bridge_status: decoded_bridge_status,
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
			topic_u256(U256::from(product_id)),
			topic_u256(settlement_id),
			topic_u256(U256::from(spoke_chain_id)),
			solidity::encode_event_data((
				step,
				collect_response_chain_ids,
				finalize_chain_ids,
				request_ids,
				attestation,
				bridge_status,
			)),
		);
		handle.record_log_costs(&[&event])?;
		event.record(handle)?;

		Ok(())
	}

	/// Attest to an investor's receive() tx on a vault. See
	/// `pallet_tranche_tx_registry::record_receive_tx`'s doc comment for the full
	/// contract this dispatches into.
	///
	/// @param investor Controller whose depositRequest/redeemRequest this settles
	/// @param receiver Who actually received the funds — may differ from investor
	/// @param amount   Shares received (kind == Deposit) or assets received (kind == Redeem)
	/// @param kind     0 = Redeem, 1 = Deposit
	#[precompile::public(
		"record_receive_tx(uint64,(uint64,address),address,address,uint256,uint8,(uint64,bytes32))"
	)]
	fn record_receive_tx(
		handle: &mut impl PrecompileHandle,
		product_id: ProductId,
		vault: EvmVaultInput,
		investor: Address,
		receiver: Address,
		amount: U256,
		kind: u8,
		attestation: EvmTxAttestation,
	) -> EvmResult {
		let (vault_chain_id, vault_address) = vault;
		let vault_id = VaultId { chain_id: vault_chain_id, vault_address: vault_address.0 };
		let decoded_kind = decode_receive_kind(kind)?;
		let (chain_id, tx_hash) = attestation;

		let caller_account = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = TxRegistryCall::<Runtime>::record_receive_tx {
			product_id,
			vault: vault_id,
			investor: investor.0,
			receiver: receiver.0,
			amount,
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
			topic_u256(U256::from(product_id)),
			topic_h160(investor.0),
			solidity::encode_event_data((vault, receiver, amount, kind, attestation)),
		);
		handle.record_log_costs(&[&event])?;
		event.record(handle)?;

		Ok(())
	}

	/// Attest to one tx in a whitelist grant/revoke action's pipeline. See
	/// `pallet_tranche_tx_registry::record_whitelist_tx`'s doc comment for the full
	/// contract this dispatches into — notably, unlike every other record_*_tx
	/// function here, this one takes no `product_id` input at all; the pallet
	/// resolves it internally from `vault`.
	///
	/// @param vault  The tranche vault this whitelist action targets
	/// @param who    The account whose whitelist status is being changed
	/// @param grant  true = grant, false = revoke — fixed for this action, resupplied at
	/// every step
	/// @param nonce  Correlator for this action — Orchestrator-generated for a Multichain
	/// product, TrancheManager-generated for a SingleChain product (no Orchestrator there)
	/// @param step   0 = None (invalid), 1 = WhitelistRequested, 2 = BridgeExecuted,
	/// 3 = WhitelistApplied
	/// @param bridge_status 3 = Executed, 4 = Reverted — meaningful iff step ==
	/// BridgeExecuted, MUST be 0 otherwise
	#[precompile::public(
		"record_whitelist_tx((uint64,address),address,bool,uint256,uint8,(uint64,bytes32),uint8)"
	)]
	fn record_whitelist_tx(
		handle: &mut impl PrecompileHandle,
		vault: EvmVaultInput,
		who: Address,
		grant: bool,
		nonce: U256,
		step: u8,
		attestation: EvmTxAttestation,
		bridge_status: u8,
	) -> EvmResult {
		let (vault_chain_id, vault_address) = vault;
		let vault_id = VaultId { chain_id: vault_chain_id, vault_address: vault_address.0 };
		let decoded_step = decode_whitelist_step(step)?;
		let is_bridge_step = decoded_step == WhitelistStep::BridgeExecuted;
		let decoded_bridge_status = decode_gated_bridge_status(is_bridge_step, bridge_status)?;
		let (chain_id, tx_hash) = attestation;

		let caller_account = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = TxRegistryCall::<Runtime>::record_whitelist_tx {
			vault: vault_id,
			who: who.0,
			grant,
			nonce,
			step: decoded_step,
			chain_id,
			tx_hash,
			bridge_status: decoded_bridge_status,
		};
		RuntimeHelper::<Runtime>::try_dispatch(
			handle,
			frame_system::RawOrigin::Signed(caller_account).into(),
			call,
			0,
		)?;

		let event = log2(
			handle.context().address,
			SELECTOR_LOG_WHITELIST_TX_RECORDED,
			topic_h160(who.0),
			solidity::encode_event_data((vault, grant, nonce, step, attestation, bridge_status)),
		);
		handle.record_log_costs(&[&event])?;
		event.record(handle)?;

		Ok(())
	}

	/// Read a settlement's full state in one call: Trigger evidence, the settlement's
	/// own overall status, and every registered chain's ordered step-by-step history
	/// (each entry only for the leg kind(s) that chain's role actually needs — no
	/// zeroed-forever "not applicable" fields to interpret). Does not revert for an
	/// untriggered (product_id, settlement_id) — returns a zeroed `trigger_tx`,
	/// `status == Queued`, and empty `spoke_chains` instead.
	///
	/// `spoke_chains[i].steps` is exactly `[CollectBridgeExecuted, NavReported,
	/// ResponseBridgeExecuted, NavReceived]` for a chain with only a
	/// registered Adapter, `[FinalizeBridgeExecuted, SettleApplied]` for a
	/// chain with only a registered vault, or all six (Collect/Response then Finalize)
	/// for a chain with both — a step never present in this array means it doesn't
	/// apply to this chain at all, not merely "not yet reached." Within the array,
	/// each entry's `tx` is zeroed (`recorded_at == 0`) iff that step genuinely hasn't
	/// landed yet — check `steps[steps.length - 1].tx.recorded_at != 0` to tell
	/// whether this specific chain is done. `spoke_chains` is ordered
	/// collect_response-declared chains first, then any finalize-only chains not
	/// already included.
	///
	/// `status` only ever takes one of three values: `Queued` (Trigger not yet
	/// recorded — `trigger_tx` is then zeroed too), `Triggered` (at least one chain's
	/// last step hasn't landed), `Settled` (every chain's last step has — vacuously
	/// true, and immediate, if Triggered with both chain sets empty, i.e.
	/// `spoke_chains` itself is empty). `trigger_tx` itself never changes once
	/// Triggered — only `status` moves from `Triggered` to `Settled` as chains
	/// complete.
	/// @param product_id    The product the settlement belongs to
	/// @param settlement_id The settlement to look up
	/// @return trigger_tx   Evidence for the Trigger step
	/// @return status       The settlement's own overall status — `Queued`/`Triggered`/`Settled`
	/// @return spoke_chains Per-chain ordered step history, see above
	/// @return spoke_bridge_attempts Per-chain full Bridge-phase attempt history across all
	/// three leg kinds — every attempt observed, Executed or Reverted alike, in order; a
	/// chain without a given leg kind has an empty array for it
	#[precompile::public("get_settlement(uint64,uint256)")]
	#[precompile::view]
	#[allow(clippy::type_complexity)]
	fn get_settlement(
		handle: &mut impl PrecompileHandle,
		product_id: ProductId,
		settlement_id: U256,
	) -> EvmResult<(
		EvmTxRecord,
		u8,
		Vec<EvmSettlementChainSteps>,
		Vec<EvmSettlementChainBridgeAttempts>,
	)> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let Some(trigger_tx) = pallet_tranche_tx_registry::SettlementTriggers::<Runtime>::get(
			product_id,
			settlement_id,
		) else {
			return Ok((
				encode_tx_record::<BlockNumberFor<Runtime>>(None),
				encode_settlement_step(SettlementStep::Queued),
				Vec::new(),
				Vec::new(),
			));
		};
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let collect_response_chain_ids =
			pallet_tranche_tx_registry::SettlementCollectResponseChains::<Runtime>::get(
				product_id,
				settlement_id,
			)
			.unwrap_or_default();
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let finalize_chain_ids =
			pallet_tranche_tx_registry::SettlementFinalizeChains::<Runtime>::get(
				product_id,
				settlement_id,
			)
			.unwrap_or_default();
		let chain_ids = union_chain_ids(&collect_response_chain_ids, &finalize_chain_ids);

		let mut spoke_chains = Vec::with_capacity(chain_ids.len());
		let mut spoke_bridge_attempts = Vec::with_capacity(chain_ids.len());
		let mut all_complete = true;
		for chain_id in chain_ids.iter() {
			handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
			let entry = pallet_tranche_tx_registry::SettlementChainEntries::<Runtime>::get((
				product_id,
				settlement_id,
				*chain_id,
			));

			let needs_collect_response = collect_response_chain_ids.contains(chain_id);
			let needs_finalize = finalize_chain_ids.contains(chain_id);

			// A chain in `finalize_chain_ids` isn't complete until its Finalize leg
			// lands; a Collect/Response-only chain (no vault, never gets a Finalize
			// leg — see `SettlementStep`'s doc comment) is complete once Response does.
			let complete = if needs_finalize {
				entry.settle_applied_tx.is_some()
			} else {
				entry.nav_received_tx.is_some()
			};
			if !complete {
				all_complete = false;
			}

			let collect_tx = select_executed(&entry.collect_bridge_attempts);
			let response_tx = select_executed(&entry.response_bridge_attempts);
			let finalize_tx = select_executed(&entry.finalize_bridge_attempts);

			let mut steps = Vec::with_capacity(
				if needs_collect_response { 4 } else { 0 } + if needs_finalize { 2 } else { 0 },
			);
			if needs_collect_response {
				steps.push((
					encode_settlement_step(SettlementStep::CollectBridgeExecuted),
					encode_tx_record(collect_tx),
				));
				steps.push((
					encode_settlement_step(SettlementStep::NavReported),
					encode_tx_record(entry.nav_reported_tx),
				));
				steps.push((
					encode_settlement_step(SettlementStep::ResponseBridgeExecuted),
					encode_tx_record(response_tx),
				));
				steps.push((
					encode_settlement_step(SettlementStep::NavReceived),
					encode_tx_record(entry.nav_received_tx),
				));
			}
			if needs_finalize {
				steps.push((
					encode_settlement_step(SettlementStep::FinalizeBridgeExecuted),
					encode_tx_record(finalize_tx),
				));
				steps.push((
					encode_settlement_step(SettlementStep::SettleApplied),
					encode_tx_record(entry.settle_applied_tx),
				));
			}
			spoke_chains.push((*chain_id, steps));
			spoke_bridge_attempts.push((
				*chain_id,
				encode_bridge_attempts(entry.collect_bridge_attempts),
				encode_bridge_attempts(entry.response_bridge_attempts),
				encode_bridge_attempts(entry.finalize_bridge_attempts),
			));
		}

		let status = if all_complete { SettlementStep::Settled } else { SettlementStep::Triggered };
		Ok((
			encode_tx_record(Some(trigger_tx)),
			encode_settlement_step(status),
			spoke_chains,
			spoke_bridge_attempts,
		))
	}

	/// Enumerate an investor's currently in-flight requests. An empty array means the
	/// investor has no in-flight request; this is not an error. A request is removed
	/// from here automatically once its settlement's `SettleApplied` leg lands (or, for
	/// a Hub-vault request, once its settlement's Collect/Response completes) — see
	/// `InvestorActiveRequests`'s own doc comment for the exact mechanism. For requests
	/// that have already dropped out of this list, see `get_investor_request_history`.
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
		Ok(requests.into_iter().collect())
	}

	/// Page through an investor's full request history for one product — every
	/// `request_id` ever opened (`RequestStep::Requested`), including ones long since
	/// completed and no longer in `get_investor_active_requests`. Returned most-recent
	/// first; `offset`/`limit` index into that most-recent-first order (`offset == 0`
	/// is the single most recent request). `total` is the full history length for this
	/// (investor, product_id), so a caller can compute page count without a separate
	/// call; `offset >= total` returns an empty array rather than reverting, so a
	/// caller can page forward until it gets one back.
	///
	/// `limit` MUST NOT exceed `MAX_HISTORY_PAGE_SIZE` (rejected otherwise) — this
	/// bounds the response size regardless of how large the underlying history has
	/// grown, but does NOT bound the underlying storage read cost: `InvestorRequestHistory`
	/// is stored as one `Vec` per (investor, product_id), and Substrate has no way to
	/// read/decode only a slice of a stored `Vec` — the full history is always read and
	/// decoded from storage first, then sliced down to the requested page in memory.
	/// `record_cost` below still only charges for one DB read (this pallet's existing
	/// convention — see e.g. `get_request`'s per-loop-iteration charging elsewhere in
	/// this file for the pattern this deliberately does NOT need here, since this is a
	/// single read of a single storage entry, not one read per loop iteration), so a
	/// very long history is charged the same gas as a short one despite doing more
	/// real work — accepted for now since growth is bounded by real, gas-costed
	/// on-chain requests, not something this precompile can be tricked into inflating
	/// for free. See `InvestorRequestHistory`'s own doc comment for the full rationale.
	///
	/// @param investor    The investor address to look up
	/// @param product_id  The product to page history for
	/// @param offset      How many of the most-recent entries to skip
	/// @param limit       Max entries to return — MUST NOT exceed MAX_HISTORY_PAGE_SIZE
	/// @return request_ids Up to `limit` request_ids, most-recent first
	/// @return total       Total history length for this (investor, product_id)
	#[precompile::public("get_investor_request_history(address,uint64,uint256,uint256)")]
	#[precompile::view]
	fn get_investor_request_history(
		handle: &mut impl PrecompileHandle,
		investor: Address,
		product_id: ProductId,
		offset: U256,
		limit: U256,
	) -> EvmResult<(Vec<H256>, U256)> {
		if limit > U256::from(MAX_HISTORY_PAGE_SIZE) {
			return Err(revert("limit exceeds MAX_HISTORY_PAGE_SIZE"));
		}
		let limit = limit.as_usize();

		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let history = pallet_tranche_tx_registry::InvestorRequestHistory::<Runtime>::get(
			investor.0, product_id,
		);
		let total = U256::from(history.len());
		if offset >= total {
			return Ok((Vec::new(), total));
		}
		// Safe: offset < total, and total was itself built from a real `usize`
		// (`history.len()`) above, so offset necessarily fits in a `usize` too.
		let offset = offset.as_usize();

		let request_ids = history.iter().rev().skip(offset).take(limit).copied().collect();
		Ok((request_ids, total))
	}

	/// Page through an investor's full receive() history for one product — every
	/// `(vault, tx_hash)` ever recorded via `record_receive_tx`. Same
	/// most-recent-first/`offset`/`limit`/`total` contract as
	/// `get_investor_request_history` (see that function's own doc comment for the
	/// full rationale, including why pagination bounds the response size but not
	/// the underlying storage read cost) — this is its receive-side equivalent,
	/// since a receive isn't linked to a specific `request_id` the way a request's
	/// own history is (TrancheManager pools receivable amounts per (investor,
	/// vault), not per request). Each returned `(vault, tx_hash)` pair is exactly
	/// what `ReceiveEntries`' own key needs, so pass it straight through to a
	/// storage-level lookup if the full `ReceiveEntry` is ever exposed via a
	/// dedicated getter.
	///
	/// @param investor    The investor (controller) address to look up
	/// @param product_id  The product to page history for
	/// @param offset      How many of the most-recent entries to skip
	/// @param limit       Max entries to return — MUST NOT exceed MAX_HISTORY_PAGE_SIZE
	/// @return receives Up to `limit` (vault, tx_hash) pairs, most-recent first
	/// @return total    Total history length for this (investor, product_id)
	#[precompile::public("get_investor_receive_history(address,uint64,uint256,uint256)")]
	#[precompile::view]
	fn get_investor_receive_history(
		handle: &mut impl PrecompileHandle,
		investor: Address,
		product_id: ProductId,
		offset: U256,
		limit: U256,
	) -> EvmResult<(Vec<EvmReceiveHistoryEntry>, U256)> {
		if limit > U256::from(MAX_HISTORY_PAGE_SIZE) {
			return Err(revert("limit exceeds MAX_HISTORY_PAGE_SIZE"));
		}
		let limit = limit.as_usize();

		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let history = pallet_tranche_tx_registry::InvestorReceiveHistory::<Runtime>::get(
			investor.0, product_id,
		);
		let total = U256::from(history.len());
		if offset >= total {
			return Ok((Vec::new(), total));
		}
		// Safe: offset < total, and total was itself built from a real `usize`
		// (`history.len()`) above, so offset necessarily fits in a `usize` too.
		let offset = offset.as_usize();

		let receives = history
			.iter()
			.rev()
			.skip(offset)
			.take(limit)
			.map(|(vault, tx_hash)| ((vault.chain_id, Address(vault.vault_address)), *tx_hash))
			.collect();
		Ok((receives, total))
	}

	/// Resolve one `(investor, vault, tx_hash)` entry from `get_investor_receive_history`
	/// (or observed directly off a `ReceiveTxRecorded` event) into its full detail —
	/// exactly the same "history gives you an identifier, this resolves it" relationship
	/// `get_request`/`get_settlement` have with `request_id`/`settlement_id`, except
	/// receives need all three key parts since `ReceiveEntries` has no single-field
	/// lookup the way `RequestEntries`/`SettlementTriggers` do.
	///
	/// Reverts if no such entry exists (`investor`/`vault`/`tx_hash` must exactly match
	/// a prior `record_receive_tx` call) — same convention as `get_request`, not
	/// `get_settlement`'s more lenient zeroed-response-for-not-yet-triggered behavior,
	/// since there's no meaningful "not yet" state for a receive: either the tx_hash was
	/// attested or it wasn't.
	///
	/// @param investor The controller whose request this receive() call settled
	/// @param vault    The vault this receive() call was against
	/// @param tx_hash  The receive() tx's hash on that vault's chain
	/// @return receiver Who actually received the funds — may differ from investor
	/// @return amount   Shares received (kind == Deposit) or assets received (kind == Redeem)
	/// @return kind     Which receivable pool this receive() call drained
	/// @return tx       Evidence for this receive() tx
	#[precompile::public("get_receive(address,(uint64,address),bytes32)")]
	#[precompile::view]
	fn get_receive(
		handle: &mut impl PrecompileHandle,
		investor: Address,
		vault: EvmVaultInput,
		tx_hash: H256,
	) -> EvmResult<(Address, U256, u8, EvmTxRecord)> {
		let (vault_chain_id, vault_address) = vault;
		let vault_id = VaultId { chain_id: vault_chain_id, vault_address: vault_address.0 };

		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let entry = pallet_tranche_tx_registry::ReceiveEntries::<Runtime>::get((
			investor.0, vault_id, tx_hash,
		))
		.ok_or_else(|| revert("receive not found"))?;

		Ok((
			Address(entry.receiver),
			entry.amount,
			encode_receive_kind(entry.kind),
			encode_tx_record(Some(entry.tx)),
		))
	}

	/// Read the most recent whitelist action's `nonce` for a given `(vault, who)`.
	/// Reverts if no `WhitelistRequested` has ever been recorded for this pair.
	/// Pass the returned `nonce` straight into `get_whitelist` for that action's
	/// full state — this pallet keeps no history array (see
	/// `pallet_tranche_tx_registry::LatestWhitelistNonce`'s doc comment for why),
	/// so this is the only on-chain way to discover a `(vault, who)` pair's
	/// current `nonce` without already knowing it from watching
	/// `WhitelistTxRecorded`.
	///
	/// @param vault The tranche vault to look up
	/// @param who   The account whose whitelist status to look up
	/// @return nonce The most recent whitelist action's nonce for this pair
	#[precompile::public("get_latest_whitelist_nonce((uint64,address),address)")]
	#[precompile::view]
	fn get_latest_whitelist_nonce(
		handle: &mut impl PrecompileHandle,
		vault: EvmVaultInput,
		who: Address,
	) -> EvmResult<U256> {
		let (vault_chain_id, vault_address) = vault;
		let vault_id = VaultId { chain_id: vault_chain_id, vault_address: vault_address.0 };

		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		pallet_tranche_tx_registry::LatestWhitelistNonce::<Runtime>::get(who.0, vault_id)
			.ok_or_else(|| revert("no whitelist action recorded for this (vault, who)"))
	}

	/// Read a whitelist grant/revoke action's full state in one call. Reverts if
	/// `record_whitelist_tx` has never opened an entry for this `(vault, who, nonce)`
	/// — via `WhitelistRequested` (Multichain) or self-opened via `WhitelistApplied`
	/// (SingleChain, see that arm's dev notes in the pallet).
	///
	/// For a Multichain product's action: `steps[0]` is always
	/// `(WhitelistRequested, request_tx)` and `steps`' last entry is always
	/// `(WhitelistApplied, applied_tx)` — same Hub-vault/Spoke-vault branching as
	/// `get_request`'s `request_steps`: for a Hub-vault action (`vault.chain_id`
	/// equals this chain's own EVM chain ID), that's the array's only two entries
	/// (length 2, no Bridge leg); for a Spoke-vault one, a `BridgeExecuted` entry
	/// sits between them (length 3).
	/// For a SingleChain product's action: `WhitelistRequested` never appears at
	/// all — there's no Orchestrator-driven Trigger for that model — so `steps` is
	/// just `[WhitelistApplied]` (length 1). Check `steps.length` (1 vs 2 vs 3) to
	/// tell which case this action is, same "absent means not applicable"
	/// convention `get_request`'s `request_steps` uses.
	/// `status` is the last step whose evidence has actually landed.
	///
	/// @param vault The tranche vault this whitelist action targeted
	/// @param who   The account whose whitelist status was being changed
	/// @param nonce Correlator for this action — Orchestrator-generated (Multichain)
	/// or TrancheManager-generated (SingleChain)
	/// @return grant  true = grant, false = revoke
	/// @return steps  Ordered step history — length 1 (SingleChain), 2 (Hub-vault), or
	/// 3 (Spoke-vault)
	/// @return status The furthest step reached so far
	/// @return bridge_attempts The Bridge leg's full attempt history — every attempt
	/// observed, Executed or Reverted alike, in order; empty if no Bridge leg applies
	/// (same cases `steps` itself omits BridgeExecuted for) or simply not yet attempted
	#[precompile::public("get_whitelist((uint64,address),address,uint256)")]
	#[precompile::view]
	fn get_whitelist(
		handle: &mut impl PrecompileHandle,
		vault: EvmVaultInput,
		who: Address,
		nonce: U256,
	) -> EvmResult<(bool, Vec<EvmWhitelistTxStep>, u8, Vec<EvmBridgeAttempt>)> {
		let (vault_chain_id, vault_address) = vault;
		let vault_id = VaultId { chain_id: vault_chain_id, vault_address: vault_address.0 };

		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let entry =
			pallet_tranche_tx_registry::WhitelistEntries::<Runtime>::get((who.0, vault_id, nonce))
				.ok_or_else(|| revert("whitelist action not found"))?;

		// A SingleChain product's action has no Orchestrator-driven `WhitelistRequested`
		// at all — `WhitelistApplied` self-opened this entry directly, so `request_tx`
		// is permanently `None` here, not merely pending. Omit `WhitelistRequested`
		// from `steps` entirely for it, same "absent means not applicable" convention
		// `get_request`'s `request_steps` uses for a colocated request's omitted
		// `RequestQueued`.
		let single_chain_id =
			pallet_tranche_system::Pallet::<Runtime>::single_chain_id(entry.product_id);
		let is_single_chain = single_chain_id.is_some();
		let local_chain_id =
			single_chain_id.unwrap_or_else(<Runtime as pallet_evm::Config>::ChainId::get);
		let has_bridge_leg = entry.vault.chain_id != local_chain_id;
		let bridge_tx = select_executed(&entry.bridge_attempts);
		let status = if entry.applied_tx.is_some() {
			WhitelistStep::WhitelistApplied
		} else if bridge_tx.is_some() {
			WhitelistStep::BridgeExecuted
		} else {
			WhitelistStep::WhitelistRequested
		};
		let mut steps = Vec::new();
		if !is_single_chain {
			steps.push((
				encode_whitelist_step(WhitelistStep::WhitelistRequested),
				encode_tx_record(entry.request_tx),
			));
		}
		if has_bridge_leg {
			steps.push((
				encode_whitelist_step(WhitelistStep::BridgeExecuted),
				encode_tx_record(bridge_tx),
			));
		}
		steps.push((
			encode_whitelist_step(WhitelistStep::WhitelistApplied),
			encode_tx_record(entry.applied_tx),
		));

		Ok((
			entry.grant,
			steps,
			encode_whitelist_step(status),
			encode_bridge_attempts(entry.bridge_attempts),
		))
	}

	/// Read a request's full state in one call: its static details (bundled as one
	/// `RequestInfo`), the Requested/Inbound-leg evidence (bundled as one ordered
	/// `request_steps` array, same "step, tx" shape as every per-chain leg entry —
	/// see below), every declared Adapter chain's ordered step-by-step history, the
	/// request's own overall `status`, and its linked settlement's completion. Reverts
	/// if `record_request_tx` has never been called with `step == Requested` for
	/// this `request_id`.
	///
	/// `request_steps[0]` is always `(Requested, request_tx)` — every request that
	/// exists has one, unconditionally. For a `Multichain` product, `request_steps`'
	/// last entry is always `(RequestQueued, queued_tx)` — every such request
	/// reaches this step, Hub-vault or Spoke-vault alike. For a Hub-vault request
	/// (no Inbound leg applies at all — `info.vault.chain_id` equals this chain's
	/// own EVM chain ID), that's the array's only other entry (length 2); for a
	/// Spoke-vault one, a `RequestBridgeExecuted` entry sits between them (length
	/// 3). For a `SingleChain` product's request, `RequestQueued` never appears at
	/// all — there's no `DepositQueued`/`RedeemQueued`-equivalent event for that
	/// model (see `RequestStep`'s doc comment) — so `request_steps` is just
	/// `[Requested]` (length 1). Same "absent means not applicable, present-but-
	/// zeroed means pending" convention as `get_settlement`'s
	/// `spoke_chains[i].steps` — check `request_steps.length` (1 vs 2 vs 3) to tell
	/// which case this request is, and each present entry's own `tx.recorded_at`
	/// to tell whether it's landed yet.
	/// `adapter_legs[i].steps` is `[AdapterBridgeExecuted, AdapterApplied]` (length
	/// 2) for a genuinely remote chain, but just `[AdapterApplied]` (length 1) for
	/// a chain that's the same as the origin vault's own chain, or this product's
	/// own local chain — same "absent means not applicable" convention as
	/// `request_steps` above, since such a chain never gets a Bridge phase at all
	/// (fulfilled synchronously — see
	/// `pallet_tranche_tx_registry::record_request_tx`'s dev notes), not merely
	/// pending one. `adapter_legs` itself is always empty for a `SingleChain`
	/// product — its Adapters are colocated too, so there's no leg to track at
	/// all. Check `adapter_legs[i].steps.length` (1 vs 2) the same way
	/// `request_steps.length` is checked, rather than assuming a fixed shape.
	/// `RequestAdapterChains`' own order (and so `adapter_legs`' order) is normally
	/// the order declared at `RequestQueued`, but a self-fulfilling chain (recorded
	/// before `RequestQueued` ever ran) appears in whatever order it was first
	/// touched instead.
	///
	/// `status` only ever takes `Requested` (`RequestQueued` not yet reached, or
	/// some Adapter leg still has an unfinished step — never true for a
	/// `SingleChain` product, which has neither) or `RequestCompleted`
	/// (`RequestQueued` reached and every declared Adapter chain's last step
	/// landed, or none were declared at all — immediate for a `Multichain`
	/// product's fully local request, and *always* immediate for a `SingleChain`
	/// product's request, right from `Requested`, since neither `RequestQueued`
	/// nor any Adapter leg ever applies to it). This is deliberately unrelated to
	/// `settlement_id`/`settled` below — see `RequestStep`'s doc comment for why
	/// `RequestCompleted` was renamed from `Completed` to disambiguate the two.
	///
	/// `settlement_id`/`settled` are resolved from `entry.settlement_id`/
	/// `entry.approved_tx`, written by `record_settlement_tx`'s
	/// `SettlementStep::RequestsApproved` arm — `settlement_id` is 0 until that step
	/// is recorded (Valuation's `DepositsApproved`/`RedeemsApproved` event). `settled`
	/// depends on whether this request's own vault is on Hub or Spoke, mirroring the
	/// Inbound-leg asymmetry above: for a Spoke vault, true once the Finalize leg's
	/// Hooks phase lands for this request's own origin chain; for a Hub vault (no
	/// Finalize leg of its own to wait on), true once every one of the linked
	/// settlement's `collect_response_chain_ids` reaches `NavReceived` (vacuously
	/// true, and immediate, if that set was declared empty) — same criterion
	/// `Pallet::try_close_request`/`try_close_local_requests` use pallet-side.
	/// `settled` is always false while `settlement_id == 0`, and is entirely
	/// independent of `status`/`adapter_legs` — a request's own delivery to the Hub
	/// and its linked settlement's delivery of results back out are two separate
	/// concerns.
	/// @param product_id The product the request belongs to
	/// @param request_id The request to look up
	/// @return info           Investor/vault/amount/order_type, unchanged since Requested
	/// @return request_steps  Ordered Requested + Inbound-leg step history, see above
	/// @return adapter_legs   Per-chain ordered Adapter-leg step history, see above
	/// @return status `Requested` or `RequestCompleted`, see above
	/// @return settlement_id  The settlement this request is linked to, 0 if not yet linked
	/// @return settled        Whether this request's settlement has fully completed (the
	/// investor can now call receive() for it, though that call itself isn't tracked here —
	/// see record_receive_tx)
	/// @return request_bridge_attempts The Inbound leg's full attempt history, empty if no
	/// Inbound leg applies or simply not yet attempted
	/// @return adapter_bridge_attempts Per-chain full Adapter-leg attempt history, parallel
	/// to adapter_legs — a self-fulfilling chain's entry is always empty
	#[precompile::public("get_request(uint64,bytes32)")]
	#[precompile::view]
	#[allow(clippy::type_complexity)]
	fn get_request(
		handle: &mut impl PrecompileHandle,
		product_id: ProductId,
		request_id: H256,
	) -> EvmResult<(
		EvmRequestInfo,
		Vec<EvmRequestTxStep>,
		Vec<EvmAdapterLeg>,
		u8,
		U256,
		bool,
		Vec<EvmBridgeAttempt>,
		Vec<EvmChainBridgeAttempts>,
	)> {
		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let entry =
			pallet_tranche_tx_registry::RequestEntries::<Runtime>::get(product_id, request_id)
				.ok_or_else(|| revert("request not found"))?;

		let info: EvmRequestInfo = (
			Address(entry.investor),
			(entry.vault.chain_id, Address(entry.vault.vault_address)),
			entry.amount,
			encode_request_order_type(entry.order_type),
		);

		// A request colocated with its product's own local chain — Hub for a
		// `Multichain` product's Hub-vault request, or a `SingleChain` product's own
		// chain — has no Inbound leg at all (nothing to bridge when the vault is
		// already colocated with Valuation). `has_inbound_leg` still matters below
		// for the `settled` computation, which asks a different question (Finalize
		// leg vs. Collect/Response completion) than `request_steps`/`queued_done` do.
		let single_chain_id = pallet_tranche_system::Pallet::<Runtime>::single_chain_id(product_id);
		let local_chain_id =
			single_chain_id.unwrap_or_else(<Runtime as pallet_evm::Config>::ChainId::get);
		let has_inbound_leg = entry.vault.chain_id != local_chain_id;
		// A `SingleChain` product's recorder never has a genuine `DepositQueued`/
		// `RedeemQueued` event to observe (no such event exists for that model — see
		// `RequestStep`'s doc comment), so `RequestQueued` is permanently
		// inapplicable for it, not merely pending — omit it entirely (same "absent
		// means not applicable" convention `has_inbound_leg` already uses above) and
		// treat completion as needing only the `Requested` step.
		let is_single_chain = single_chain_id.is_some();
		let queued_done = is_single_chain || entry.queued_tx.is_some();
		let request_bridge_tx = select_executed(&entry.bridge_attempts);
		let request_bridge_attempts = encode_bridge_attempts(entry.bridge_attempts.clone());
		let mut request_steps =
			vec![(encode_request_step(RequestStep::Requested), encode_tx_record(entry.request_tx))];
		if has_inbound_leg {
			request_steps.push((
				encode_request_step(RequestStep::RequestBridgeExecuted),
				encode_tx_record(request_bridge_tx),
			));
		}
		if !is_single_chain {
			request_steps.push((
				encode_request_step(RequestStep::RequestQueued),
				encode_tx_record(entry.queued_tx),
			));
		}

		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let adapter_chain_ids = pallet_tranche_tx_registry::RequestAdapterChains::<Runtime>::get(
			product_id, request_id,
		)
		.unwrap_or_default();

		let mut adapter_legs = Vec::with_capacity(adapter_chain_ids.len());
		let mut adapter_bridge_attempts = Vec::with_capacity(adapter_chain_ids.len());
		let mut all_adapter_done = true;
		for chain_id in adapter_chain_ids.iter() {
			handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
			let leg = pallet_tranche_tx_registry::RequestChainEntries::<Runtime>::get((
				product_id, request_id, *chain_id,
			));
			if leg.applied_tx.is_none() {
				all_adapter_done = false;
			}
			// A chain that's the same as the origin vault's own chain, or this
			// product's own local chain, never gets a Bridge phase at all (fulfilled
			// synchronously — see `pallet_tranche_tx_registry::record_request_tx`'s
			// dev notes), so `AdapterBridgeExecuted` is permanently inapplicable for
			// it, not merely pending — omit it entirely rather than showing a zeroed
			// entry, same "absent means not applicable" convention `request_steps`
			// uses for a colocated request's Inbound leg above.
			let is_self_fulfilling =
				*chain_id == entry.vault.chain_id || *chain_id == local_chain_id;
			let leg_bridge_tx = select_executed(&leg.bridge_attempts);
			let steps = if is_self_fulfilling {
				vec![(
					encode_request_step(RequestStep::AdapterApplied),
					encode_tx_record(leg.applied_tx),
				)]
			} else {
				vec![
					(
						encode_request_step(RequestStep::AdapterBridgeExecuted),
						encode_tx_record(leg_bridge_tx),
					),
					(
						encode_request_step(RequestStep::AdapterApplied),
						encode_tx_record(leg.applied_tx),
					),
				]
			};
			adapter_legs.push((*chain_id, steps));
			adapter_bridge_attempts.push((*chain_id, encode_bridge_attempts(leg.bridge_attempts)));
		}
		let status = if queued_done && all_adapter_done {
			RequestStep::RequestCompleted
		} else {
			RequestStep::Requested
		};

		let Some(settlement_id) = entry.settlement_id else {
			return Ok((
				info,
				request_steps,
				adapter_legs,
				encode_request_step(status),
				U256::zero(),
				false,
				request_bridge_attempts,
				adapter_bridge_attempts,
			));
		};

		// A Hub-vault request has no Finalize leg of its own to wait on (mirrors
		// `inbound_done` above) — it's settled once every one of
		// `SettlementCollectResponseChains` has reached `NavReceived`
		// (vacuously true, including if that set is empty), same criterion
		// `try_close_local_requests` uses pallet-side. A Spoke-vault request
		// instead waits on its own chain's `SettleApplied`, via
		// `SettlementFinalizeChains`.
		let settled =
			if !has_inbound_leg {
				handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
				match pallet_tranche_tx_registry::SettlementCollectResponseChains::<Runtime>::get(
					product_id,
					settlement_id,
				) {
					Some(chains) => {
						let mut all_responded = true;
						for chain_id in chains.iter() {
							handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
							let responded = pallet_tranche_tx_registry::SettlementChainEntries::<
								Runtime,
							>::get((product_id, settlement_id, *chain_id))
							.nav_received_tx
							.is_some();
							if !responded {
								all_responded = false;
								break;
							}
						}
						all_responded
					},
					None => false,
				}
			} else {
				handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
				match pallet_tranche_tx_registry::SettlementFinalizeChains::<Runtime>::get(
					product_id,
					settlement_id,
				) {
					Some(_) => {
						handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
						pallet_tranche_tx_registry::SettlementChainEntries::<Runtime>::get((
							product_id,
							settlement_id,
							entry.vault.chain_id,
						))
						.settle_applied_tx
						.is_some()
					},
					None => false,
				}
			};

		Ok((
			info,
			request_steps,
			adapter_legs,
			encode_request_step(status),
			settlement_id,
			settled,
			request_bridge_attempts,
			adapter_bridge_attempts,
		))
	}
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

/// Wire value matches CCCP-v2's own `SocketEventStatus` discriminant for each
/// outcome (`Executed = 3`, `Reverted = 4`) rather than a bespoke 0-based
/// encoding — see `BridgeStatus`'s own doc comment in
/// `pallet_tranche_tx_registry` for why. `0` (== `SocketEventStatus::None`)
/// is deliberately never returned here — it's reserved as the "not
/// applicable" sentinel `decode_gated_bridge_status` checks for below,
/// unambiguous precisely because neither real status is `0`.
fn encode_bridge_status(status: BridgeStatus) -> u8 {
	match status {
		BridgeStatus::Executed => 3,
		BridgeStatus::Reverted => 4,
	}
}

fn decode_bridge_status(bridge_status: u8) -> EvmResult<BridgeStatus> {
	match bridge_status {
		3 => Ok(BridgeStatus::Executed),
		4 => Ok(BridgeStatus::Reverted),
		_ => Err(revert("invalid bridge_status — expected 3 (Executed) or 4 (Reverted)")),
	}
}

/// Translates a `record_request_tx`/`record_settlement_tx`/`record_whitelist_tx`
/// call's `bridge_status` calldata into the pallet's `Option<BridgeStatus>`,
/// gated by whether the step being recorded is one of that extrinsic's
/// Bridge-phase steps. Reverts if `bridge_status` isn't `0` when
/// `is_bridge_step` is false — matching interface.sol's documented contract.
/// `0` can never be confused with a genuine attempt outcome here since
/// neither `Executed` (3) nor `Reverted` (4) is `0` — see `encode_bridge_status`.
fn decode_gated_bridge_status(
	is_bridge_step: bool,
	bridge_status: u8,
) -> EvmResult<Option<BridgeStatus>> {
	if is_bridge_step {
		Ok(Some(decode_bridge_status(bridge_status)?))
	} else {
		if bridge_status != 0 {
			return Err(revert("bridge_status must be 0 unless step is a Bridge-phase step"));
		}
		Ok(None)
	}
}

/// The `Executed` attempt in `attempts`, if any — what every pre-existing
/// `steps`/`spoke_chains`/`adapter_legs` array entry shows for a Bridge-phase
/// step (zeroed if none, regardless of how many `Reverted` attempts preceded
/// it). See `BridgeAttempts`'s own doc comment for the "which one counts as
/// done" convention this preserves unchanged from before this pallet tracked
/// `Reverted` attempts at all.
fn select_executed<BlockNumber: Clone>(
	attempts: &BridgeAttempts<BlockNumber>,
) -> Option<TxRecord<BlockNumber>> {
	attempts
		.iter()
		.find(|attempt| attempt.status == BridgeStatus::Executed)
		.map(|attempt| attempt.tx.clone())
}

/// Encodes a leg's full attempt history — every attempt observed, `Executed` or
/// `Reverted` alike, in order. The new, "Option A" full-history counterpart to
/// `select_executed`, exposed by `get_request`/`get_settlement`/`get_whitelist`
/// alongside (not instead of) their pre-existing `steps`-shaped return values.
fn encode_bridge_attempts<BlockNumber: Into<U256> + Clone>(
	attempts: BridgeAttempts<BlockNumber>,
) -> Vec<EvmBridgeAttempt> {
	attempts
		.into_iter()
		.map(|attempt| (encode_bridge_status(attempt.status), encode_tx_record(Some(attempt.tx))))
		.collect()
}

/// `a` followed by every id in `b` not already in `a`, deduplicated — used to
/// enumerate a settlement's full chain set for `get_settlement`, since
/// `SettlementCollectResponseChains` and `SettlementFinalizeChains` may overlap
/// (a chain with both a registered Adapter and a registered vault) but
/// `get_settlement` returns one flat, chain-ordered array rather than the two
/// sets separately.
fn union_chain_ids(a: &[pallet_tranche_tx_registry::ChainId], b: &[u64]) -> Vec<u64> {
	let mut ids: Vec<u64> = a.to_vec();
	for id in b {
		if !ids.contains(id) {
			ids.push(*id);
		}
	}
	ids
}

fn decode_request_step(step: u8) -> EvmResult<RequestStep> {
	match step {
		0 => Ok(RequestStep::None),
		1 => Ok(RequestStep::Requested),
		2 => Ok(RequestStep::RequestBridgeExecuted),
		3 => Ok(RequestStep::RequestQueued),
		4 => Ok(RequestStep::AdapterBridgeExecuted),
		5 => Ok(RequestStep::AdapterApplied),
		6 => Ok(RequestStep::RequestCompleted),
		_ => Err(revert("invalid step")),
	}
}

fn encode_request_step(step: RequestStep) -> u8 {
	match step {
		RequestStep::None => 0,
		RequestStep::Requested => 1,
		RequestStep::RequestBridgeExecuted => 2,
		RequestStep::RequestQueued => 3,
		RequestStep::AdapterBridgeExecuted => 4,
		RequestStep::AdapterApplied => 5,
		RequestStep::RequestCompleted => 6,
	}
}

/// Numbering matches `pallet_tranche_tx_registry::SettlementStep`'s own
/// declaration order (`RequestsApproved` sits right after `NavReceived`,
/// where it actually fires — see that enum's doc comment) — this mapping is
/// hand-maintained rather than a derive-based cast, so it's free to do that
/// regardless of the Rust enum's internal SCALE discriminants.
fn decode_settlement_step(step: u8) -> EvmResult<SettlementStep> {
	match step {
		0 => Ok(SettlementStep::Queued),
		1 => Ok(SettlementStep::Triggered),
		2 => Ok(SettlementStep::CollectBridgeExecuted),
		3 => Ok(SettlementStep::NavReported),
		4 => Ok(SettlementStep::ResponseBridgeExecuted),
		5 => Ok(SettlementStep::NavReceived),
		6 => Ok(SettlementStep::RequestsApproved),
		7 => Ok(SettlementStep::FinalizeBridgeExecuted),
		8 => Ok(SettlementStep::SettleApplied),
		9 => Ok(SettlementStep::Settled),
		_ => Err(revert("invalid step")),
	}
}

fn encode_settlement_step(step: SettlementStep) -> u8 {
	match step {
		SettlementStep::Queued => 0,
		SettlementStep::Triggered => 1,
		SettlementStep::CollectBridgeExecuted => 2,
		SettlementStep::NavReported => 3,
		SettlementStep::ResponseBridgeExecuted => 4,
		SettlementStep::NavReceived => 5,
		SettlementStep::RequestsApproved => 6,
		SettlementStep::FinalizeBridgeExecuted => 7,
		SettlementStep::SettleApplied => 8,
		SettlementStep::Settled => 9,
	}
}

fn decode_receive_kind(kind: u8) -> EvmResult<ReceiveKind> {
	match kind {
		0 => Ok(ReceiveKind::Redeem),
		1 => Ok(ReceiveKind::Deposit),
		_ => Err(revert("invalid kind")),
	}
}

fn encode_receive_kind(kind: ReceiveKind) -> u8 {
	match kind {
		ReceiveKind::Redeem => 0,
		ReceiveKind::Deposit => 1,
	}
}

fn decode_request_order_type(order_type: u8) -> EvmResult<OrderType> {
	match order_type {
		0 => Ok(OrderType::Redeem),
		1 => Ok(OrderType::Deposit),
		_ => Err(revert("invalid order_type")),
	}
}

fn decode_whitelist_step(step: u8) -> EvmResult<WhitelistStep> {
	match step {
		0 => Ok(WhitelistStep::None),
		1 => Ok(WhitelistStep::WhitelistRequested),
		2 => Ok(WhitelistStep::BridgeExecuted),
		3 => Ok(WhitelistStep::WhitelistApplied),
		_ => Err(revert("invalid step")),
	}
}

fn encode_whitelist_step(step: WhitelistStep) -> u8 {
	match step {
		WhitelistStep::None => 0,
		WhitelistStep::WhitelistRequested => 1,
		WhitelistStep::BridgeExecuted => 2,
		WhitelistStep::WhitelistApplied => 3,
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
/// iff `step == Requested` (opens a fresh registry entry), all zero/empty otherwise)
/// into the pallet's `Option<RequestOpening>`. Reverts on any partial/inconsistent
/// combination rather than silently ignoring a stray value, matching interface.sol's
/// documented contract.
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

/// Translates `record_request_tx`'s `adapter_chain_ids` calldata into the
/// pallet's `Option<BoundedVec<..>>`. Meaningful (and possibly empty) only for
/// `step == RequestQueued` — Hub-vault and Spoke-vault alike, since
/// `RequestQueued` is precisely the step defined as "the Adapter decision is
/// now known," regardless of vault location — `None`/empty for every other
/// step, including `Requested`. Reverts on any syntactically inconsistent
/// combination, matching interface.sol's documented contract.
fn decode_request_adapter_chains(
	step: RequestStep,
	adapter_chain_ids: &[u64],
) -> EvmResult<Option<BoundedVec<pallet_tranche_tx_registry::ChainId, ConstU32<MAX_SPOKE_CHAINS>>>>
{
	match step {
		RequestStep::RequestQueued => {
			let bounded = BoundedVec::try_from(adapter_chain_ids.to_vec())
				.map_err(|_| revert("too many adapter chains"))?;
			Ok(Some(bounded))
		},
		_ => {
			if !adapter_chain_ids.is_empty() {
				return Err(revert("adapter_chain_ids must be empty unless step == RequestQueued"));
			}
			Ok(None)
		},
	}
}

/// `record_settlement_tx`'s decoded `spoke_chain_id`/`collect_response_chain_ids`/
/// `finalize_chain_ids` triple — either only `spoke_chain_id` is `Some` (a leg step),
/// or both chain sets are (`step == Triggered`), or neither (`step ==
/// RequestsApproved`, settlement-wide like `Triggered` but with no chain sets of
/// its own — see `decode_settlement_request_ids`), matching
/// `pallet_tranche_tx_registry::record_settlement_tx`'s own parameter shapes.
type BoundedChainIds = BoundedVec<pallet_tranche_tx_registry::ChainId, ConstU32<MAX_SPOKE_CHAINS>>;
type DecodedSpokeChains =
	(Option<pallet_tranche_tx_registry::ChainId>, Option<BoundedChainIds>, Option<BoundedChainIds>);

/// Translates `record_settlement_tx`'s flat, sentinel-gated calldata into the
/// pallet's `Option<ChainId>`/`Option<BoundedVec<..>>` triple. Three cases:
/// `step == Triggered` requires `spoke_chain_id == 0` and both
/// `collect_response_chain_ids`/`finalize_chain_ids` that may independently be
/// empty (both empty means the settlement needs no cross-chain action at all);
/// `step == RequestsApproved` also requires `spoke_chain_id == 0` but both chain
/// sets empty (it has `request_ids` instead — see
/// `decode_settlement_request_ids`); every leg step requires non-zero
/// `spoke_chain_id` and both chain sets empty. Reverts on any other combination,
/// matching interface.sol's documented contract.
fn decode_settlement_spoke_chains(
	step: SettlementStep,
	spoke_chain_id: u64,
	collect_response_chain_ids: &[u64],
	finalize_chain_ids: &[u64],
) -> EvmResult<DecodedSpokeChains> {
	if step == SettlementStep::Triggered {
		if spoke_chain_id != 0 {
			return Err(revert("spoke_chain_id must be 0 when step == Triggered"));
		}
		let bounded_collect_response = BoundedVec::try_from(collect_response_chain_ids.to_vec())
			.map_err(|_| revert("too many collect_response chains"))?;
		let bounded_finalize = BoundedVec::try_from(finalize_chain_ids.to_vec())
			.map_err(|_| revert("too many finalize chains"))?;
		Ok((None, Some(bounded_collect_response), Some(bounded_finalize)))
	} else {
		if !collect_response_chain_ids.is_empty() || !finalize_chain_ids.is_empty() {
			return Err(revert(
				"collect_response_chain_ids/finalize_chain_ids must be empty unless step == Triggered",
			));
		}
		if step == SettlementStep::RequestsApproved {
			if spoke_chain_id != 0 {
				return Err(revert("spoke_chain_id must be 0 when step == RequestsApproved"));
			}
			return Ok((None, None, None));
		}
		if spoke_chain_id == 0 {
			return Err(revert(
				"spoke_chain_id required unless step == Triggered or RequestsApproved",
			));
		}
		Ok((Some(spoke_chain_id), None, None))
	}
}

/// Translates `record_settlement_tx`'s `request_ids` calldata into the pallet's
/// `Option<BoundedVec<RequestId, ..>>`. Meaningful (and required non-empty) only
/// for `step == RequestsApproved` — empty/`None` for every other step, same
/// sentinel-gating convention as `decode_request_adapter_chains`. Reverts on any
/// syntactically inconsistent combination, matching interface.sol's documented
/// contract.
fn decode_settlement_request_ids(
	step: SettlementStep,
	request_ids: &[H256],
) -> EvmResult<
	Option<BoundedVec<pallet_tranche_tx_registry::RequestId, ConstU32<MAX_SETTLEMENT_REQUESTS>>>,
> {
	match step {
		SettlementStep::RequestsApproved => {
			if request_ids.is_empty() {
				return Err(revert("request_ids required when step == RequestsApproved"));
			}
			let bounded = BoundedVec::try_from(request_ids.to_vec())
				.map_err(|_| revert("too many request_ids"))?;
			Ok(Some(bounded))
		},
		_ => {
			if !request_ids.is_empty() {
				return Err(revert("request_ids must be empty unless step == RequestsApproved"));
			}
			Ok(None)
		},
	}
}
