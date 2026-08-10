// SPDX-License-Identifier: GPL-3.0-only
pragma solidity >=0.8.0;

/**
 * @title Tranche Tx Registry Precompile Interface (tranche-system draft)
 * @notice Off-chain tx registry for the tranche-system's deposit/redeem request pipeline,
 *         settlement pipeline, and vault claims. CCCP-v2 is a Bridge&Call protocol: every
 *         cross-chain message costs two on-chain tx — a bridge-vote tx (relayers submit
 *         ⅔+ signatures, emitting SocketMessage.status = Executed) followed by a separate
 *         tx that calls Hooks.execute() on the destination chain to actually run the
 *         payload. Neither of those tx (nor a vault's local claim() tx) touches the
 *         Investments pallet directly, so a single trusted off-chain recorder account
 *         attests to each one after independently detecting it, giving
 *         investors/dashboards a way to verify exactly which step a request or settlement
 *         is currently at. The recorder account itself is registered via a pallet-native
 *         extrinsic (Root-gated, not exposed through this EVM interface — same pattern as
 *         pallet-tranche-system's Orchestrator registration), not through a function here;
 *         record_request_tx/record_settlement_tx/record_receive_tx below are the only entry
 *         points this interface exposes, and all three revert unless `msg.sender` matches
 *         that registration.
 *
 * Three things are covered by this registry:
 *  - Request (per request_id, 3 tx): request_tx (Spoke) -> bridge_tx (Spoke->Hub) ->
 *    hooks_tx (Hub).
 *  - Settlement (per settlement_id, fanned out per spoke chain): a single
 *    trigger_tx (Hub-local tryUpdateNAV), then three message legs per chain — Collect
 *    (Hub->Spoke NAV request), Response (Spoke->Hub NAV report), Finalize (Hub->Spoke
 *    settle result) — each leg itself a bridge_tx/hooks_tx pair. Legs progress
 *    independently per chain; there is no cross-chain ordering constraint.
 *  - Receive (per investor per vault): a plain local Spoke-chain claim() tx, not part of
 *    the Bridge&Call pipelines above. TrancheManager pools receivable amounts per
 *    (investor, vault) rather than per request_id, so receives are tracked separately
 *    from the request pipeline (see record_receive_tx's dev notes for why).
 *
 * DRAFT — split out of the Investments precompile (2026-08-06) once the tx-tracing
 * functionality outgrew it: different trust model (a single bot-driven recorder account
 * vs. the product's Valuation Contract), different write volume (expected far more
 * frequent than Investments' own ledger calls), and potential reuse by other CCCP-v2
 * Bridge&Call flows beyond tranche-investments. pallet-tranche-tx-registry does not exist
 * yet; this interface is written ahead of the pallet, same as tranche-system/investments
 * (see [[project_omnifi_revamp_callflow]] in project memory).
 *
 *   - Address: next free slot after tranche-system (0x...0200), investments (0x...0201),
 *     tranche-permissions (0x...0202) — 0x...0203 was briefly assigned to the
 *     now-deleted precompile-rwa-loans and is confirmed free for reuse here.
 *
 * Address: 0x0000000000000000000000000000000000000203
 */
interface TrancheTxRegistry {
    /// @dev Mirrors Investments's VaultInput — duplicated here rather than shared, same
    ///      pattern already used between tranche-system and tranche-permissions.
    /// @param chain_id      EVM chain ID where the tranche's ERC-7540 vault is deployed
    /// @param vault_address ERC-7540 vault contract address identifying the tranche
    struct VaultInput {
        uint64 chain_id;
        address vault_address;
    }

    /// @dev Caller-attested off-chain tx reference — what the recorder claims happened.
    /// @param chain_id EVM chain ID the tx occurred on
    /// @param tx_hash  Transaction hash on that chain
    struct TxAttestation {
        uint64 chain_id;
        bytes32 tx_hash;
    }

    /// @dev Stored form of a TxAttestation — identical fields plus `recorded_at`, this
    ///      chain's own block number when the attestation was accepted. `recorded_at == 0`
    ///      means "not yet recorded" (used as the unset sentinel by every view function
    ///      below that returns TxRecord without reverting).
    struct TxRecord {
        uint64 chain_id;
        bytes32 tx_hash;
        uint256 recorded_at;
    }

