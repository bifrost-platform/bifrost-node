#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]

use frame_support::dispatch::{GetDispatchInfo, PostDispatchInfo};
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_evm::AddressMapping;
use pallet_tranche_system::VaultId;
use pallet_tranche_tx_registry::{
	Call as TxRegistryCall, OrderType, ReceiveKind, RequestOpening, RequestStep, SettlementStep,
	TxRecord, MAX_SPOKE_CHAINS,
};
use precompile_utils::prelude::*;
use sp_core::{ConstU32, Get, H160, H256, U256};
use sp_runtime::{traits::Dispatchable, BoundedVec};
use sp_std::{marker::PhantomData, vec, vec::Vec};

// ---------------------------------------------------------------------------
// Event log selectors
// ---------------------------------------------------------------------------

pub(crate) const SELECTOR_LOG_REQUEST_TX_RECORDED: [u8; 32] = keccak256!(
	"RequestTxRecorded(uint256,bytes32,address,uint64,address,uint256,uint8,uint8,uint64[],(uint64,bytes32))"
);
pub(crate) const SELECTOR_LOG_SETTLEMENT_TX_RECORDED: [u8; 32] = keccak256!(
	"SettlementTxRecorded(uint256,uint256,uint64,uint8,uint64[],uint64[],(uint64,bytes32))"
);
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
type EvmInvestorRequest = (U256, H256);

// ---------------------------------------------------------------------------
// Precompile
// ---------------------------------------------------------------------------

