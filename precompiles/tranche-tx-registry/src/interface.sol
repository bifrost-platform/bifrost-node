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
 *  - Request (per request_id): a single request_tx (opens the entry), then two independent
 *    kinds of leg — an Inbound leg (Spoke->Hub, only if the vault is on a Spoke chain, at
 *    most one) delivering the request itself to the Hub Valuation Contract, and zero or
 *    more Adapter legs (Hub->Spoke, one per remote actually-weighted MultichainAdapter
 *    chain) pushing the now-arrived capital back out. Each leg is its own bridge_tx/hooks_tx
 *    pair. `adapter_chain_ids` — which chains need an Adapter leg — can only be
 *    declared once the capital has genuinely arrived at the Valuation Contract: immediately,
 *    at `Requested` itself, for a Hub-vault request (no Inbound leg needed); or at the
 *    Inbound leg's own `InboundHooksExecuted`, for a Spoke-vault request. A request needing no
 *    cross-chain action at all (Hub vault, no weighted remote Adapters) declares an empty
 *    set at `Requested` and is immediately Completed.
 *  - Settlement (per settlement_id, fanned out per chain): a single trigger_tx
 *    (Hub-local tryUpdateNAV) that also declares two independent chain sets —
 *    collect_response_chain_ids (chains with a registered Adapter, excluding Hub
 *    itself) and finalize_chain_ids (chains with a registered vault, excluding Hub) —
 *    then, per chain, whichever leg kind(s) its role calls for: Collect (Hub->Spoke NAV
 *    request) + Response (Spoke->Hub NAV report) for a collect_response chain, Finalize
 *    (Hub->Spoke settle result) for a finalize chain, both for a chain with both roles.
 *    Each leg is its own bridge_tx/hooks_tx pair. Legs progress independently per chain;
 *    there is no cross-chain ordering constraint. A settlement needing no cross-chain
 *    action at all is Triggered with both sets empty.
 *  - Receive (per investor per vault): a plain local Spoke-chain claim() tx, not part of
 *    the Bridge&Call pipelines above. TrancheManager pools receivable amounts per
 *    (investor, vault) rather than per request_id, so receives are tracked separately
 *    from the request pipeline (see record_receive_tx's dev notes for why).
 *
 * Every enum value other than `Queued`/`Completed` (RequestStep) and `Queued`/`Settled`
 * (SettlementStep) is directly recordable by the tx recorder backend — those four are
 * read-only sentinels only ever returned by a view function, never valid record_*_tx input.
 * Determining "does this leg exist at all" or "is this chain's leg done yet" is entirely the
 * pallet's job on the read side, not something the recorder declares as a special step.
 *
 * get_request/get_settlement (the only two read functions besides
 * get_investor_active_requests) return each chain's/leg's steps as an ordered array of
 * (step, tx) pairs rather than a fixed-shape struct with one field per possible step: the
 * array only ever contains the steps that actually apply to that particular chain/request
 * (e.g. a collect_response-only settlement chain's array has no Finalize entries at all,
 * and a Hub-vault request's `request_steps` array has no Inbound entries), so a step's
 * absence from the array unambiguously means "does not apply here" — never confusable
 * with "hasn't happened yet," which is instead represented by that step's own
 * `tx.recorded_at == 0` while it's still present in the array. This also means a
 * chain/leg's own completion is always just
 * `steps[steps.length - 1].tx.recorded_at != 0` — no separate "is this one done" field
 * needed per entry.
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
    ///      means "not yet recorded" (used as the unset sentinel by every TxStep still
    ///      present in a get_request/get_settlement steps array — see this interface's
    ///      top-level dev notes on why a step's absence from that array, rather than a
    ///      zeroed TxRecord, is what signals "does not apply").
    struct TxRecord {
        uint64 chain_id;
        bytes32 tx_hash;
        uint256 recorded_at;
    }

    /// @dev One step's evidence within a settlement chain's leg history.
    /// @param step Which SettlementStep this entry is for
    /// @param tx   Evidence for this step — zeroed (recorded_at == 0) iff not yet reached
    struct SettlementTxStep {
        SettlementStep step;
        TxRecord tx;
    }

    /// @dev One chain's full ordered step history within a settlement — see this
    ///      interface's top-level dev notes for the array-shape convention this follows.
    /// @param spoke_chain_id The chain these steps are for
    /// @param steps          Exactly `[CollectBridgeExecuted, CollectHooksExecuted,
    ///                       ResponseBridgeExecuted, ResponseHooksExecuted]` if this chain
    ///                       only has a registered Adapter, `[FinalizeBridgeExecuted,
    ///                       FinalizeHooksExecuted]` if it only has a registered vault, or
    ///                       all six (Collect/Response then Finalize) if it has both. This
    ///                       chain is fully done iff `steps[steps.length - 1].tx.recorded_at
    ///                       != 0`.
    struct SettlementChainSteps {
        uint64 spoke_chain_id;
        SettlementTxStep[] steps;
    }

    /// @dev One step's evidence within a request's Inbound or Adapter leg history.
    /// @param step Which RequestStep this entry is for
    /// @param tx   Evidence for this step — zeroed (recorded_at == 0) iff not yet reached
    struct RequestTxStep {
        RequestStep step;
        TxRecord tx;
    }

    /// @dev One chain's Adapter leg — always exactly `[AdapterBridgeExecuted,
    ///      AdapterHooksExecuted]`, since every declared Adapter chain needs the same two
    ///      steps. This leg is done iff `steps[1].tx.recorded_at != 0`.
    /// @param chain_id The chain this leg is for
    /// @param steps    `[AdapterBridgeExecuted, AdapterHooksExecuted]`
    struct AdapterLeg {
        uint64 chain_id;
        RequestTxStep[] steps;
    }

    /// @dev A request's static details — unchanged since record_request_tx's Requested
    ///      step, so returned as one bundle rather than four separate values.
    /// @param investor   Investor address the registry entry was opened with
    /// @param vault      The tranche vault this request targets
    /// @param amount     Investor's full requested amount, as submitted at Requested step
    /// @param order_type 0 = redeem, 1 = deposit
    struct RequestInfo {
        address investor;
        VaultInput vault;
        uint256 amount;
        uint8 order_type;
    }

    /// @dev One of an investor's in-flight requests.
    /// @param product_id Product the request belongs to
    /// @param request_id The request itself
    struct InvestorRequest {
        uint256 product_id;
        bytes32 request_id;
    }

    /// @dev Step within a request's pipeline. `Requested`/`InboundBridgeExecuted`/
    ///      `InboundHooksExecuted`/`AdapterBridgeExecuted`/`AdapterHooksExecuted`
    ///      are the only five values record_request_tx ever accepts as input. Fragmented
    ///      into two directions, since they're causally sequenced and not interchangeable:
    ///
    ///      - `Requested`, then (only if the vault is on a Spoke chain)
    ///        `InboundBridgeExecuted`/`InboundHooksExecuted` — the *Inbound* leg, delivering
    ///        the request itself from the investor's own vault chain to the Hub Valuation
    ///        Contract. At most one such leg per request. Never recorded at all for a
    ///        Hub-vault request (`Requested` already means the deposit is at the Valuation
    ///        Contract in that same call).
    ///      - `AdapterBridgeExecuted`/`AdapterHooksExecuted`, once per declared
    ///        chain — the *Adapter* leg(s), the Hub Valuation Contract pushing the
    ///        now-arrived capital back out to each remote, actually-weighted
    ///        MultichainAdapter chain (see record_request_tx's dev notes for exactly which
    ///        chains belong in that set). Keyed by the call's own `attestation.chain_id`,
    ///        since a request can need more than one such leg at once.
    ///
    ///      `adapter_chain_ids` — the full set of chains needing an Adapter leg —
    ///      can only be declared once the Hub Valuation Contract has actually decided it,
    ///      which happens at whichever step first represents "this request's capital has
    ///      arrived at the Valuation Contract": `Requested` itself for a Hub-vault request,
    ///      or the Inbound leg's own `InboundHooksExecuted` for a Spoke-vault request (the
    ///      Valuation Contract emits its own event declaring the chains right when that
    ///      Hooks call lands, for the recorder to observe and attach here) — never at
    ///      `Requested` for a Spoke-vault request, since the adapter decision genuinely
    ///      isn't known that early.
    ///
    ///      `Queued` and `Completed` are read-only sentinels, never valid record_request_tx
    ///      input — mirrors SettlementStep's `Queued`/`Settled`, and (unlike an earlier
    ///      version of this interface) no longer appear anywhere in get_request's return
    ///      shape either, since a step's mere presence/absence in a steps array plus its own
    ///      `tx.recorded_at` already convey everything `Queued` used to.
    enum RequestStep {
        Queued,
        Requested,
        InboundBridgeExecuted,
        InboundHooksExecuted,
        AdapterBridgeExecuted,
        AdapterHooksExecuted,
        Completed
    }

    /// @dev Step within the settlement pipeline. `Queued`/`Triggered`/`Settled` are the
    ///      three overall states get_settlement's own `status` moves through —
    ///      `Queued` (not yet triggered), `Triggered` (triggered, awaiting completion),
    ///      `Settled` (every chain has reached *its own* terminal step — see below). The
    ///      middle six describe one chain's own progress instead — a
    ///      Collect/Response/Finalize leg crossed with a Bridge/Hooks phase (each suffixed
    ///      `Executed`, since by the time any of these six is recorded, that half of the leg
    ///      has already landed) — flattened into this same enum so record_settlement_tx
    ///      takes a single step argument. `Queued` and `Settled` are read-only sentinels —
    ///      "nothing recorded yet" and "settlement fully complete", respectively — and must
    ///      never be passed to record_settlement_tx (seven recordable values in total:
    ///      `Triggered` plus the six leg steps).
    ///
    ///      Each chain's own terminal step depends on its role, declared at Trigger time
    ///      (see record_settlement_tx's dev notes): `FinalizeHooksExecuted` if it's in
    ///      `finalize_chain_ids` (it has a vault to deliver a result to), otherwise
    ///      `ResponseHooksExecuted` (it only has an Adapter — Collect/Response-only chains
    ///      never get a Finalize leg to begin with, and never have Finalize entries in
    ///      get_settlement's `steps` array at all). get_settlement's `status == Settled`
    ///      once every chain has reached its own terminal step.
    ///
    ///      A settlement that needs no cross-chain action at all is represented by
    ///      `Triggered` with *both* `collect_response_chain_ids` and `finalize_chain_ids`
    ///      empty — no separate step value needed: `Triggered` no longer requires either
    ///      set to be non-empty, so both empty alone unambiguously means "nothing to
    ///      collect/respond/finalize" via vacuous truth (get_settlement's `spoke_chains`
    ///      array comes back empty, and `status` is `Settled` immediately).
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
    ///      convention as record_request_tx's own parameters. `adapter_chain_ids` is
    ///      meaningful (and may be empty) when `step == Requested` (Hub-vault request) or
    ///      `step == InboundHooksExecuted` (Spoke-vault request's Inbound leg), empty otherwise.
    ///      Deliberately no `settlement_id` field here — see record_request_tx's dev notes
    ///      for why.
    event RequestTxRecorded(
        uint256 indexed product_id,
        bytes32 indexed request_id,
        address indexed investor,
        uint64 vault_chain_id,
        address vault_address,
        uint256 amount,
        uint8 order_type,
        RequestStep step,
        uint64[] adapter_chain_ids,
        TxAttestation attestation
    );

    /// @dev `spoke_chain_id` is 0 only when `step == Triggered` (`collect_response_chain_ids`/
    ///      `finalize_chain_ids` are then meaningful, each independently possibly empty);
    ///      otherwise `spoke_chain_id` identifies the chain and both arrays are empty — same
    ///      sentinel convention as record_settlement_tx's own parameters. `settlement_id` is
    ///      only unique within `product_id`'s own namespace (each product's Valuation
    ///      Contract generates its own sequence), never globally.
    event SettlementTxRecorded(
        uint256 indexed product_id,
        uint256 indexed settlement_id,
        uint64 indexed spoke_chain_id,
        SettlementStep step,
        uint64[] collect_response_chain_ids,
        uint64[] finalize_chain_ids,
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
     * @notice Attest to one tx in a request's pipeline — the single Requested tx, one
     *         Bridge/Hooks half of the Inbound leg (Spoke-vault requests only), or one
     *         Bridge/Hooks half of a per-chain Adapter leg.
     * @dev Only callable by the pallet-registered tx recorder account. `investor`/
     *      `vault_chain_id`/`vault_address`/`amount`/`order_type` are sentinel-gated
     *      together: they MUST all be non-zero/non-empty when `step == Requested` (opens a
     *      fresh registry entry) and MUST all be zero/empty for every other step (rejected
     *      otherwise, to catch caller bugs early rather than silently ignoring a stray
     *      value). Deliberately no `settlement_id` parameter — it isn't actually knowable at
     *      request_tx time: it's assigned by the Valuation Contract only once the request
     *      lands on the Hub and record_investment_request runs, and the Spoke-side bridge
     *      message itself carries no settlement_id field for the recorder to observe earlier
     *      than that.
     *      `adapter_chain_ids` declares every chain (besides Hub) this request's
     *      capital will need its own Adapter leg for — every remote chain with a
     *      MultichainAdapter the deposited capital gets distributed to, but only those
     *      currently weighted (non-zero weightBps) at the moment the Valuation Contract
     *      actually processes the request; a remote Adapter weighted to 0 receives no
     *      allocation, so it doesn't force a leg. It's meaningful (and may be empty) at
     *      exactly one of two steps, depending on whether the vault is on Hub or Spoke — see
     *      RequestStep's dev notes:
     *      - `step == Requested`, for a Hub-vault request (no Inbound leg — the deposit is
     *        already at the Valuation Contract, so the decision is knowable immediately).
     *        MUST be omitted (empty) at `Requested` for a Spoke-vault request instead.
     *      - `step == InboundHooksExecuted`, for a Spoke-vault request (the Inbound leg's own
     *        arrival at the Valuation Contract — not knowable any earlier). Never valid for
     *        a Hub-vault request, which has no Inbound leg to begin with.
     *      An empty set means the request needs no Adapter at all — once its Inbound
     *      leg (if any) is done, it's immediately Completed.
     *      Each declared Adapter chain then needs exactly one
     *      AdapterBridgeExecuted then AdapterHooksExecuted call, identified by
     *      `attestation.chain_id` — no ordering constraint across different chains, only
     *      within one chain's own Bridge-then-Hooks pair, and only after that request's
     *      Adapter chains have actually been declared. Likewise the Inbound leg's own
     *      Bridge must precede its Hooks. `step == Requested` must not be called twice for
     *      the same request_id, and InboundBridgeExecuted/InboundHooksExecuted revert if
     *      called for a Hub-vault request.
     *      Opening a registry entry registers (product_id, request_id) under the investor for
     *      get_investor_active_requests; this registration is independent of, and can
     *      precede, the Investments precompile's record_investment_request (which is only
     *      callable once the request has actually landed on the Hub).
     *      Emits RequestTxRecorded.
     * @param product_id             The product this request belongs to
     * @param request_id             The request this entry is for
     * @param investor               Investor address — required iff step == Requested
     * @param vault_chain_id         EVM chain ID of the tranche vault — required iff
     *                               step == Requested
     * @param vault_address          ERC-7540 vault contract address — required iff
     *                               step == Requested
     * @param amount                 Investor's full requested amount — required iff
     *                               step == Requested
     * @param order_type             0 = redeem, 1 = deposit — meaningful iff step == Requested
     * @param adapter_chain_ids Every chain (besides Hub) needing its own Adapter
     *                               leg — meaningful (and may be empty) iff step == Requested
     *                               (Hub-vault only) or step == InboundHooksExecuted
     *                               (Spoke-vault only), empty otherwise
     * @param step                   Which pipeline step this attestation is for
     * @param attestation            The attested off-chain tx
     */
    function record_request_tx(
        uint256 product_id,
        bytes32 request_id,
        address investor,
        uint64 vault_chain_id,
        address vault_address,
        uint256 amount,
        uint8 order_type,
        uint64[] calldata adapter_chain_ids,
        RequestStep step,
        TxAttestation calldata attestation
    ) external;

    /**
     * @notice Attest to one tx in a settlement's pipeline: either the single Trigger tx, or
     *         one bridge/hooks half of a Collect/Response/Finalize leg for one chain.
     * @dev Only callable by the pallet-registered tx recorder account. `step` MUST NOT be
     *      `SettlementStep.Queued` or `SettlementStep.Settled` — both are read-only
     *      sentinels reserved for get_settlement's own `status`, never a
     *      valid attestation to record. `settlement_id` is only unique within
     *      `product_id`'s own namespace — each product's Valuation
     *      Contract generates its own sequence, same scoping as every settlement_id use in
     *      the Investments precompile (e.g. record_tranche_settlement) — so all storage
     *      here is keyed by (product_id, settlement_id), never settlement_id alone.
     *      `spoke_chain_id` and the two chain-set params are sentinel-gated, mirroring
     *      record_request_tx: for `step == Triggered`, `spoke_chain_id` MUST be 0 and both
     *      `collect_response_chain_ids`/`finalize_chain_ids` are meaningful — each may
     *      independently be empty, and both empty means the settlement needs no cross-chain
     *      action at all; for every leg step, `spoke_chain_id` MUST be non-zero and both
     *      chain-set params MUST be empty. Safe as a sentinel because no EVM chain in this
     *      protocol's supported set is ever assigned chain_id 0. `spoke_chain_id` identifies
     *      which chain a leg step is about — not necessarily the chain `attestation.chain_id`
     *      itself executed on, since e.g. the Response leg's bridge_tx/hooks_tx both
     *      physically execute on the Hub (see get_settlement's dev notes) even though
     *      `spoke_chain_id` there still names the chain being responded for.
     *      `collect_response_chain_ids` declares every chain (excluding Hub) with a
     *      registered Adapter this settlement queries NAV from — a CollectBridgeExecuted/
     *      CollectHooksExecuted/ResponseBridgeExecuted/ResponseHooksExecuted call is only
     *      valid for a chain in this set. `finalize_chain_ids` declares every chain
     *      (excluding Hub) with a registered vault this settlement delivers a result to — a
     *      FinalizeBridgeExecuted/FinalizeHooksExecuted call is only valid for a chain in
     *      this set. A chain may appear in both (it has both a registered Adapter and a
     *      registered vault) or just one. `step == Triggered` must be recorded exactly once
     *      per (product_id, settlement_id), before any leg step for that settlement. Within
     *      a given (spoke_chain_id, leg) pair, Bridge must be recorded before Hooks, with no
     *      duplicates — this ordering is enforced only within that pair, not across chains
     *      or legs, since chains progress independently.
     *      Emits SettlementTxRecorded.
     * @param product_id     The product this settlement belongs to
     * @param settlement_id  The settlement cycle this attestation belongs to
     * @param spoke_chain_id  The chain this leg step is for — 0 if step == Triggered
     * @param collect_response_chain_ids Chains needing a Collect/Response leg — meaningful
     *                        (and may be empty) iff step == Triggered, empty otherwise
     * @param finalize_chain_ids Chains needing a Finalize leg — meaningful (and may be
     *                        empty) iff step == Triggered, empty otherwise
     * @param step            Which pipeline step this attestation is for
     * @param attestation     The attested off-chain tx
     */
    function record_settlement_tx(
        uint256 product_id,
        uint256 settlement_id,
        uint64 spoke_chain_id,
        uint64[] calldata collect_response_chain_ids,
        uint64[] calldata finalize_chain_ids,
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
     *      per-request_id link here — once a request becomes `settled` (see get_request),
     *      tracking "was THIS request specifically claimed" stops being meaningful, since a
     *      single claim() may drain a pooled balance spanning several distributed requests
     *      at once. No dedicated getter yet — ReceiveTxRecorded is the only way to observe
     *      receives for now.
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
     * @notice Read a settlement's full state in one call: Trigger evidence, the
     *         settlement's own overall status, and every registered chain's ordered
     *         step-by-step history.
     * @dev Does not revert for an untriggered (product_id, settlement_id) — returns a
     *      zeroed `trigger_tx`, `status == Queued`, and empty `spoke_chains` instead, so
     *      callers can poll a not-yet-started settlement_id without a revert.
     *      Chain IDs are not returned separately — read them off
     *      `spoke_chains[i].spoke_chain_id`. `spoke_chains` is the union of
     *      `collect_response_chain_ids` and `finalize_chain_ids` declared at Trigger time
     *      (collect_response-declared chains first, then any finalize-only chains not
     *      already included; empty if Triggered with no cross-chain action needed, or if
     *      not yet triggered at all).
     *      `spoke_chains[i].steps` only contains the step kinds that chain's role actually
     *      needs — see SettlementChainSteps' own dev notes — so a step's absence there means
     *      "does not apply," never "not yet reached"; within the array, each entry's own
     *      `tx.recorded_at == 0` means "not yet reached." There is no separate on-chain
     *      aggregate beyond `status`/each chain's own last-step check (Valuation tracks its
     *      own completion condition internally; this call exists purely for external
     *      registry visibility).
     *      `status` only ever takes one of three values:
     *      - `Queued` — Trigger not yet recorded (`trigger_tx` is then zeroed too).
     *      - `Triggered` — Trigger recorded, but at least one chain hasn't yet reached its
     *        own terminal step (last entry in its `steps` array).
     *      - `Settled` — every chain has reached its own terminal step (vacuously true, and
     *        immediate, if Triggered with both chain sets empty, i.e. `spoke_chains` is
     *        empty). `trigger_tx` itself never changes once Triggered — only `status` moves
     *        from `Triggered` to `Settled` as chains complete.
     * @param product_id    The product the settlement belongs to
     * @param settlement_id The settlement to look up
     * @return trigger_tx   Evidence for the Trigger step
     * @return status       The settlement's own overall status — `Queued`/`Triggered`/`Settled`
     * @return spoke_chains Per-chain ordered step history, see above
     */
    function get_settlement(
        uint256 product_id,
        uint256 settlement_id
    )
        external
        view
        returns (
            TxRecord memory trigger_tx,
            SettlementStep status,
            SettlementChainSteps[] memory spoke_chains
        );

    /**
     * @notice Enumerate an investor's currently in-flight requests — those whose registry
     *         entry has been opened (record_request_tx, step == Requested) but whose
     *         settlement hasn't fully completed yet (see get_request's `settled`).
     * @dev An empty array means the investor has no in-flight request; this is not an error.
     * @param investor The investor address to look up
     * @return requests The investor's in-flight (product_id, request_id) pairs
     */
    function get_investor_active_requests(
        address investor
    ) external view returns (InvestorRequest[] memory requests);

    /**
     * @notice Read a request's full state in one call: its static details (bundled as one
     *         `RequestInfo`), the Requested/Inbound-leg evidence (bundled as one ordered
     *         `request_steps` array, same "step, tx" shape as every per-chain leg entry),
     *         every declared Adapter chain's ordered step-by-step history, the request's
     *         own overall `status`, and — separately — `settled`, composed from its linked
     *         settlement's own completion for this request's origin chain.
     * @dev Reverts if record_request_tx has never been called with step == Requested for
     *      this request_id.
     *      `request_steps[0]` is always `(Requested, request_tx)` — every request that
     *      exists has one, unconditionally. For a Hub-vault request (no Inbound leg
     *      applies at all — `info.vault.chain_id` equals this chain's own EVM chain ID),
     *      that's the array's only entry; for a Spoke-vault one, two more entries follow:
     *      `(InboundBridgeExecuted, ...)` then `(InboundHooksExecuted, ...)` — same "absent
     *      means not applicable" convention as get_settlement's `steps` arrays, so check
     *      `request_steps.length` (1 vs 3) to tell whether this request has an Inbound leg
     *      at all. Each `adapter_legs[i].steps` is always exactly `[AdapterBridgeExecuted,
     *      AdapterHooksExecuted]`, ordered as declared (at `Requested` for a Hub-vault
     *      request, at the Inbound leg's own `InboundHooksExecuted` for a Spoke-vault one;
     *      empty if none were declared, yet or ever).
     *      `status` only ever takes `Requested` (Inbound leg, if any, or some
     *      Adapter leg still has an unfinished step) or `Completed` (Inbound leg, if any,
     *      done, and every declared Adapter chain's last step landed, or none were declared
     *      at all — immediate for a fully local request).
     *      Unlike every other function here, this also reads
     *      `pallet-tranche-investments::ApprovedInvestments` directly to resolve
     *      `settlement_id`/`settled` — `pallet_tranche_tx_registry` the pallet deliberately
     *      has no dependency on `pallet-tranche-investments` (see this pallet's module docs
     *      on why the two precompiles were split apart), but that decoupling is a
     *      pallet-level concern, not a precompile-level one: this precompile crate is the
     *      per-runtime aggregation layer, and `TrancheInvestmentsPrecompile` itself already
     *      sets the precedent of reading `pallet-tranche-system`'s storage directly despite
     *      `pallet-tranche-investments` not depending on that pallet either.
     *      `settlement_id` is 0 until this request is linked to a settlement via the
     *      Investments precompile's record_investment_approval (which — per the call-flow
     *      design — happens together with that settlement's Response leg, so by the time
     *      settlement_id is non-zero, Collect/Response are already done; only Finalize can
     *      still be pending). Safe as a sentinel because settlement_id is 1-indexed
     *      protocol-wide (0 is reserved to mean "no settlement", the first real settlement
     *      is 1) — see the Investments precompile's get_settlement_id.
     *      `settled` depends on whether this request's own vault is on Hub or Spoke,
     *      mirroring the Inbound-leg asymmetry above (a Hub-vault request has no Finalize
     *      leg of its own to wait on):
     *      - Spoke vault: true once the Finalize leg's Hooks phase is recorded
     *        (get_settlement) for `settlement_id` on this request's own origin chain —
     *        escrow share burn + payout-receivable for redeem, or distributeShares for
     *        deposit.
     *      - Hub vault: true once every one of `settlement_id`'s `collect_response_chain_ids`
     *        has reached `ResponseHooksExecuted` (vacuously true, and immediate, if that set
     *        was declared empty at Trigger) — NAV must be fully known before the Hub vault's
     *        own payout/allocation can be computed, which then happens synchronously with no
     *        Finalize leg of its own.
     *      Named `settled` rather than `receivable` to read naturally against
     *      `settlement_id`: non-zero `settlement_id` + `settled == false` means still being
     *      settled; non-zero `settlement_id` + `settled == true` means fully settled. It
     *      does not mean received: the investor's own separate claim() call (withdraw/redeem
     *      or deposit/mint), taken at whatever later time they choose, is not tracked
     *      per-request — see record_receive_tx (ReceiveTxRecorded), which tracks receives
     *      per (investor, vault) instead, since TrancheManager pools receivable amounts
     *      rather than keeping them keyed by request_id. `settled` is always false while
     *      `settlement_id == 0`, and is entirely independent of
     *      `status`/`adapter_legs` — a request's own delivery to the Hub and its
     *      linked settlement's delivery of results back out are two separate concerns
     *      tracked here.
     * @param product_id The product the request belongs to
     * @param request_id The request to look up
     * @return info           Investor/vault/amount/order_type, unchanged since Requested
     * @return request_steps  Ordered Requested + Inbound-leg step history, see above
     * @return adapter_legs   Per-chain ordered Adapter-leg step history, see above
     * @return status         `Requested` or `Completed` — see dev notes above
     * @return settlement_id  The settlement this request is linked to, 0 if not yet linked
     * @return settled        Whether this request's settlement has fully completed (the
     *                        investor can now call claim() for it, though that call itself
     *                        isn't tracked here — see record_receive_tx)
     */
    function get_request(
        uint256 product_id,
        bytes32 request_id
    )
        external
        view
        returns (
            RequestInfo memory info,
            RequestTxStep[] memory request_steps,
            AdapterLeg[] memory adapter_legs,
            RequestStep status,
            uint256 settlement_id,
            bool settled
        );
}