    /// @dev One spoke chain's full leg-by-leg record set within a settlement.
    /// @param spoke_chain_id     The spoke chain this record set is for
    /// @param collect_bridge_tx  Collect leg, Bridge phase
    /// @param collect_hooks_tx   Collect leg, Hooks phase
    /// @param response_bridge_tx Response leg, Bridge phase
    /// @param response_hooks_tx  Response leg, Hooks phase
    /// @param finalize_bridge_tx Finalize leg, Bridge phase
    /// @param finalize_hooks_tx  Finalize leg, Hooks phase
    struct SettlementChainRecord {
        uint64 spoke_chain_id;
        TxRecord collect_bridge_tx;
        TxRecord collect_hooks_tx;
        TxRecord response_bridge_tx;
        TxRecord response_hooks_tx;
        TxRecord finalize_bridge_tx;
        TxRecord finalize_hooks_tx;
    }

    /// @dev One spoke chain's coarse status within a settlement.
    /// @param spoke_chain_id The spoke chain this status is for
    /// @param current_step   Furthest step completed for this chain — `Queued` if nothing has
    ///                       been recorded for it yet, `FinalizeHooksExecuted` once fully done.
    ///                       Never `Settled` — that value only ever appears as
    ///                       get_settlement_status's hub-level `hub_status`, not here.
    struct SettlementChainStatus {
        uint64 spoke_chain_id;
        SettlementStep current_step;
    }

    /// @dev One of an investor's in-flight requests.
    /// @param product_id Product the request belongs to
    /// @param request_id The request itself
    struct InvestorRequest {
        uint256 product_id;
        bytes32 request_id;
    }

    /// @dev Step within the 3-tx request pipeline, in order.
    enum RequestStep {
        Requested,
        BridgeExecuted,
        HooksExecuted
    }

    /// @dev Step within the settlement pipeline. `Queued`/`Triggered`/`Settled` are the
    ///      three hub-level states get_settlement_status's `hub_status` moves through —
    ///      `Queued` (not yet triggered), `Triggered` (triggered, awaiting completion),
    ///      `Settled` (every spoke chain has reached `FinalizeHooksExecuted`). The
    ///      other six describe one spoke chain's own progress instead — a
    ///      Collect/Response/Finalize leg crossed with a Bridge/Hooks phase (each suffixed
    ///      `Executed`, since by the time any of these six is recorded, that half of the leg
    ///      has already landed) — flattened into this same enum so record_settlement_tx
    ///      takes a single step argument. `Queued` and `Settled` are read-only sentinels —
    ///      "nothing recorded yet" and "hub-level completion", respectively — and must never
    ///      be passed to record_settlement_tx.
    enum SettlementStep {
        Queued,
        Triggered,
        CollectBridgeExecuted,
        CollectHooksExecuted,
        ResponseBridgeExecuted,
        ResponseHooksExecuted,
        FinalizeBridgeExecuted,
        FinalizeHooksExecuted,
        Settled
    }

    /// @dev Which receivable pool a claim() tx drained — TrancheManager pools receivable
    ///      amounts per (investor, vault), not per request_id, so redeem/deposit still need
    ///      distinguishing even though neither is tied to one specific request anymore.
    ///      Renamed from `ClaimKind` (2026-08-06) — "Claim" as a term for this whole
    ///      tracking pipeline was replaced with "Receive" throughout; the underlying
    ///      investor-facing Solidity call being tracked is still literally named `claim()`
    ///      on TrancheVault, that's an external fact this rename doesn't change.
    enum ReceiveKind {
        Redeem,
        Deposit
    }

    /// @dev `investor`/`vault_chain_id`/`vault_address`/`amount`/`order_type` are only
    ///      meaningful when `step == Requested` (zero/empty otherwise) — same sentinel
    ///      convention as record_request_tx's own parameters. Deliberately no
    ///      `settlement_id` field here — see record_request_tx's dev notes for why.
    event RequestTxRecorded(
        uint256 indexed product_id,
        bytes32 indexed request_id,
        address indexed investor,
        uint64 vault_chain_id,
        address vault_address,
        uint256 amount,
        uint8 order_type,
        RequestStep step,
        TxAttestation attestation
    );

    /// @dev `spoke_chain_id` is 0 and `spoke_chain_ids` is non-empty only when
    ///      `step == Triggered`; otherwise `spoke_chain_id` identifies the spoke chain and
    ///      `spoke_chain_ids` is empty — same sentinel convention as record_settlement_tx's
    ///      own parameters. `settlement_id` is only unique within `product_id`'s own
    ///      namespace (each product's Valuation Contract generates its own sequence), never
    ///      globally.
    event SettlementTxRecorded(
        uint256 indexed product_id,
        uint256 indexed settlement_id,
        uint64 indexed spoke_chain_id,
        SettlementStep step,
        uint64[] spoke_chain_ids,
        TxAttestation attestation
    );