/// A precompile that wraps `pallet_tranche_tx_registry`'s `record_*` extrinsics and
/// exposes read-only visibility into its registry storage — plus, for
/// `get_request` only, a read into `pallet-tranche-investments`'
/// `ApprovedInvestments` to resolve a request's linked `settlement_id`/`settled`
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
	/// Attest to one tx in a request's pipeline — the single Requested tx, one
	/// Bridge/Hooks half of the Inbound leg (Spoke-vault requests only), or one
	/// Bridge/Hooks half of a per-chain Adapter leg. See
	/// `pallet_tranche_tx_registry::record_request_tx`'s doc comment for the full
	/// ordering/duplicate-recording contract this dispatches into (including
	/// exactly when `adapter_chain_ids` attaches to `Requested` vs
	/// `InboundHooksExecuted`); this function's own job is only translating
	/// interface.sol's flat, sentinel-gated calldata into the pallet's
	/// `Option<RequestOpening>`/`Option<BoundedVec<..>>` shapes.
	///
	/// @param investor              Investor address — required iff step == Requested
	/// @param vault_chain_id        EVM chain ID of the tranche vault — required iff
	/// step == Requested
	/// @param vault_address         ERC-7540 vault contract address — required iff
	/// step == Requested
	/// @param amount                Investor's full requested amount — required iff
	/// step == Requested
	/// @param order_type            0 = redeem, 1 = deposit — meaningful iff step == Requested
	/// @param adapter_chain_ids Every chain (besides Hub) needing its own Adapter
	/// leg — meaningful (and may be empty) iff step == Requested (Hub-vault only) or
	/// step == InboundHooksExecuted (Spoke-vault only), empty otherwise
	/// @param step                  0 = Queued (never valid here), 1 = Requested,
	/// 2 = InboundBridgeExecuted, 3 = InboundHooksExecuted,
	/// 4 = AdapterBridgeExecuted, 5 = AdapterHooksExecuted,
	/// 6 = Completed (never valid here)
	#[precompile::public(
		"record_request_tx(uint256,bytes32,address,uint64,address,uint256,uint8,uint64[],uint8,(uint64,bytes32))"
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
		adapter_chain_ids: Vec<u64>,
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
		let decoded_adapter_chains =
			decode_request_adapter_chains(decoded_step, &adapter_chain_ids)?;
		let (chain_id, tx_hash) = attestation;

		let caller_account = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = TxRegistryCall::<Runtime>::record_request_tx {
			product_id: to_product_id(product_id)?,
			request_id,
			opening,
			adapter_chain_ids: decoded_adapter_chains,
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
				adapter_chain_ids,
				attestation,
			)),
		);
		handle.record_log_costs(&[&event])?;
		event.record(handle)?;

		Ok(())
	}

	/// Attest to one tx in a settlement's pipeline: either the single Trigger tx, or
	/// one bridge/hooks half of a Collect/Response/Finalize leg for one chain. A
	/// settlement needing no cross-chain action at all is recorded as `Triggered`
	/// with both chain sets empty. See
	/// `pallet_tranche_tx_registry::record_settlement_tx`'s doc comment for the full
	/// ordering/duplicate-recording contract this dispatches into; this function's own
	/// job is only translating interface.sol's flat, sentinel-gated calldata into the
	/// pallet's `Option<ChainId>`/`Option<BoundedVec<..>>` shapes.
	///
	/// @param spoke_chain_id  The spoke chain this leg step is for — 0 if step == Triggered
	/// @param collect_response_chain_ids Chains needing a Collect/Response leg (have a
	/// registered Adapter) — meaningful (and may be empty) iff step == Triggered
	/// @param finalize_chain_ids Chains needing a Finalize leg (have a registered vault) —
	/// meaningful (and may be empty) iff step == Triggered
	/// @param step            0 = Queued (never valid here), 1 = Triggered,
	/// 2-7 = leg steps, 8 = Settled (never valid here)
	#[precompile::public(
		"record_settlement_tx(uint256,uint256,uint64,uint64[],uint64[],uint8,(uint64,bytes32))"
	)]
	fn record_settlement_tx(
		handle: &mut impl PrecompileHandle,
		product_id: U256,
		settlement_id: U256,
		spoke_chain_id: u64,
		collect_response_chain_ids: Vec<u64>,
		finalize_chain_ids: Vec<u64>,
		step: u8,
		attestation: EvmTxAttestation,
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
		let (chain_id, tx_hash) = attestation;

		let caller_account = Runtime::AddressMapping::into_account_id(handle.context().caller);
		let call = TxRegistryCall::<Runtime>::record_settlement_tx {
			product_id: to_product_id(product_id)?,
			settlement_id,
			spoke_chain_id: decoded_spoke_chain_id,
			collect_response_chain_ids: decoded_collect_response_chain_ids,
			finalize_chain_ids: decoded_finalize_chain_ids,
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
			solidity::encode_event_data((
				step,
				collect_response_chain_ids,
				finalize_chain_ids,
				attestation,
			)),
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

	/// Read a settlement's full state in one call: Trigger evidence, the settlement's
	/// own overall status, and every registered chain's ordered step-by-step history
	/// (each entry only for the leg kind(s) that chain's role actually needs — no
	/// zeroed-forever "not applicable" fields to interpret). Does not revert for an
	/// untriggered (product_id, settlement_id) — returns a zeroed `trigger_tx`,
	/// `status == Queued`, and empty `spoke_chains` instead.
	///
	/// `spoke_chains[i].steps` is exactly `[CollectBridgeExecuted, CollectHooksExecuted,
	/// ResponseBridgeExecuted, ResponseHooksExecuted]` for a chain with only a
	/// registered Adapter, `[FinalizeBridgeExecuted, FinalizeHooksExecuted]` for a
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
	#[precompile::public("get_settlement(uint256,uint256)")]
	#[precompile::view]
	fn get_settlement(
		handle: &mut impl PrecompileHandle,
		product_id: U256,
		settlement_id: U256,
	) -> EvmResult<(EvmTxRecord, u8, Vec<EvmSettlementChainSteps>)> {
		let product_id = to_product_id(product_id)?;

		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let Some(trigger_tx) = pallet_tranche_tx_registry::SettlementTriggers::<Runtime>::get(
			product_id,
			settlement_id,
		) else {
			return Ok((
				encode_tx_record::<BlockNumberFor<Runtime>>(None),
				encode_settlement_step(SettlementStep::Queued),
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
				entry.finalize_hooks_tx.is_some()
			} else {
				entry.response_hooks_tx.is_some()
			};
			if !complete {
				all_complete = false;
			}

			let mut steps = Vec::with_capacity(
				if needs_collect_response { 4 } else { 0 } + if needs_finalize { 2 } else { 0 },
			);
			if needs_collect_response {
				steps.push((
					encode_settlement_step(SettlementStep::CollectBridgeExecuted),
					encode_tx_record(entry.collect_bridge_tx),
				));
				steps.push((
					encode_settlement_step(SettlementStep::CollectHooksExecuted),
					encode_tx_record(entry.collect_hooks_tx),
				));
				steps.push((
					encode_settlement_step(SettlementStep::ResponseBridgeExecuted),
					encode_tx_record(entry.response_bridge_tx),
				));
				steps.push((
					encode_settlement_step(SettlementStep::ResponseHooksExecuted),
					encode_tx_record(entry.response_hooks_tx),
				));
			}
			if needs_finalize {
				steps.push((
					encode_settlement_step(SettlementStep::FinalizeBridgeExecuted),
					encode_tx_record(entry.finalize_bridge_tx),
				));
				steps.push((
					encode_settlement_step(SettlementStep::FinalizeHooksExecuted),
					encode_tx_record(entry.finalize_hooks_tx),
				));
			}
			spoke_chains.push((*chain_id, steps));
		}

		let status = if all_complete { SettlementStep::Settled } else { SettlementStep::Triggered };
		Ok((encode_tx_record(Some(trigger_tx)), encode_settlement_step(status), spoke_chains))
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

	/// Read a request's full state in one call: its static details (bundled as one
	/// `RequestInfo`), the Requested/Inbound-leg evidence (bundled as one ordered
	/// `request_steps` array, same "step, tx" shape as every per-chain leg entry —
	/// see below), every declared Adapter chain's ordered step-by-step history, the
	/// request's own overall `status`, and its linked settlement's completion. Reverts
	/// if `record_request_tx` has never been called with `step == Requested` for
	/// this `request_id`.
	///
	/// `request_steps[0]` is always `(Requested, request_tx)` — every request that
	/// exists has one, unconditionally. For a Hub-vault request (no Inbound leg
	/// applies at all — `info.vault.chain_id` equals this chain's own EVM chain ID),
	/// that's the array's only entry; for a Spoke-vault one, two more entries follow:
	/// `(InboundBridgeExecuted, ...)` then `(InboundHooksExecuted, ...)`. Same
	/// "absent means not applicable, present-but-zeroed means pending" convention as
	/// `get_settlement`'s `spoke_chains[i].steps` — check `request_steps.length` (1
	/// vs 3) to tell whether this request has an Inbound leg at all, and each
	/// present entry's own `tx.recorded_at` to tell whether it's landed yet. Each
	/// `adapter_legs[i].steps` is always exactly `[AdapterBridgeExecuted,
	/// AdapterHooksExecuted]`, ordered as declared (at `Requested` for a Hub-vault
	/// request, at the Inbound leg's own `InboundHooksExecuted` for a Spoke-vault
	/// one).
	///
	/// `status` only ever takes `Requested` (Inbound leg, if any, or some
	/// Adapter leg still has an unfinished step) or `Completed` (Inbound leg, if
	/// any, done, and every declared Adapter chain's last step landed, or none were
	/// declared at all — immediate for a fully local request).
	///
	/// Unlike every other function here, this also reads
	/// `pallet-tranche-investments::ApprovedInvestments` directly to resolve
	/// `settlement_id`/`settled` — `pallet_tranche_tx_registry` the pallet
	/// deliberately has no dependency on `pallet-tranche-investments` (see this
	/// pallet's module docs on why the two precompiles were split apart), but that
	/// decoupling is a pallet-level concern, not a precompile-level one: this
	/// precompile crate is the per-runtime aggregation layer, and
	/// `TrancheInvestmentsPrecompile` itself already sets the precedent of reading
	/// `pallet-tranche-system`'s storage directly despite `pallet-tranche-investments`
	/// not depending on that pallet either. `settlement_id` is 0 until this request is
	/// linked to a settlement via the Investments precompile's
	/// `record_investment_approval`. `settled` depends on whether this request's own
	/// vault is on Hub or Spoke, mirroring the Inbound-leg asymmetry above: for a
	/// Spoke vault, true once the Finalize leg's Hooks phase lands for this request's
	/// own origin chain; for a Hub vault (no Finalize leg of its own to wait on),
	/// true once every one of the linked settlement's `collect_response_chain_ids`
	/// reaches `ResponseHooksExecuted` (vacuously true, and immediate, if that set
	/// was declared empty). `settled` is always false while `settlement_id == 0`,
	/// and is entirely independent of `status`/`adapter_legs` — a request's
	/// own delivery to the Hub and its linked settlement's delivery of results back
	/// out are two separate concerns.
	/// @param product_id The product the request belongs to
	/// @param request_id The request to look up
	/// @return info           Investor/vault/amount/order_type, unchanged since Requested
	/// @return request_steps  Ordered Requested + Inbound-leg step history, see above
	/// @return adapter_legs   Per-chain ordered Adapter-leg step history, see above
	/// @return status `Requested` or `Completed`, see above
	/// @return settlement_id  The settlement this request is linked to, 0 if not yet linked
	/// @return settled        Whether this request's settlement has fully completed (the
	/// investor can now call claim() for it, though that call itself isn't tracked here —
	/// see record_receive_tx)
	#[precompile::public("get_request(uint256,bytes32)")]
	#[precompile::view]
	#[allow(clippy::type_complexity)]
	fn get_request(
		handle: &mut impl PrecompileHandle,
		product_id: U256,
		request_id: H256,
	) -> EvmResult<(EvmRequestInfo, Vec<EvmRequestTxStep>, Vec<EvmAdapterLeg>, u8, U256, bool)> {
		let product_id = to_product_id(product_id)?;
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

		// A Hub-vault request has no Inbound leg at all (Requested already means the
		// deposit is at the Valuation Contract) — trivially "done" for completion purposes.
		let hub_chain_id = <Runtime as pallet_evm::Config>::ChainId::get();
		let has_inbound_leg = entry.vault.chain_id != hub_chain_id;
		let inbound_done = !has_inbound_leg || entry.hooks_tx.is_some();
		let mut request_steps =
			vec![(encode_request_step(RequestStep::Requested), encode_tx_record(entry.request_tx))];
		if has_inbound_leg {
			request_steps.push((
				encode_request_step(RequestStep::InboundBridgeExecuted),
				encode_tx_record(entry.bridge_tx),
			));
			request_steps.push((
				encode_request_step(RequestStep::InboundHooksExecuted),
				encode_tx_record(entry.hooks_tx),
			));
		}

		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let adapter_chain_ids = pallet_tranche_tx_registry::RequestAdapterChains::<Runtime>::get(
			product_id, request_id,
		)
		.unwrap_or_default();

		let mut adapter_legs = Vec::with_capacity(adapter_chain_ids.len());
		let mut all_adapter_done = true;
		for chain_id in adapter_chain_ids.iter() {
			handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
			let leg = pallet_tranche_tx_registry::RequestChainEntries::<Runtime>::get((
				product_id, request_id, *chain_id,
			));
			if leg.hooks_tx.is_none() {
				all_adapter_done = false;
			}
			let steps = vec![
				(
					encode_request_step(RequestStep::AdapterBridgeExecuted),
					encode_tx_record(leg.bridge_tx),
				),
				(
					encode_request_step(RequestStep::AdapterHooksExecuted),
					encode_tx_record(leg.hooks_tx),
				),
			];
			adapter_legs.push((*chain_id, steps));
		}
		let status = if inbound_done && all_adapter_done {
			RequestStep::Completed
		} else {
			RequestStep::Requested
		};

		handle.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		let Some(approved) =
			pallet_tranche_investments::ApprovedInvestments::<Runtime>::get(product_id, request_id)
		else {
			return Ok((
				info,
				request_steps,
				adapter_legs,
				encode_request_step(status),
				U256::zero(),
				false,
			));
		};
		let settlement_id = approved.settlement_id;

		// A Hub-vault request has no Finalize leg of its own to wait on (mirrors
		// `inbound_done` above) — it's settled once every one of
		// `SettlementCollectResponseChains` has reached `ResponseHooksExecuted`
		// (vacuously true, including if that set is empty), same criterion
		// `try_close_hub_vault_requests` uses pallet-side. A Spoke-vault request
		// instead waits on its own chain's `FinalizeHooksExecuted`, via
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
							.response_hooks_tx
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
						.finalize_hooks_tx
						.is_some()
					},
					None => false,
				}
			};

		Ok((info, request_steps, adapter_legs, encode_request_step(status), settlement_id, settled))
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
		0 => Ok(RequestStep::Queued),
		1 => Ok(RequestStep::Requested),
		2 => Ok(RequestStep::InboundBridgeExecuted),
		3 => Ok(RequestStep::InboundHooksExecuted),
		4 => Ok(RequestStep::AdapterBridgeExecuted),
		5 => Ok(RequestStep::AdapterHooksExecuted),
		6 => Ok(RequestStep::Completed),
		_ => Err(revert("invalid step")),
	}
}

