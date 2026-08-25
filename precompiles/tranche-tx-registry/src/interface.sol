// SPDX-License-Identifier: GPL-3.0-only
pragma solidity >=0.8.0;

/**
 * @title Tranche Tx Registry Precompile Interface (tranche-system draft)
 * @notice Off-chain tx registry for the tranche-system's deposit/redeem request pipeline,
 *         settlement pipeline, and vault receives. CCCP-v2 is a Bridge&Call protocol: every
 *         cross-chain message costs two on-chain tx — a bridge-vote tx (relayers submit
 *         ⅔+ signatures, emitting SocketMessage.status = Executed) followed by a separate
 *         tx that calls Hooks.execute() on the destination chain to actually run the
 *         payload. Neither of those tx (nor a vault's local receive() tx) touches the
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
 *  - Request (per request_id): a single request_tx (opens the entry), a single queued_tx
 *    (RequestQueued — the request's capital confirmed at the Hub Valuation Contract, Hub-vault
 *    or Spoke-vault alike), plus two independent kinds of leg — an Inbound leg (Spoke->Hub,
 *    only if the vault is on a Spoke chain, at most one, Bridge phase only — its own Hooks
 *    phase is what RequestQueued represents) delivering the request itself to the Hub
 *    Valuation Contract, and zero or more Adapter legs (one per actually-weighted
 *    MultichainAdapter chain) pushing capital out to each. Each leg is its own
 *    Bridge-attempt-history/applied_tx (or Bridge-attempt-history/queued_tx) pair — see
 *    BridgeAttempt's own dev notes for why the Bridge half is a full attempt history rather
 *    than a single tx — except when an Adapter leg's chain is
 *    Hub itself, or the request's own origin vault chain for a Redeem specifically: that
 *    allocation is fulfilled synchronously (no Bridge phase at all — the Adapter Contract
 *    call is local), so only applied_tx is ever set for it. A Redeem collects its share out
 *    of the origin chain's own Adapter at request time, before anything reaches the Hub, so
 *    it never needs a Bridge phase there either way. A Deposit's Adapter leg on a Spoke chain
 *    always needs a real Bridge phase instead, even one that happens to be the origin vault's
 *    own chain — a Deposit's allocation decision is only made once capital reaches the
 *    Valuation Contract, so unlike a Redeem it can't be pre-applied locally at the origin
 *    chain (2026-08-21 change — see request-flow.md's changelog). `adapter_chain_ids` — which
 *    chains need an Adapter leg — is normally declared at RequestQueued, once the capital has
 *    genuinely arrived at the Valuation Contract (never at Requested itself), but a
 *    self-fulfilling chain's AdapterApplied evidence can be recorded before RequestQueued ever
 *    runs (its Adapter Contract call happens locally, in the same tx as — and possibly logged before — the
 *    domain event that would otherwise declare it); record_request_tx accepts
 *    AdapterBridgeExecuted/AdapterApplied calls in whatever order the recorder actually
 *    observed the underlying events, self-declaring a not-yet-seen chain rather than requiring
 *    RequestQueued to have listed it first. A request needing no cross-chain action at all
 *    (Hub vault, no weighted remote Adapters) declares an empty set at RequestQueued and is
 *    immediately RequestCompleted. Separately, once Valuation's DepositsApproved/RedeemsApproved
 *    event (fired once per settlement, batching every request it approves) links a batch of
 *    requests to a settlement_id, a single record_settlement_tx RequestsApproved call records
 *    that linkage for the whole batch — deliberately named apart from RequestCompleted, which
 *    is about a request's own delivery pipeline finishing, not whether its settlement has (see
 *    SettlementStep's dev notes).
 *  - Settlement (per settlement_id, fanned out per chain): a single settle_started_tx
 *    (Hub-local tryUpdateNAV) that also declares two independent chain sets —
 *    collect_response_chain_ids (chains with a registered Adapter, excluding Hub
 *    itself) and finalize_chain_ids (chains with a registered vault, excluding Hub) —
 *    then, per chain, whichever leg kind(s) its role calls for: Collect (Hub->Spoke NAV
 *    request) + Response (Spoke->Hub NAV report) for a collect_response chain, Finalize
 *    (Hub->Spoke settle result) for a finalize chain, both for a chain with both roles.
 *    Each leg is its own Bridge+Hooks tx pair (the Hooks-phase evidence named after its
 *    own Contract event — NavReported/NavReceived/SettleApplied). Legs progress independently
 *    per chain;
 *    there is no cross-chain ordering constraint. A settlement needing no cross-chain
 *    action at all is SettleStarted with both sets empty.
 *  - Receive (per investor per vault): a plain local Spoke-chain receive() tx, not part of
 *    the Bridge&Call pipelines above. TrancheManager pools receivable amounts per
 *    (investor, vault) rather than per request_id, so receives are tracked separately
 *    from the request pipeline (see record_receive_tx's dev notes for why).
 *  - Whitelist (per vault per who per nonce, added 2026-08-14): a single trigger_tx
 *    (Orchestrator's WhitelistRequested, Hub) then either just an Applied tx (Hub-vault
 *    — TrancheManager applies the grant/revoke locally, no bridging needed) or a
 *    Bridge+Applied pair (Spoke-vault — TrancheManager's WhitelistApplied), same
 *    Hub-vault/Spoke-vault branching as Request. No `product_id` input on
 *    record_whitelist_tx — resolved internally from `vault` (see that function's dev
 *    notes). Unlike Request/Settlement/Receive, this pipeline keeps no on-chain
 *    history array — only the latest nonce per (vault, who) is tracked
 *    (get_latest_whitelist_nonce), since nonces increment per (product, chain) and a
 *    past one is still directly readable via get_whitelist by anyone who already has
 *    it (e.g. from watching WhitelistTxRecorded); full history browsing is left to an
 *    off-chain indexer.
 *
 * Every enum value other than `None`/`RequestCompleted` (RequestStep), `Queued`/`Settled`
 * (SettlementStep), and `None` (WhitelistStep) is directly recordable by the tx recorder
 * backend — those five are read-only sentinels only ever returned by a view function,
 * never valid record_*_tx input.
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
 * Split out of the Investments precompile (2026-08-06) once the tx-tracing
 * functionality outgrew it: different trust model (a single bot-driven recorder account
 * vs. the product's Valuation Contract), different write volume (expected far more
 * frequent than Investments' own ledger calls), and potential reuse by other CCCP-v2
 * Bridge&Call flows beyond tranche-investments.
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
    /// @param steps          Exactly `[CollectBridgeExecuted, NavReported,
    ///                       ResponseBridgeExecuted, NavReceived]` if this chain
    ///                       only has a registered Adapter, `[FinalizeBridgeExecuted,
    ///                       SettleApplied]` if it only has a registered vault, or
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

    /// @dev One step's evidence within a whitelist grant/revoke action's history.
    /// @param step Which WhitelistStep this entry is for
    /// @param tx   Evidence for this step — zeroed (recorded_at == 0) iff not yet reached
    struct WhitelistTxStep {
        WhitelistStep step;
        TxRecord tx;
    }

    /// @dev One chain's Adapter leg. `steps` is `[AdapterBridgeExecuted,
    ///      AdapterApplied]` (length 2) for a Deposit's Adapter leg on a Spoke chain —
    ///      even one that happens to be the request's own origin vault chain, since a
    ///      Deposit's allocation is only decided once capital reaches the Hub, so even
    ///      the origin chain's own share routes back out over a real Bridge phase —
    ///      this leg is done iff `steps[1].tx.recorded_at != 0`. For Hub itself (any
    ///      order type), or for a Redeem whose Adapter chain is the origin vault's own
    ///      chain (that share is collected out of the same chain's own Adapter at
    ///      request time, before anything reaches the Hub), the allocation is
    ///      fulfilled synchronously (no Bridge phase at all — see
    ///      record_request_tx's dev notes), so `steps` is just `[AdapterApplied]`
    ///      (length 1) instead; this leg is done iff `steps[0].tx.recorded_at != 0`.
    ///      Always check `steps.length` before indexing — it is NOT fixed the way
    ///      SettlementTxStep-family arrays with a truly constant shape are.
    /// @param chain_id The chain this leg is for
    /// @param steps    `[AdapterBridgeExecuted, AdapterApplied]` (Deposit, Spoke chain) or
    ///                 `[AdapterApplied]` (Hub, or a Redeem's own origin chain — self-fulfilling)
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
        uint64 product_id;
        bytes32 request_id;
    }

    /// @dev One entry in an investor's receive() history — see
    ///      get_investor_receive_history. Together with the investor/product_id already
    ///      passed to that call, this pair is exactly what ReceiveEntries is keyed by.
    /// @param vault   The vault this receive() call was against
    /// @param tx_hash The receive() tx's hash on that vault's chain
    struct ReceiveHistoryEntry {
        VaultInput vault;
        bytes32 tx_hash;
    }

    /// @dev Step within a request's pipeline. `Requested`/`RequestBridgeExecuted`/
    ///      `RequestQueued`/`AdapterBridgeExecuted`/`AdapterApplied` are the only five
    ///      values record_request_tx ever accepts as input — a request's link to a
    ///      settlement is recorded elsewhere entirely now, via
    ///      record_settlement_tx's `RequestsApproved` step (see SettlementStep's dev
    ///      notes for why). Named after the underlying Valuation Contract events
    ///      wherever one exists 1:1 — `Requested` (DepositRequested/RedeemRequested, at
    ///      TrancheManager), `RequestQueued` (DepositQueued/RedeemQueued, at Valuation),
    ///      `AdapterApplied` (Supplied/WithdrawRequested, at the MultichainAdapter) — so
    ///      a recorder can map "which event did I just see" to "which step do I record"
    ///      without needing to special-case Hub-vault vs Spoke-vault:
    ///
    ///      - `Requested` — the investor's request lands at TrancheManager, wherever the
    ///        vault is (Hub or Spoke). Opens the registry entry, never carries
    ///        `adapter_chain_ids` (not knowable yet, even for a Hub-vault request —
    ///        that's `RequestQueued`'s job, one step later).
    ///      - (only if the vault is on a Spoke chain) `RequestBridgeExecuted` — the
    ///        *Inbound* leg's Bridge phase, Spoke -> Hub. At most one per request. Never
    ///        recorded for a Hub-vault request — there's nothing to bridge when the vault
    ///        is already on Hub.
    ///      - `RequestQueued` — the moment the request's capital is confirmed at the Hub
    ///        Valuation Contract and `adapter_chain_ids` becomes known, for **both**
    ///        Hub-vault and Spoke-vault requests alike. For a Hub-vault request this
    ///        typically lands in the very same transaction as `Requested` (TrancheManager
    ///        -> Valuation is a synchronous local call) but is still a separate
    ///        record_request_tx call — for a Spoke-vault request it's the Inbound leg's
    ///        own Hooks phase, only reachable once `RequestBridgeExecuted` has landed.
    ///      - `AdapterBridgeExecuted`/`AdapterApplied`, once per declared chain — the
    ///        *Adapter* leg(s), the Hub Valuation Contract pushing the now-arrived capital
    ///        back out to each remote, actually-weighted MultichainAdapter chain (see
    ///        record_request_tx's dev notes for exactly which chains belong in that set).
    ///        Keyed by the call's own `attestation.chain_id`, since a request can need
    ///        more than one such leg at once. `AdapterApplied` covers both directions —
    ///        `Supplied` for a deposit, `WithdrawRequested` for a redeem — since both mean
    ///        the same thing structurally: the Adapter has been notified and acted on this
    ///        leg.
    ///
    ///      `adapter_chain_ids` is only ever supplied at `RequestQueued` — never at
    ///      `Requested`, regardless of Hub or Spoke.
    ///
    ///      `None` and `RequestCompleted` are read-only sentinels, never valid
    ///      record_request_tx input. Unlike SettlementStep.Queued (which get_settlement
    ///      genuinely returns for a not-yet-started settlement), `None` is never actually
    ///      returned by get_request either — get_request reverts outright for a request_id
    ///      that was never opened, so there's no "not yet requested" state to report. Named
    ///      `None` rather than `Queued` specifically to avoid sitting next to
    ///      `RequestQueued` under a near-identical name while meaning something completely
    ///      different. `RequestCompleted` (renamed from `Completed`) is get_request's own
    ///      `status` value once `RequestQueued` has landed and every declared Adapter
    ///      chain's last step has too — it describes the request's own delivery pipeline
    ///      finishing, and is deliberately unrelated to whether the settlement it's linked
    ///      to (see SettlementStep.RequestsApproved) has itself settled; get_request's
    ///      separate `settled` return value answers that second question.
    enum RequestStep {
        None,
        Requested,
        RequestBridgeExecuted,
        RequestQueued,
        AdapterBridgeExecuted,
        AdapterApplied,
        RequestCompleted
    }

    /// @dev Step within the settlement pipeline. `Queued`/`SettleStarted`/`Settled` are the
    ///      three overall states get_settlement's own `status` moves through —
    ///      `Queued` (not yet started settling), `SettleStarted` (started, awaiting completion),
    ///      `Settled` (every chain has reached *its own* terminal step — see below). The
    ///      middle six describe one chain's own progress instead — a
    ///      Collect/Response/Finalize leg crossed with a Bridge/Hooks phase, flattened into
    ///      this same enum so record_settlement_tx takes a single step argument. The three
    ///      Bridge-phase values are suffixed `Executed` (generic Socket-message evidence,
    ///      nothing more specific to name them after); the three Hooks-phase values are
    ///      instead named after the underlying Contract event each one's evidence actually
    ///      is — `NavReported` (TrancheManager, Collect leg), `NavReceived` (Valuation,
    ///      Response leg), `SettleApplied` (TrancheManager, Finalize leg) — same
    ///      event-name-mirroring convention as RequestStep's `RequestQueued`/`AdapterApplied`.
    ///      `Queued` is the one read-only sentinel — "nothing recorded yet" — and must
    ///      never be passed to record_settlement_tx (nine recordable values in total:
    ///      `SettleStarted`, `RequestsApproved`, the six leg steps, and `Settled` itself
    ///      in the one narrow case described below).
    ///
    ///      `Settled` is a *computed* status for every Multichain settlement — "settlement
    ///      fully complete" — never itself a valid record_settlement_tx input there. The
    ///      one exception is a `SingleChain` product's settlement (reverts otherwise):
    ///      its SYNC (or manually-settled) Valuation Contract never emits a separate
    ///      SettleStarted — it emits `Settled` as the pipeline's only event, request and
    ///      settlement completing in the same tx. Recording that observed event as
    ///      `SettleStarted`, as if a genuine SettleStarted had actually fired, would break
    ///      this enum's own event-name-mirroring convention for exactly the one case where
    ///      the recorder's evidence tx and the pipeline's terminal status happen to
    ///      coincide — so record_settlement_tx accepts `Settled` directly instead, with
    ///      the exact same effect `SettleStarted` (both chain sets empty) already has.
    ///      `spoke_chain_id` and both chain-set params MUST be 0/empty for it, same as
    ///      RequestsApproved — see record_settlement_tx's dev notes for the full gating.
    ///
    ///      Each chain's own terminal step depends on its role, declared at SettleStarted time
    ///      (see record_settlement_tx's dev notes): `SettleApplied` if it's in
    ///      `finalize_chain_ids` (it has a vault to deliver a result to), otherwise
    ///      `NavReceived` (it only has an Adapter — Collect/Response-only chains
    ///      never get a Finalize leg to begin with, and never have Finalize entries in
    ///      get_settlement's `steps` array at all). get_settlement's `status == Settled`
    ///      once every chain has reached its own terminal step.
    ///
    ///      A settlement that needs no cross-chain action at all is represented by
    ///      `SettleStarted` with *both* `collect_response_chain_ids` and `finalize_chain_ids`
    ///      empty — no separate step value needed: `SettleStarted` no longer requires either
    ///      set to be non-empty, so both empty alone unambiguously means "nothing to
    ///      collect/respond/finalize" via vacuous truth (get_settlement's `spoke_chains`
    ///      array comes back empty, and `status` is `Settled` immediately).
    ///
    ///      `RequestsApproved` is declared right after `NavReceived` — where it actually
    ///      fires in the real pipeline — rather than at the end, since nothing depends on
    ///      this enum's declaration order the way a raw SCALE-decoded Rust enum would (see
    ///      the pallet's own SettlementStep doc comment for the full reasoning); the
    ///      numeric `step` values below follow the same order. Records evidence for every
    ///      request_id Valuation approved into this settlement, in one batch — replaces
    ///      what used to be a per-request `SettlementApproved` step on record_request_tx
    ///      (one record_request_tx call per approved request). The underlying Valuation
    ///      Contract event changed from firing once per request
    ///      (DepositApproved/RedeemApproved) to firing once per settlement
    ///      (DepositsApproved/RedeemsApproved, each carrying an array of approved items),
    ///      so the recorder now only needs one record_settlement_tx call — carrying every
    ///      approved request_id from that one event — instead of one call per request. For a
    ///      Multichain product this fires at essentially the same moment as before (right
    ///      after every one of the settlement's collect_response_chain_ids has reported
    ///      NAV), just batched — but for a SingleChain SYNC product, Valuation emits
    ///      DepositsApproved/RedeemsApproved *before* Settled, so this step can genuinely
    ///      land before SettleStarted for the same settlement_id. Deliberately has NO
    ///      SettleStarted precondition (unlike every leg step) precisely because of that —
    ///      the old per-request SettlementApproved step this replaced never required
    ///      SettleStarted to have landed first either. Like `SettleStarted`, `RequestsApproved`
    ///      is settlement-wide rather than chain-scoped:
    ///      `spoke_chain_id` MUST be 0 and both collect_response_chain_ids/
    ///      finalize_chain_ids MUST be empty for it, same as `SettleStarted` — but unlike
    ///      `SettleStarted`, its own dedicated `request_ids` parameter is what MUST be
    ///      non-empty instead. Per-entry amounts/price the underlying approved-items array
    ///      may carry aren't recorded here — this contract only ever tracks tx evidence
    ///      and the request<->settlement linkage, never settlement financials. Because of
    ///      the race with the settlement-side completion trigger described above, recording
    ///      `RequestsApproved` also opportunistically closes each of its request_ids out of
    ///      get_investor_active_requests immediately if its settlement's completion
    ///      condition has already landed by the time this call runs.
    enum SettlementStep {
        Queued,
        SettleStarted,
        CollectBridgeExecuted,
        NavReported,
        ResponseBridgeExecuted,
        NavReceived,
        RequestsApproved,
        FinalizeBridgeExecuted,
        SettleApplied,
        Settled
    }

    /// @dev Which receivable pool a receive() tx drained — TrancheManager pools receivable
    ///      amounts per (investor, vault), not per request_id, so redeem/deposit still need
    ///      distinguishing even though neither is tied to one specific request anymore.
    ///      Renamed from `ClaimKind` (2026-08-06) — "Claim" as a term for this whole
    ///      tracking pipeline was replaced with "Receive" throughout; the underlying
    ///      investor-facing Solidity call being tracked is still literally named `receive()`
    ///      on TrancheVault, that's an external fact this rename doesn't change.
    enum ReceiveKind {
        Redeem,
        Deposit
    }

    /// @dev Step within a whitelist grant/revoke action's pipeline. Named after the
    ///      underlying Contract event wherever one exists 1:1 — `WhitelistRequested`
    ///      (Orchestrator, Hub) and `WhitelistApplied` (TrancheManager, either chain) —
    ///      same event-name-mirroring convention as RequestStep/SettlementStep.
    ///      `BridgeExecuted` is the generic Bridge-phase Socket evidence in between,
    ///      same as every other pipeline's Bridge phase.
    ///
    ///      Same Hub-vault/Spoke-vault branching as RequestStep for a Multichain product:
    ///      a whitelist action always originates on Hub (a ProductAdmin's grant_permission/
    ///      revoke_permission call, routed through Orchestrator), but TrancheManager
    ///      (the contract that actually applies the grant/revoke) can live on Hub too
    ///      — a product with a Hub-deployed vault binds a Hub-chain TrancheManager
    ///      entry the same as any Spoke chain. For a Hub-vault action,
    ///      `WhitelistApplied` follows `WhitelistRequested` directly (no Bridge leg
    ///      — `BridgeExecuted` reverts if attempted); for a Spoke-vault action, all
    ///      three steps are needed, in order.
    ///
    ///      A SingleChain product's action skips WhitelistRequested/BridgeExecuted
    ///      entirely — there's no Orchestrator at all for that model, so TrancheManager
    ///      manages the nonce itself and applies the grant/revoke in one local step,
    ///      emitting only WhitelistApplied. record_whitelist_tx lets WhitelistApplied
    ///      self-open the registry entry in this case (see that function's dev notes).
    ///
    ///      `None` is a read-only sentinel, never valid record_whitelist_tx input.
    enum WhitelistStep {
        None,
        WhitelistRequested,
        BridgeExecuted,
        WhitelistApplied
    }

    /// @dev One observed Bridge-phase attempt for a leg — a leg's full attempt
    ///      history (get_request/get_settlement/get_whitelist's new
    ///      *_bridge_attempts return values) is an ordered array of these, one per
    ///      SocketMessage resolution the recorder observed, `Executed` (3) or
    ///      `Reverted` (4) alike — `status` is never `0` here, since an attempt that
    ///      exists always resolved to one or the other. At most one `Executed`
    ///      attempt can ever exist per leg — once one lands, no further attempt is
    ///      ever recorded for it (nothing left to retry). Distinct from the
    ///      pre-existing *_steps/*_tx return values, which only ever show the single
    ///      attempt that succeeded (if any), zeroed otherwise, regardless of how many
    ///      `Reverted` attempts preceded it — see this interface's top-level dev
    ///      notes on the two views' relationship.
    ///
    ///      `status` (and every `bridge_status` parameter elsewhere in this
    ///      interface) is a plain `uint8`, deliberately NOT a Solidity `enum` — its
    ///      value matches CCCP-v2's own SocketEventStatus (`None = 0, Requested = 1,
    ///      Failed = 2, Executed = 3, Reverted = 4, Accepted = 5, Rejected = 6,
    ///      Committed = 7, Rollbacked = 8`) restricted to exactly three values:
    ///        - `0` — not applicable (matches SocketEventStatus.None) — the sentinel
    ///                every `bridge_status` parameter elsewhere in this interface
    ///                uses when no Bridge-phase step is involved. Unambiguous, since
    ///                neither real outcome below is ever `0`. Never valid for
    ///                `BridgeAttempt.status` itself (an attempt that exists always
    ///                resolved to one of the two real outcomes).
    ///        - `3` — Executed. The Bridge message was relayed and successfully
    ///                executed at its destination — the leg's own Hooks phase (if
    ///                any) is now reachable.
    ///        - `4` — Reverted. The Bridge message was rolled back — the leg never
    ///                reached its destination, and the refunded contract
    ///                (MultichainTrancheManager for an Inbound/Response leg's
    ///                refund, OrchestratorHub for an Adapter/Collect/Finalize/
    ///                Whitelist leg's refund) is expected to retry() it; any step
    ///                gated on this leg stays unreachable until some later attempt
    ///                resolves `Executed`.
    ///      Solidity enums are always contiguously 0-indexed, so a genuine `enum`
    ///      here couldn't represent this non-contiguous pair (3/4) without also
    ///      declaring seven unused placeholder variants — a recorder that already
    ///      has a raw SocketEventStatus byte in hand (from watching the Socket
    ///      contract directly) can instead pass it straight through as
    ///      `bridge_status` with no translation step. Any value other than 0/3/4
    ///      reverts wherever `bridge_status` is a `record_*_tx` input.
    /// @param status `3` (Executed) or `4` (Reverted) — see the dev notes above
    /// @param tx     Evidence for this specific attempt
    struct BridgeAttempt {
        uint8 status;
        TxRecord tx;
    }

    /// @dev One chain's full Bridge-phase attempt history for a single-leg pipeline
    ///      (a request's Adapter leg) — parallel to AdapterLeg's own per-chain shape,
    ///      but carrying every attempt observed for that leg rather than just the one
    ///      (if any) that landed in AdapterLeg.steps. Empty `attempts` means either no
    ///      attempt has been observed yet, or this chain self-fulfills synchronously
    ///      (Hub itself, or a Redeem's own origin chain — no Bridge phase at all, see
    ///      AdapterLeg's own dev notes); the two cases aren't distinguishable from this
    ///      struct alone, same as `AdapterLeg.steps` already can't distinguish "empty
    ///      because self-fulfilling" without cross-referencing the chain and order type.
    /// @param chain_id  The chain this leg is for
    /// @param attempts  Every attempt observed for this chain's Adapter leg, in order
    struct ChainBridgeAttempts {
        uint64 chain_id;
        BridgeAttempt[] attempts;
    }

    /// @dev One chain's full Bridge-phase attempt history across all three settlement
    ///      leg kinds — parallel to SettlementChainSteps's own per-chain shape, but
    ///      exposing every attempt observed for each Bridge phase (Collect/Response/
    ///      Finalize) rather than just the one (if any) that succeeded. A chain
    ///      without a given leg kind at all (e.g. a Collect/Response-only chain has no
    ///      Finalize leg) simply has an empty array for it — same "empty means either
    ///      not-yet-attempted or not-applicable" caveat as ChainBridgeAttempts.
    /// @param spoke_chain_id     The chain these attempt histories are for
    /// @param collect_attempts   Every attempt observed for this chain's Collect leg
    /// @param response_attempts  Every attempt observed for this chain's Response leg
    /// @param finalize_attempts  Every attempt observed for this chain's Finalize leg
    struct SettlementChainBridgeAttempts {
        uint64 spoke_chain_id;
        BridgeAttempt[] collect_attempts;
        BridgeAttempt[] response_attempts;
        BridgeAttempt[] finalize_attempts;
    }

    /// @dev `investor`/`vault_chain_id`/`vault_address`/`amount`/`order_type` are only
    ///      meaningful when `step == Requested` (zero/empty otherwise) — same sentinel
    ///      convention as record_request_tx's own parameters. `adapter_chain_ids` is
    ///      meaningful (and may be empty) exclusively when `step == RequestQueued`, empty
    ///      otherwise. `bridge_status` is meaningful
    ///      exclusively when `step` is `RequestBridgeExecuted` or `AdapterBridgeExecuted`
    ///      (the two Bridge-phase steps) — MUST be 0 (ignored) otherwise, same
    ///      "reuses a real enum value as its own N/A sentinel, disambiguated only by
    ///      `step`" convention `order_type` already uses. No `settlement_id` field here —
    ///      a request's link to a settlement is recorded via SettlementTxRecorded's own
    ///      `request_ids` instead, see record_settlement_tx's dev notes.
    event RequestTxRecorded(
        uint64 indexed product_id,
        bytes32 indexed request_id,
        address indexed investor,
        uint64 vault_chain_id,
        address vault_address,
        uint256 amount,
        uint8 order_type,
        RequestStep step,
        uint64[] adapter_chain_ids,
        TxAttestation attestation,
        uint8 bridge_status
    );

    /// @dev `spoke_chain_id` is 0 only when `step == SettleStarted` or `RequestsApproved`
    ///      (`collect_response_chain_ids`/`finalize_chain_ids` are then meaningful, each
    ///      independently possibly empty, only for `SettleStarted`; `request_ids` is meaningful,
    ///      and non-empty, only for `RequestsApproved`); otherwise `spoke_chain_id`
    ///      identifies the chain and all three array params are empty — same sentinel
    ///      convention as record_settlement_tx's own parameters. `settlement_id` is
    ///      only unique within `product_id`'s own namespace (each product's Valuation
    ///      Contract generates its own sequence), never globally. `bridge_status` is
    ///      meaningful exclusively when `step` is `CollectBridgeExecuted`/
    ///      `ResponseBridgeExecuted`/`FinalizeBridgeExecuted` (the three Bridge-phase leg
    ///      steps) — MUST be 0 (ignored) otherwise, same convention as
    ///      RequestTxRecorded's own `bridge_status`.
    event SettlementTxRecorded(
        uint64 indexed product_id,
        uint256 indexed settlement_id,
        uint64 indexed spoke_chain_id,
        SettlementStep step,
        uint64[] collect_response_chain_ids,
        uint64[] finalize_chain_ids,
        bytes32[] request_ids,
        TxAttestation attestation,
        uint8 bridge_status
    );

    event ReceiveTxRecorded(
        uint64 indexed product_id,
        address indexed investor,
        VaultInput vault,
        address receiver,
        uint256 amount,
        ReceiveKind kind,
        TxAttestation attestation
    );

    /// @dev No `product_id` topic here, unlike every other *TxRecorded event — deliberate:
    ///      record_whitelist_tx takes no product_id input (see its own dev notes for why),
    ///      so emitting one here would need an extra storage read this event doesn't
    ///      otherwise need. `product_id` is still recorded pallet-side (see
    ///      pallet_tranche_tx_registry::Event::WhitelistTxRecorded) for anything that
    ///      needs it. `bridge_status` is meaningful exclusively when `step ==
    ///      BridgeExecuted` — MUST be 0 (ignored) otherwise, same
    ///      convention as RequestTxRecorded's own `bridge_status`.
    event WhitelistTxRecorded(
        address indexed who,
        VaultInput vault,
        bool grant,
        uint256 nonce,
        WhitelistStep step,
        TxAttestation attestation,
        uint8 bridge_status
    );

    /**
     * @notice Attest to one tx in a request's pipeline — the single Requested tx, one
     *         Bridge/Hooks half of the Inbound leg (Spoke-vault requests only), or one
     *         Bridge/Hooks half of a per-chain Adapter leg. A request's link to a
     *         settlement is recorded separately, via record_settlement_tx's
     *         RequestsApproved step.
     * @dev Only callable by the pallet-registered tx recorder account. `attestation.tx_hash`
     *      MUST NOT be the zero hash (rejected otherwise) — the pallet's own "not yet
     *      recorded" sentinel is a stored TxRecord's `recorded_at == 0`, never `tx_hash`, so
     *      a zero `tx_hash` slipping into storage would be indistinguishable from a genuine
     *      attestation to any reader inspecting `tx_hash` alone.
     *      `investor`/`vault_chain_id`/`vault_address`/`amount`/`order_type` are sentinel-gated
     *      together: they MUST all be non-zero/non-empty when `step == Requested` (opens a
     *      fresh registry entry) and MUST all be zero/empty for every other step (rejected
     *      otherwise, to catch caller bugs early rather than silently ignoring a stray
     *      value).
     *      `adapter_chain_ids` declares every chain this request's capital will need its
     *      own Adapter leg for — every chain with a MultichainAdapter the deposited capital
     *      gets distributed to, but only those currently weighted (non-zero weightBps) at
     *      the moment the Valuation Contract actually processes the request; an Adapter
     *      weighted to 0 receives no allocation, so it doesn't force a leg. As an explicit
     *      call parameter it's meaningful (and may be empty) exclusively at
     *      `step == RequestQueued` — never at `Requested`, for either a Hub-vault or a
     *      Spoke-vault request; see RequestStep's dev notes for why this is now uniform
     *      across both. MUST be omitted (empty) at every other step. An empty set at
     *      RequestQueued means the request needs no further Adapter leg beyond whatever's
     *      already self-declared (see below); if none are, it's immediately RequestCompleted.
     *      Each Adapter chain needs an AdapterApplied call, identified by
     *      `attestation.chain_id` — preceded by AdapterBridgeExecuted for a Deposit's Adapter
     *      leg on a Spoke chain, including one that's the same as the origin vault's own
     *      chain (a Deposit's allocation is only decided once capital reaches the Hub, so
     *      routing even the origin chain's own share back out still needs a real bridge
     *      there), but not for Hub itself, nor for a Redeem whose Adapter chain is the origin
     *      vault's own chain (that share is collected out of the same chain's own Adapter at
     *      request time, before anything reaches the Hub): those two cases are fulfilled
     *      synchronously (no Bridge phase at all), so AdapterApplied alone is valid for them,
     *      and calling AdapterBridgeExecuted for either is simply unnecessary (not rejected —
     *      it just never happens in practice). A chain does NOT need to already appear in a
     *      prior RequestQueued call's
     *      `adapter_chain_ids` before its AdapterBridgeExecuted/AdapterApplied can be
     *      recorded — this pallet self-declares a not-yet-seen chain on first touch, since
     *      a self-fulfilling chain's AdapterApplied evidence can arrive before RequestQueued
     *      ever runs (record_request_tx calls only need to follow the order the recorder
     *      actually observed the underlying events, not this pipeline's own conceptual
     *      order). `step == Requested` must not be called twice for the same request_id, and
     *      RequestBridgeExecuted reverts if called for a Hub-vault request.
     *      `bridge_status` MUST be meaningful (either `3` (Executed) or `4` (Reverted))
     *      when `step` is `RequestBridgeExecuted` or `AdapterBridgeExecuted`, and MUST be
     *      `0` for every other step — same sentinel-gating convention as every other
     *      field here. Recording `4` (Reverted) means the Bridge message was
     *      rolled back for this attempt; it is never itself an error to record — only a
     *      further attempt after an `Executed` one already landed for the same leg is
     *      (nothing left to retry). Appended to the leg's own attempt list rather than
     *      overwriting — see get_request's new *_bridge_attempts return values for the
     *      full history, and BridgeAttempt's own dev notes for the "at most one Executed
     *      ever" invariant.
     *      A request's link to a settlement is recorded separately, via
     *      record_settlement_tx's `RequestsApproved` step (batched across every request
     *      approved into one settlement — see that function's dev notes and
     *      SettlementStep's own doc comment).
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
     * @param adapter_chain_ids Every chain needing its own Adapter leg — meaningful
     *                               (and may be empty) iff step == RequestQueued, empty
     *                               otherwise. Not the only way a chain can end up
     *                               declared — see dev notes above on self-declaration
     * @param step                   Which pipeline step this attestation is for
     * @param attestation            The attested off-chain tx
     * @param bridge_status          3 (Executed) or 4 (Reverted) — meaningful iff step ==
     *                               RequestBridgeExecuted or AdapterBridgeExecuted, MUST be 0
     *                               (ignored) otherwise
     */
    function record_request_tx(
        uint64 product_id,
        bytes32 request_id,
        address investor,
        uint64 vault_chain_id,
        address vault_address,
        uint256 amount,
        uint8 order_type,
        uint64[] calldata adapter_chain_ids,
        RequestStep step,
        TxAttestation calldata attestation,
        uint8 bridge_status
    ) external;

    /**
     * @notice Attest to one tx in a settlement's pipeline: the single SettleStarted tx (or,
     *         for a SingleChain product's settlement, that same tx recorded as `Settled`
     *         instead — see SettlementStep's dev notes), the (possibly batched)
     *         RequestsApproved tx, or one bridge/hooks half of a Collect/Response/Finalize
     *         leg for one chain.
     * @dev Only callable by the pallet-registered tx recorder account. `attestation.tx_hash`
     *      MUST NOT be the zero hash (rejected otherwise) — same rationale as
     *      record_request_tx's own `tx_hash` check.
     *      `step` MUST NOT be `SettlementStep.Queued` — the one read-only sentinel,
     *      reserved for get_settlement's own `status`, never a valid attestation to
     *      record. `step == SettlementStep.Settled` IS otherwise valid here, but only for
     *      a SingleChain product's settlement (reverts for Multichain — see
     *      SettlementStep's dev notes). `settlement_id` is only unique within
     *      `product_id`'s own namespace — each product's Valuation
     *      Contract generates its own sequence, same scoping as every settlement_id use in
     *      the Investments precompile (e.g. record_settlement) — so all storage
     *      here is keyed by (product_id, settlement_id), never settlement_id alone.
     *      `spoke_chain_id` and the two chain-set params are sentinel-gated, mirroring
     *      record_request_tx: for `step == SettleStarted`, `spoke_chain_id` MUST be 0 and both
     *      `collect_response_chain_ids`/`finalize_chain_ids` are meaningful — each may
     *      independently be empty, and both empty means the settlement needs no cross-chain
     *      action at all; for `step == RequestsApproved` or `step == Settled`, `spoke_chain_id`
     *      MUST also be 0 and both chain-set params MUST be empty — RequestsApproved has
     *      `request_ids` instead (see below), Settled has nothing else to supply (its chain
     *      sets are always empty by construction — see SettlementStep's dev notes); for every
     *      leg step, `spoke_chain_id` MUST be non-zero and both
     *      chain-set params MUST be empty. Safe as a sentinel because no EVM chain in this
     *      protocol's supported set is ever assigned chain_id 0. `spoke_chain_id` identifies
     *      which chain a leg step is about — not necessarily the chain `attestation.chain_id`
     *      itself executed on, since e.g. the Response leg's ResponseBridgeExecuted/NavReceived
     *      both physically execute on the Hub (see get_settlement's dev notes) even though
     *      `spoke_chain_id` there still names the chain being responded for.
     *      `collect_response_chain_ids` declares every chain (excluding Hub) with a
     *      registered Adapter this settlement queries NAV from — a CollectBridgeExecuted/
     *      NavReported/ResponseBridgeExecuted/NavReceived call is only
     *      valid for a chain in this set. `finalize_chain_ids` declares every chain
     *      (excluding Hub) with a registered vault this settlement delivers a result to — a
     *      FinalizeBridgeExecuted/SettleApplied call is only valid for a chain in
     *      this set. A chain may appear in both (it has both a registered Adapter and a
     *      registered vault) or just one. `step == SettleStarted` must be recorded exactly once
     *      per (product_id, settlement_id), before any leg step for that settlement — but NOT
     *      necessarily before a `RequestsApproved` call for it: a SingleChain SYNC product's
     *      Valuation Contract emits DepositsApproved/RedeemsApproved before Settled, so
     *      `RequestsApproved` can legitimately land before `SettleStarted` (unlike every leg
     *      step, `RequestsApproved` has no SettleStarted precondition). Within a given
     *      (spoke_chain_id, leg) pair, Bridge must be recorded before Hooks, with no
     *      duplicates — this ordering is enforced only within that pair, not across chains
     *      or legs, since chains progress independently.
     *      `request_ids` MUST be non-empty when `step == RequestsApproved` and MUST be empty
     *      for every other step — records evidence for every request_id Valuation approved
     *      into this settlement, in one batch (see SettlementStep.RequestsApproved's doc
     *      comment for the full mechanism). Reverts the whole call if any request_id in the
     *      batch doesn't have an open registry entry, or already has one recorded — same
     *      behavior a caller would see repeating an already-recorded request_id.
     *      `bridge_status` MUST be meaningful (either `3` (Executed) or `4` (Reverted)) when
     *      `step` is `CollectBridgeExecuted`/`ResponseBridgeExecuted`/`FinalizeBridgeExecuted`,
     *      and MUST be `0` for every other step — same convention as
     *      record_request_tx's own `bridge_status`; see that function's dev notes for the
     *      full retry/attempt-list semantics this shares.
     *      Emits SettlementTxRecorded.
     * @param product_id     The product this settlement belongs to
     * @param settlement_id  The settlement cycle this attestation belongs to
     * @param spoke_chain_id  The chain this leg step is for — 0 if step == SettleStarted,
     *                        RequestsApproved, or Settled (all settlement-wide, not chain-scoped)
     * @param collect_response_chain_ids Chains needing a Collect/Response leg — meaningful
     *                        (and may be empty) iff step == SettleStarted, empty otherwise
     * @param finalize_chain_ids Chains needing a Finalize leg — meaningful (and may be
     *                        empty) iff step == SettleStarted, empty otherwise
     * @param request_ids     Every request_id Valuation approved into this settlement —
     *                        required (non-empty) iff step == RequestsApproved, empty
     *                        otherwise
     * @param step            Which pipeline step this attestation is for
     * @param attestation     The attested off-chain tx
     * @param bridge_status   3 (Executed) or 4 (Reverted) — meaningful iff step is one of the three
     *                        Bridge-phase leg steps, MUST be 0 (ignored) otherwise
     */
    function record_settlement_tx(
        uint64 product_id,
        uint256 settlement_id,
        uint64 spoke_chain_id,
        uint64[] calldata collect_response_chain_ids,
        uint64[] calldata finalize_chain_ids,
        bytes32[] calldata request_ids,
        SettlementStep step,
        TxAttestation calldata attestation,
        uint8 bridge_status
    ) external;

    /**
     * @notice Attest to an investor's receive() tx on a vault — a plain local Spoke-chain tx,
     *         not part of the Bridge&Call request/settlement pipelines above. TrancheManager
     *         pools receivable amounts per (investor, vault) rather than per request_id, so
     *         this is intentionally NOT keyed by request_id and has no step ordering — one
     *         attestation per receive.
     * @dev Only callable by the pallet-registered tx recorder account. `attestation.tx_hash`
     *      MUST NOT be the zero hash (rejected otherwise) — same rationale as
     *      record_request_tx's own `tx_hash` check. There is no
     *      per-request_id link here — once a request becomes `settled` (see get_request),
     *      tracking "was THIS request specifically received" stops being meaningful, since a
     *      single receive() may drain a pooled balance spanning several distributed requests
     *      at once.
     *      `investor` is the ERC-7540 controller — the party whose depositRequest/
     *      redeemRequest this settles, matching every other "investor" field in this
     *      interface (RequestInfo.investor, get_investor_active_requests, etc.). `receiver`
     *      is who actually got the funds — TrancheManager's receive() call lets the
     *      controller designate a different receiver, so `receiver` can differ from
     *      `investor`; `receiver == investor` when the controller receives for themselves.
     *      For get_investor_receive_history to page through history, see below.
     *      Emits ReceiveTxRecorded.
     * @param product_id The product the received-against vault belongs to
     * @param vault      The vault this receive() call was against
     * @param investor   The controller whose request this settles
     * @param receiver   Who actually received the funds — may differ from investor
     * @param amount     Shares received (kind == Deposit) or assets received (kind == Redeem)
     * @param kind       Which receivable pool this receive() call drained
     * @param attestation The attested off-chain receive tx
     */
    function record_receive_tx(
        uint64 product_id,
        VaultInput calldata vault,
        address investor,
        address receiver,
        uint256 amount,
        ReceiveKind kind,
        TxAttestation calldata attestation
    ) external;

    /**
     * @notice Attest to one tx in a whitelist grant/revoke action's pipeline — the
     *         Trigger tx (Orchestrator's WhitelistRequested), the Bridge phase, or the
     *         Applied tx (TrancheManager's WhitelistApplied).
     * @dev Only callable by the pallet-registered tx recorder account. `attestation.tx_hash`
     *      MUST NOT be the zero hash (rejected otherwise) — same convention as
     *      record_request_tx/record_settlement_tx.
     *      Unlike every other record_*_tx function here, this one takes no `product_id`
     *      parameter — none of this pipeline's chain-observed evidence (vault, who, grant,
     *      nonce) carries it directly the way DepositRequested/DepositReceived do, so the
     *      pallet resolves it internally from `vault` instead, when the entry is opened
     *      (reverts if `vault` isn't registered to any product).
     *      `grant` must be resupplied identically at every step (Solidity has no
     *      `Option<bool>` to leave it sentinel-gated the way e.g. record_request_tx's
     *      investor/amount fields are) — reverts if it doesn't match the value this
     *      action was opened with.
     *      `nonce` is the correlator that ties this action's tx together — for a
     *      Multichain product, Orchestrator-generated, present from WhitelistRequested
     *      onward, threaded through the CCCP bridge message's own variants payload
     *      unchanged, and echoed back verbatim by WhitelistApplied; for a SingleChain
     *      product (no Orchestrator at all), TrancheManager-generated and only ever seen
     *      on WhitelistApplied itself.
     *      For a Multichain product: `step == WhitelistRequested` must not be called
     *      twice for the same (vault, who, nonce). `step == BridgeExecuted` reverts if
     *      `vault` is on its product's own local chain — such an action has no Bridge leg
     *      at all (TrancheManager applies the grant/revoke locally, same branching as
     *      record_request_tx's RequestBridgeExecuted); for a Spoke-vault action,
     *      WhitelistApplied is only reachable once BridgeExecuted has landed.
     *      For a SingleChain product: there is no WhitelistRequested/BridgeExecuted at
     *      all — `step == WhitelistApplied` opens the entry itself, the first time it's
     *      seen for a given (vault, who, nonce), since `vault` resolves to a registered
     *      SingleChain product.
     *      `bridge_status` MUST be meaningful (either `3` (Executed) or `4` (Reverted)) when
     *      `step == BridgeExecuted`, and MUST be `0` for every other step — same
     *      convention as record_request_tx's own `bridge_status`; see that function's dev
     *      notes for the full retry/attempt-list semantics this shares.
     *      Emits WhitelistTxRecorded.
     * @param vault        The tranche vault this whitelist action targets
     * @param who          The account whose whitelist status is being changed
     * @param grant        true = grant, false = revoke — fixed for this action, resupplied
     *                     at every step
     * @param nonce        Correlator for this action — Orchestrator-generated (Multichain)
     *                     or TrancheManager-generated (SingleChain)
     * @param step         Which pipeline step this attestation is for
     * @param attestation  The attested off-chain tx
     * @param bridge_status 3 (Executed) or 4 (Reverted) — meaningful iff step == BridgeExecuted,
     *                     MUST be 0 (ignored) otherwise
     */
    function record_whitelist_tx(
        VaultInput calldata vault,
        address who,
        bool grant,
        uint256 nonce,
        WhitelistStep step,
        TxAttestation calldata attestation,
        uint8 bridge_status
    ) external;

    /**
     * @notice Page through an investor's full receive() history for one product — every
     *         (vault, tx_hash) ever recorded via record_receive_tx.
     * @dev Same most-recent-first/offset/limit/total contract as
     *      get_investor_request_history (see that function's own dev notes for the full
     *      rationale, including why pagination bounds the response size but not the
     *      underlying storage read cost) — this is its receive-side equivalent, since a
     *      receive isn't linked to a specific request_id the way a request's own history is.
     *      Each returned (vault, tx_hash) pair is exactly what get_receive needs (together
     *      with this same investor) to resolve the full ReceiveEntry.
     *      `limit` MUST NOT exceed MAX_HISTORY_PAGE_SIZE (50) — rejected, not silently
     *      clamped.
     * @param investor   The investor (controller) address to look up
     * @param product_id The product to page history for
     * @param offset     How many of the most-recent entries to skip
     * @param limit      Max entries to return — MUST NOT exceed 50
     * @return receives Up to `limit` (vault, tx_hash) pairs, most-recent first
     * @return total    Total history length for this (investor, product_id)
     */
    function get_investor_receive_history(
        address investor,
        uint64 product_id,
        uint256 offset,
        uint256 limit
    )
        external
        view
        returns (ReceiveHistoryEntry[] memory receives, uint256 total);

    /**
     * @notice Resolve one (investor, vault, tx_hash) entry from get_investor_receive_history
     *         (or observed directly off a ReceiveTxRecorded event) into its full detail.
     * @dev Exactly the same "history gives you an identifier, this resolves it" relationship
     *      get_request/get_settlement have with request_id/settlement_id, except receives
     *      need all three key parts since ReceiveEntries has no single-field lookup the way
     *      RequestEntries/SettlementTriggers do.
     *      Reverts if no such entry exists (investor/vault/tx_hash must exactly match a
     *      prior record_receive_tx call) — same convention as get_request, not
     *      get_settlement's more lenient zeroed-response-for-not-yet-started behavior,
     *      since there's no meaningful "not yet" state for a receive: either the tx_hash
     *      was attested or it wasn't.
     * @param investor The controller whose request this receive() call settled
     * @param vault    The vault this receive() call was against
     * @param tx_hash  The receive() tx's hash on that vault's chain
     * @return receiver Who actually received the funds — may differ from investor
     * @return amount   Shares received (kind == Deposit) or assets received (kind == Redeem)
     * @return kind     Which receivable pool this receive() call drained
     * @return tx       Evidence for this receive() tx
     */
    function get_receive(
        address investor,
        VaultInput calldata vault,
        bytes32 tx_hash
    )
        external
        view
        returns (
            address receiver,
            uint256 amount,
            ReceiveKind kind,
            TxRecord memory tx
        );

    /**
     * @notice Read the most recent whitelist action's nonce for a given (vault, who).
     * @dev Reverts if no WhitelistRequested has ever been recorded for this pair. Pass the
     *      returned nonce straight into get_whitelist for that action's full state — this
     *      interface keeps no history array (see the top-level Whitelist notes above), so
     *      this is the only on-chain way to discover a (vault, who) pair's current nonce
     *      without already knowing it from watching WhitelistTxRecorded.
     * @param vault The tranche vault to look up
     * @param who   The account whose whitelist status to look up
     * @return nonce The most recent whitelist action's nonce for this pair
     */
    function get_latest_whitelist_nonce(
        VaultInput calldata vault,
        address who
    ) external view returns (uint256 nonce);

    /**
     * @notice Read a whitelist grant/revoke action's full state in one call.
     * @dev Reverts if record_whitelist_tx has never opened an entry for this
     *      (vault, who, nonce) — via WhitelistRequested (Multichain) or self-opened via
     *      WhitelistApplied (SingleChain, see that function's dev notes) — same
     *      convention as get_request, not get_settlement's more lenient
     *      zeroed-response-for-not-yet-started behavior, since there's no meaningful
     *      "not yet" state to report: a whitelist action doesn't exist at all until
     *      some step has opened it.
     *      For a Multichain product's action: `steps[0]` is always
     *      `(WhitelistRequested, request_tx)` and `steps`' last entry is always
     *      `(WhitelistApplied, applied_tx)` — same Hub-vault/Spoke-vault branching as
     *      get_request's request_steps: for a Hub-vault action (`vault.chain_id` equals
     *      this chain's own EVM chain ID), that's the array's only two entries (length
     *      2, no Bridge leg); for a Spoke-vault one, a `BridgeExecuted` entry sits
     *      between them (length 3).
     *      For a SingleChain product's action: WhitelistRequested never appears at all
     *      — there's no Orchestrator-driven Trigger for that model — so `steps` is just
     *      `[WhitelistApplied]` (length 1). Check `steps.length` (1 vs 2 vs 3) to tell
     *      which case this action is.
     *      `status` is the last step whose evidence has actually landed
     *      (`tx.recorded_at != 0`).
     *      `bridge_attempts` is the Bridge leg's full attempt history — every attempt
     *      observed, `Executed` or `Reverted` alike, in order — as opposed to `steps`'
     *      own `BridgeExecuted` entry, which only ever shows the single attempt that
     *      succeeded (if any), zeroed otherwise, regardless of how many `Reverted`
     *      attempts preceded it. Empty for a Hub-vault or SingleChain-product action
     *      (no Bridge leg at all — same cases `steps` itself omits `BridgeExecuted`
     *      for), or simply not yet attempted.
     * @param vault The tranche vault this whitelist action targeted
     * @param who   The account whose whitelist status was being changed
     * @param nonce Correlator for this action — Orchestrator-generated (Multichain) or
     *              TrancheManager-generated (SingleChain)
     * @return grant  true = grant, false = revoke
     * @return steps  Ordered step history — length 1 (SingleChain), 2 (Hub-vault), or 3
     *                (Spoke-vault)
     * @return status The furthest step reached so far
     * @return bridge_attempts The Bridge leg's full attempt history, see above
     */
    function get_whitelist(
        VaultInput calldata vault,
        address who,
        uint256 nonce
    )
        external
        view
        returns (
            bool grant,
            WhitelistTxStep[] memory steps,
            WhitelistStep status,
            BridgeAttempt[] memory bridge_attempts
        );

    /**
     * @notice Read a settlement's full state in one call: SettleStarted evidence, the
     *         settlement's own overall status, and every registered chain's ordered
     *         step-by-step history.
     * @dev Does not revert for a not-yet-started (product_id, settlement_id) — returns a
     *      zeroed `settle_started_tx`, `status == Queued`, and empty `spoke_chains` instead, so
     *      callers can poll a not-yet-started settlement_id without a revert.
     *      Chain IDs are not returned separately — read them off
     *      `spoke_chains[i].spoke_chain_id`. `spoke_chains` is the union of
     *      `collect_response_chain_ids` and `finalize_chain_ids` declared at SettleStarted time
     *      (collect_response-declared chains first, then any finalize-only chains not
     *      already included; empty if SettleStarted with no cross-chain action needed, or if
     *      not yet started at all).
     *      `spoke_chains[i].steps` only contains the step kinds that chain's role actually
     *      needs — see SettlementChainSteps' own dev notes — so a step's absence there means
     *      "does not apply," never "not yet reached"; within the array, each entry's own
     *      `tx.recorded_at == 0` means "not yet reached." There is no separate on-chain
     *      aggregate beyond `status`/each chain's own last-step check (Valuation tracks its
     *      own completion condition internally; this call exists purely for external
     *      registry visibility).
     *      `status` only ever takes one of three values:
     *      - `Queued` — SettleStarted not yet recorded (`settle_started_tx` is then zeroed too).
     *      - `SettleStarted` — SettleStarted recorded, but at least one chain hasn't yet reached
     *        its own terminal step (last entry in its `steps` array).
     *      - `Settled` — every chain has reached its own terminal step (vacuously true, and
     *        immediate, if SettleStarted with both chain sets empty, i.e. `spoke_chains` is
     *        empty). `settle_started_tx` itself never changes once recorded — only `status`
     *        moves from `SettleStarted` to `Settled` as chains complete.
     *      `spoke_bridge_attempts` is each chain's full Bridge-phase attempt history
     *      across all three leg kinds — every attempt observed, `Executed` or `Reverted`
     *      alike, in order — as opposed to `spoke_chains[i].steps`' own Bridge-phase
     *      entries, which only ever show the single attempt that succeeded (if any),
     *      zeroed otherwise. Same chain ordering as `spoke_chains`; a chain without a
     *      given leg kind simply has an empty array for it (see
     *      SettlementChainBridgeAttempts' own dev notes).
     * @param product_id    The product the settlement belongs to
     * @param settlement_id The settlement to look up
     * @return settle_started_tx   Evidence for the SettleStarted step
     * @return status       The settlement's own overall status — `Queued`/`SettleStarted`/`Settled`
     * @return spoke_chains Per-chain ordered step history, see above
     * @return spoke_bridge_attempts Per-chain full Bridge-phase attempt history, see above
     */
    function get_settlement(
        uint64 product_id,
        uint256 settlement_id
    )
        external
        view
        returns (
            TxRecord memory settle_started_tx,
            SettlementStep status,
            SettlementChainSteps[] memory spoke_chains,
            SettlementChainBridgeAttempts[] memory spoke_bridge_attempts
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
     * @notice Page through an investor's full request history for one product — every
     *         request_id ever opened (record_request_tx, step == Requested), including
     *         ones long since completed and no longer in get_investor_active_requests.
     * @dev Returned most-recent first; `offset`/`limit` index into that most-recent-first
     *      order (`offset == 0` is the single most recent request). `total` is the full
     *      history length for this (investor, product_id), so a caller can compute page
     *      count without a separate call; `offset >= total` returns an empty array rather
     *      than reverting, so a caller can page forward until it gets one back.
     *      `limit` MUST NOT exceed MAX_HISTORY_PAGE_SIZE (50) — rejected, not silently
     *      clamped, same "catch caller bugs early" convention as every other sentinel-gated
     *      parameter in this interface. This bounds the response size, but does NOT bound
     *      the underlying storage read cost: the full per-(investor, product_id) history is
     *      always read and decoded from storage first, then sliced down to the requested
     *      page — a very long history costs the same gas as a short one despite doing more
     *      real work under the hood.
     * @param investor    The investor address to look up
     * @param product_id  The product to page history for
     * @param offset      How many of the most-recent entries to skip
     * @param limit       Max entries to return — MUST NOT exceed 50
     * @return request_ids Up to `limit` request_ids, most-recent first
     * @return total       Total history length for this (investor, product_id)
     */
    function get_investor_request_history(
        address investor,
        uint64 product_id,
        uint256 offset,
        uint256 limit
    ) external view returns (bytes32[] memory request_ids, uint256 total);

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
     *      exists has one, unconditionally. For a Multichain product, `request_steps`'
     *      last entry is always `(RequestQueued, queued_tx)` — every such request reaches
     *      this step, Hub-vault or Spoke-vault alike. For a Hub-vault request (no Inbound
     *      leg applies at all — `info.vault.chain_id` equals this chain's own EVM chain
     *      ID), that's the array's only other entry (length 2); for a Spoke-vault one, a
     *      `RequestBridgeExecuted` entry sits between them (length 3). For a SingleChain
     *      product's request, `RequestQueued` never appears at all — there's no
     *      DepositQueued/RedeemQueued-equivalent event for that model (see RequestStep's
     *      dev notes) — so `request_steps` is just `[Requested]` (length 1). Same "absent
     *      means not applicable, present-but-zeroed means pending" convention as
     *      get_settlement's `steps` arrays, so check `request_steps.length` (1 vs 2 vs 3)
     *      to tell which case this request is, and each present entry's own
     *      `tx.recorded_at` to tell whether it's landed yet. `adapter_legs[i].steps` is
     *      `[AdapterBridgeExecuted, AdapterApplied]` (length 2) for a Deposit's Adapter leg
     *      on a Spoke chain — including one that's the same as the origin vault's own chain,
     *      since a Deposit's allocation is only decided once capital reaches the Hub — but
     *      just `[AdapterApplied]` (length 1) for this product's own local chain (Hub, any
     *      order type), or for a Redeem whose Adapter chain is the origin vault's own chain
     *      (collected out of that chain's own Adapter at request time, before anything
     *      reaches the Hub) — same "absent means not applicable" convention as request_steps
     *      above, since neither ever gets a Bridge phase (fulfilled synchronously — see
     *      record_request_tx's dev notes), not merely a pending one. `adapter_legs` itself
     *      is always empty for a SingleChain product — its Adapters are colocated too, so
     *      there's no leg to track at all. Check `adapter_legs[i].steps.length` (1 vs 2)
     *      the same way request_steps.length is checked. `RequestAdapterChains`' own
     *      order (and so `adapter_legs`' order) is normally the order declared at
     *      RequestQueued, but a self-fulfilling entry (recorded before RequestQueued
     *      ever ran) appears in whatever order it was first touched instead;
     *      `adapter_legs` itself is empty if none were declared, yet or ever.
     *      `status` only ever takes `Requested` (`RequestQueued` not yet reached, or some
     *      Adapter leg still has an unfinished step — never true for a SingleChain
     *      product, which has neither) or `RequestCompleted` (`RequestQueued` reached and
     *      every declared Adapter chain's last step landed, or none were declared at all —
     *      immediate for a Multichain product's fully local request, and always immediate
     *      for a SingleChain product's request, right from `Requested`, since neither
     *      RequestQueued nor any Adapter leg ever applies to it). This is deliberately
     *      unrelated to `settlement_id`/`settled` below — see RequestStep's dev notes for
     *      why `RequestCompleted` was renamed from `Completed` to disambiguate the two.
     *      `settlement_id`/`settled` are resolved entirely from this pallet's own registry
     *      entry, written by record_settlement_tx's `RequestsApproved` step (Valuation's
     *      DepositsApproved/RedeemsApproved event) — `settlement_id` is 0 until that step is
     *      recorded. Safe as a sentinel because settlement_id is 1-indexed protocol-wide (0
     *      is reserved to mean "no settlement", the first real settlement is 1) — see the
     *      Investments precompile's get_settlement_id.
     *      `settled` depends on whether this request's own vault is on Hub or Spoke,
     *      mirroring the Inbound-leg asymmetry above (a Hub-vault request has no Finalize
     *      leg of its own to wait on):
     *      - Spoke vault: true once the Finalize leg's Hooks phase is recorded
     *        (get_settlement) for `settlement_id` on this request's own origin chain —
     *        escrow share burn + payout-receivable for redeem, or distributeShares for
     *        deposit.
     *      - Hub vault: true once every one of `settlement_id`'s `collect_response_chain_ids`
     *        has reached `NavReceived` (vacuously true, and immediate, if that set
     *        was declared empty at SettleStarted) — NAV must be fully known before the Hub vault's
     *        own payout/allocation can be computed, which then happens synchronously with no
     *        Finalize leg of its own.
     *      Named `settled` rather than `receivable` to read naturally against
     *      `settlement_id`: non-zero `settlement_id` + `settled == false` means still being
     *      settled; non-zero `settlement_id` + `settled == true` means fully settled. It
     *      does not mean received: the investor's own separate receive() call (withdraw/redeem
     *      or deposit/mint), taken at whatever later time they choose, is not tracked
     *      per-request — see record_receive_tx (ReceiveTxRecorded), which tracks receives
     *      per (investor, vault) instead, since TrancheManager pools receivable amounts
     *      rather than keeping them keyed by request_id. `settled` is always false while
     *      `settlement_id == 0`, and is entirely independent of
     *      `status`/`adapter_legs` — a request's own delivery to the Hub and its
     *      linked settlement's delivery of results back out are two separate concerns
     *      tracked here.
     *      `request_bridge_attempts` is the Inbound leg's full attempt history — every
     *      attempt observed, `Executed` or `Reverted` alike, in order — as opposed to
     *      `request_steps`' own `RequestBridgeExecuted` entry, which only ever shows the
     *      single attempt that succeeded (if any). Empty for a Hub-vault or
     *      SingleChain-product request (no Inbound leg at all), or simply not yet
     *      attempted. `adapter_bridge_attempts` is the same idea per Adapter leg, parallel
     *      to `adapter_legs` (same chain order); a self-fulfilling entry (Hub, or a Redeem's
     *      own origin chain) is always empty (no Bridge phase at all for it — see
     *      AdapterLeg's own dev notes).
     * @param product_id The product the request belongs to
     * @param request_id The request to look up
     * @return info           Investor/vault/amount/order_type, unchanged since Requested
     * @return request_steps  Ordered Requested + Inbound-leg step history, see above
     * @return adapter_legs   Per-chain ordered Adapter-leg step history, see above
     * @return status         `Requested` or `RequestCompleted` — see dev notes above
     * @return settlement_id  The settlement this request is linked to, 0 if not yet linked
     * @return settled        Whether this request's settlement has fully completed (the
     *                        investor can now call receive() for it, though that call itself
     *                        isn't tracked here — see record_receive_tx)
     * @return request_bridge_attempts The Inbound leg's full attempt history, see above
     * @return adapter_bridge_attempts Per-chain full Adapter-leg attempt history, see above
     */
    function get_request(
        uint64 product_id,
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
            bool settled,
            BridgeAttempt[] memory request_bridge_attempts,
            ChainBridgeAttempts[] memory adapter_bridge_attempts
        );
}