    event ReceiveTxRecorded(
        uint256 indexed product_id,
        address indexed investor,
        VaultInput vault,
        ReceiveKind kind,
        TxAttestation attestation
    );

    /**
     * @notice Attest to one tx in a request's 3-tx pipeline.
     * @dev Only callable by the pallet-registered tx recorder account. `investor`/
     *      `vault_chain_id`/`vault_address`/`amount`/`order_type` are sentinel-gated
     *      together: they MUST all be non-zero/non-empty when `step == Requested` (this
     *      call opens a fresh registry entry) and MUST all be zero/empty for every other
     *      step (rejected otherwise, to catch caller bugs early rather than silently
     *      ignoring a stray value). Deliberately no `settlement_id` parameter — it isn't
     *      actually knowable at request_tx time: it's assigned by the Valuation Contract
     *      only once the request lands on the Hub and record_investment_request runs, and
     *      the Spoke-side bridge message itself carries no settlement_id field for the
     *      recorder to observe earlier than that. Steps must be recorded in order
     *      (Requested, then BridgeExecuted, then HooksExecuted) with no duplicates, and
     *      `step == Requested` must not be called twice for the same request_id.
     *      Opening a registry entry registers (product_id, request_id) under the investor for
     *      get_investor_active_requests; this registration is independent of, and can
     *      precede, the Investments precompile's record_investment_request (which is only
     *      callable once the request has actually landed on the Hub).
     *      Emits RequestTxRecorded.
     * @param product_id     The product this request belongs to
     * @param request_id     The request this entry is for
     * @param investor       Investor address — required iff step == Requested, else address(0)
     * @param vault_chain_id EVM chain ID of the tranche vault — required iff step == Requested
     * @param vault_address  ERC-7540 vault contract address — required iff step == Requested
     * @param amount         Investor's full requested amount — required iff step == Requested
     * @param order_type     0 = redeem, 1 = deposit — meaningful iff step == Requested
     * @param step           Which pipeline step this attestation is for
     * @param attestation    The attested off-chain tx
     */
    function record_request_tx(
        uint256 product_id,
        bytes32 request_id,
        address investor,
        uint64 vault_chain_id,
        address vault_address,
        uint256 amount,
        uint8 order_type,
        RequestStep step,
        TxAttestation calldata attestation
    ) external;

    /**
     * @notice Attest to one tx in a settlement's pipeline: either the single Trigger tx, or
     *         one bridge/hooks half of a Collect/Response/Finalize leg for one chain.
     * @dev Only callable by the pallet-registered tx recorder account. `step` MUST NOT be
     *      `SettlementStep.Queued` or `SettlementStep.Settled` — both are read-only
     *      sentinels reserved for get_settlement_status's hub-level `hub_status`, never a
     *      valid attestation to record. `settlement_id` is only unique within
     *      `product_id`'s own namespace — each product's Valuation
     *      Contract generates its own sequence, same scoping as every settlement_id use in
     *      the Investments precompile (e.g. record_tranche_settlement) — so all storage
     *      here is keyed by (product_id, settlement_id), never settlement_id alone.
     *      `spoke_chain_id` and `spoke_chain_ids` are sentinel-gated, mirroring
     *      record_request_tx: for `step == Triggered`, `spoke_chain_id` MUST be 0 and
     *      `spoke_chain_ids` MUST be non-empty (the full set of spoke chain IDs this
     *      settlement will collect from/distribute to); for every other step,
     *      `spoke_chain_id` MUST be non-zero (and a member of the settlement's
     *      already-registered spoke chains) and `spoke_chain_ids` MUST be empty. Safe as a
     *      sentinel because no EVM chain in this protocol's supported set is ever assigned
     *      chain_id 0. `spoke_chain_id` identifies which spoke chain a leg step is about —
     *      not necessarily the chain `attestation.chain_id` itself executed on, since e.g.
     *      the Response leg's bridge_tx/hooks_tx both physically execute on the Hub (see
     *      get_settlement_record's dev notes) even though `spoke_chain_id` there still
     *      names the spoke chain being responded for. `Triggered` must be recorded exactly
     *      once per (product_id, settlement_id), before any leg step for that settlement.
     *      Within a given (spoke_chain_id, leg) pair, Bridge must be recorded before Hooks,
     *      with no duplicates — this ordering is enforced only within that pair, not across
     *      chains or legs, since chains progress independently.
     *      Emits SettlementTxRecorded.
     * @param product_id     The product this settlement belongs to
     * @param settlement_id  The settlement cycle this attestation belongs to
     * @param spoke_chain_id  The spoke chain this leg step is for — 0 if step == Triggered
     * @param spoke_chain_ids Full spoke chain ID set — non-empty if step == Triggered
     * @param step            Which pipeline step this attestation is for
     * @param attestation     The attested off-chain tx
     */
    function record_settlement_tx(
        uint256 product_id,
        uint256 settlement_id,
        uint64 spoke_chain_id,
        uint64[] calldata spoke_chain_ids,
        SettlementStep step,
        TxAttestation calldata attestation
    ) external;