fn encode_request_step(step: RequestStep) -> u8 {
	match step {
		RequestStep::Queued => 0,
		RequestStep::Requested => 1,
		RequestStep::InboundBridgeExecuted => 2,
		RequestStep::InboundHooksExecuted => 3,
		RequestStep::AdapterBridgeExecuted => 4,
		RequestStep::AdapterHooksExecuted => 5,
		RequestStep::Completed => 6,
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
/// pallet's `Option<BoundedVec<..>>`. Meaningful (and possibly empty) for
/// `step == Requested` or `step == InboundHooksExecuted` — the two steps that
/// can represent a request's capital arriving at the Valuation Contract
/// (Hub-vault and Spoke-vault respectively) — must be empty for every other
/// step. Whether a given `step`/vault-chain combination is actually *valid* for
/// declaring adapter_chain_ids at all (e.g. `Requested` only accepts it for
/// a Hub-vault request) is left to the pallet to enforce, since only it knows
/// the request's own vault chain once `step != Requested`. Reverts on any
/// syntactically inconsistent combination, matching interface.sol's documented
/// contract.
fn decode_request_adapter_chains(
	step: RequestStep,
	adapter_chain_ids: &[u64],
) -> EvmResult<Option<BoundedVec<pallet_tranche_tx_registry::ChainId, ConstU32<MAX_SPOKE_CHAINS>>>>
{
	match step {
		RequestStep::Requested | RequestStep::InboundHooksExecuted => {
			let bounded = BoundedVec::try_from(adapter_chain_ids.to_vec())
				.map_err(|_| revert("too many adapter chains"))?;
			Ok(Some(bounded))
		},
		_ => {
			if !adapter_chain_ids.is_empty() {
				return Err(revert(
					"adapter_chain_ids must be empty unless step == Requested or InboundHooksExecuted",
				));
			}
			Ok(None)
		},
	}
}

/// `record_settlement_tx`'s decoded `spoke_chain_id`/`collect_response_chain_ids`/
/// `finalize_chain_ids` triple — either only `spoke_chain_id` is `Some` (a leg step),
/// or both chain sets are (never a mix), matching
/// `pallet_tranche_tx_registry::record_settlement_tx`'s own parameter shapes.
type BoundedChainIds = BoundedVec<pallet_tranche_tx_registry::ChainId, ConstU32<MAX_SPOKE_CHAINS>>;
type DecodedSpokeChains =
	(Option<pallet_tranche_tx_registry::ChainId>, Option<BoundedChainIds>, Option<BoundedChainIds>);

/// Translates `record_settlement_tx`'s flat, sentinel-gated calldata into the
/// pallet's `Option<ChainId>`/`Option<BoundedVec<..>>` triple. Two cases:
/// `step == Triggered` requires `spoke_chain_id == 0` and both
/// `collect_response_chain_ids`/`finalize_chain_ids` that may independently be
/// empty (both empty means the settlement needs no cross-chain action at all);
/// every leg step requires non-zero `spoke_chain_id` and both empty. Reverts on
/// any other combination, matching interface.sol's documented contract.
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
		if spoke_chain_id == 0 {
			return Err(revert("spoke_chain_id required unless step == Triggered"));
		}
		Ok((Some(spoke_chain_id), None, None))
	}
}