    /**
     * @notice Attest to an investor's claim() tx on a vault — a plain local Spoke-chain tx,
     *         not part of the Bridge&Call request/settlement pipelines above. TrancheManager
     *         pools receivable amounts per (investor, vault) rather than per request_id, so
     *         this is intentionally NOT keyed by request_id and has no step ordering — one
     *         attestation per receive.
     * @dev Only callable by the pallet-registered tx recorder account. There is no
     *      per-request_id link here — once a request reaches `receivable` (see
     *      get_request_status), tracking "was THIS request specifically claimed" stops being
     *      meaningful, since a single claim() may drain a pooled balance spanning several
     *      distributed requests at once. No dedicated getter yet — ReceiveTxRecorded is the
     *      only way to observe receives for now.
     *      Emits ReceiveTxRecorded.
     * @param product_id  The product the claimed vault belongs to
     * @param vault       The vault the investor claimed against
     * @param investor    The investor who claimed
     * @param kind        Which receivable pool this claim drained
     * @param attestation The attested off-chain claim tx
     */
    function record_receive_tx(
        uint256 product_id,
        VaultInput calldata vault,
        address investor,
        ReceiveKind kind,
        TxAttestation calldata attestation
    ) external;

    /**
     * @notice Read a request's full 3-tx registry entry.
     * @dev Reverts if record_request_tx has never been called with step == Requested for
     *      this request_id. Unrecorded later steps are returned as a zeroed TxRecord
     *      (recorded_at == 0), not a revert — check recorded_at to distinguish "not yet"
     *      from a genuine zero value.
     * @param product_id The product the request belongs to
     * @param request_id The request to look up
     * @return investor    Investor address the registry entry was opened with
     * @return request_tx  Evidence for step 1 (Requested)
     * @return bridge_tx   Evidence for step 2 (BridgeExecuted)
     * @return hooks_tx    Evidence for step 3 (HooksExecuted)
     */
    function get_request_record(
        uint256 product_id,
        bytes32 request_id
    )
        external
        view
        returns (
            address investor,
            TxRecord memory request_tx,
            TxRecord memory bridge_tx,
            TxRecord memory hooks_tx
        );

    /**
     * @notice Read a settlement's Trigger evidence plus every registered spoke chain's
     *         full leg-by-leg registry entry — all three legs' Bridge/Hooks evidence, per
     *         chain.
     * @dev Reverts if record_settlement_tx has never been called with step == Triggered
     *      for this (product_id, settlement_id) — settlement_id alone is not a valid key,
     *      since it's only unique within product_id's own namespace (see
     *      record_settlement_tx's dev notes). Spoke chain IDs are not returned
     *      separately — read them off `chains[i].spoke_chain_id`, in the order registered at
     *      Trigger time. Any field within a SettlementChainRecord not yet recorded comes
     *      back as a zeroed TxRecord (recorded_at == 0) rather than causing a revert — check
     *      recorded_at, not the rest of the struct, to tell "not yet" apart from a genuine
     *      zero value. To check "has every chain completed leg X phase Y," iterate `chains`
     *      and test the corresponding field's recorded_at off-chain — there is no separate
     *      on-chain aggregate for this, since nothing else in this interface consumes it
     *      (Valuation tracks its own completion condition internally; this call exists
     *      purely for external registry visibility).
     * @param product_id    The product the settlement belongs to
     * @param settlement_id The settlement to look up
     * @return trigger_tx Evidence for the Trigger step
     * @return chains     Per-spoke-chain leg registry entries
     */
    function get_settlement_record(
        uint256 product_id,
        uint256 settlement_id
    )
        external
        view
        returns (
            TxRecord memory trigger_tx,
            SettlementChainRecord[] memory chains
        );

    /**
     * @notice Read a settlement's coarse status at two levels: `hub_status`, the overall
     *         hub-level state, and — per registered spoke chain — `spoke_statuses`, that
     *         chain's own current step. Both are composed directly from SettlementStep,
     *         the same vocabulary record_settlement_tx writes with, rather than a separate
     *         coarse-status enum.
     * @dev Does not revert for an untriggered (product_id, settlement_id) — returns
     *      `hub_status == SettlementStep.Queued` and an empty `spoke_statuses` array instead,
     *      so callers can poll a not-yet-started settlement_id without a revert (unlike
     *      get_settlement_record, which does revert in that case).
     *
     *      `hub_status` only ever takes one of three values:
     *      - `Queued` — Trigger not yet recorded.
     *      - `Triggered` — Trigger recorded, but at least one spoke chain hasn't yet
     *        reached `FinalizeHooksExecuted`.
     *      - `Settled` — every spoke chain has reached `FinalizeHooksExecuted`.
     *
     *      `spoke_statuses[i].current_step` never returns `Settled` (that value is
     *      `hub_status`-only) — it's `Queued` for a registered chain with no leg step
     *      recorded yet, otherwise the highest SettlementStep value with a recorded TxRecord
     *      for that chain, checked in pipeline order (CollectBridgeExecuted,
     *      CollectHooksExecuted, ResponseBridgeExecuted, ResponseHooksExecuted,
     *      FinalizeBridgeExecuted, FinalizeHooksExecuted) — equivalently, iterate
     *      get_settlement_record's `chains[i]` fields in that same order and take the last
     *      one with `recorded_at != 0`. `current_step == FinalizeHooksExecuted` means that
     *      one chain is fully settled; `hub_status == Settled` means all of them are.
     * @param product_id    The product the settlement belongs to
     * @param settlement_id The settlement to check
     * @return hub_status     `Queued`, `Triggered`, or `Settled` — see dev notes above
     * @return spoke_statuses Per-spoke-chain coarse status, ordered as registered at
     *                        Trigger time; empty if `hub_status == Queued`
     */
    function get_settlement_status(
        uint256 product_id,
        uint256 settlement_id
    )
        external
        view
        returns (
            SettlementStep hub_status,
            SettlementChainStatus[] memory spoke_statuses
        );

    /**
     * @notice Enumerate an investor's currently in-flight requests — those whose registry
     *         entry has been opened (record_request_tx, step == Requested) but whose
     *         origin-chain Finalize leg hasn't landed yet (see get_request_status's
     *         `receivable`).
     * @dev An empty array means the investor has no in-flight request; this is not an error.
     * @param investor The investor address to look up
     * @return requests The investor's in-flight (product_id, request_id) pairs
     */
    function get_investor_active_requests(
        address investor
    ) external view returns (InvestorRequest[] memory requests);

    /**
     * @notice Read one request's status, composed directly from its request registry entry
     *         and (once linked) its settlement's Finalize leg for the request's own
     *         origin chain — no separate coarse-status enum, just the two pipelines' own
     *         vocabulary.
     * @dev Reverts under the same condition as get_request_record (registry entry never
     *      opened). `request_step` is the last completed step of the 3-tx request
     *      pipeline. `settlement_id` is 0 until this request is linked to a settlement via
     *      the Investments precompile's record_investment_approval (which — per the
     *      call-flow design — happens together with that settlement's Response leg, so by
     *      the time settlement_id is non-zero, Collect/Response are already done; only
     *      Finalize can still be pending). Safe as a sentinel because settlement_id is
     *      1-indexed protocol-wide (0 is reserved to mean "no settlement", the first real
     *      settlement is 1) — see the Investments precompile's get_settlement_id.
     *      `receivable` is true once the Finalize leg's Hooks phase is recorded
     *      (get_settlement_record) for `settlement_id` on this request's own origin chain —
     *      escrow share burn + payout-receivable for redeem, or distributeShares for deposit.
     *      It means receivable, not received: the investor's own separate claim() call
     *      (withdraw/redeem or deposit/mint), taken at whatever later time they choose, is
     *      not tracked per-request — see record_receive_tx (ReceiveTxRecorded), which tracks
     *      receives per (investor, vault) instead, since TrancheManager pools receivable
     *      amounts rather than keeping them keyed by request_id. `receivable` is always false
     *      while `settlement_id == 0`.
     * @param product_id The product the request belongs to
     * @param request_id The request to look up
     * @return request_step  Last completed step of the request's own 3-tx pipeline
     * @return settlement_id The settlement this request is linked to, 0 if not yet linked
     * @return receivable    Whether the investor can now call claim() for this request
     */
    function get_request_status(
        uint256 product_id,
        bytes32 request_id
    )
        external
        view
        returns (
            RequestStep request_step,
            uint256 settlement_id,
            bool receivable
        );
}
