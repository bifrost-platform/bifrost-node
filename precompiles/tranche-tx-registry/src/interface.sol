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
 *    bridge_tx/applied_tx (or bridge_tx/queued_tx) pair — except when an Adapter leg's chain is
 *    the same chain as the origin vault, or Hub itself: that chain's allocation is fulfilled
 *    synchronously (no Bridge phase at all — the Adapter Contract call is local), so only
 *    applied_tx is ever set for it. `adapter_chain_ids` — which chains need an Adapter leg —
 *    is normally declared at RequestQueued, once the capital has genuinely arrived at the
 *    Valuation Contract (never at Requested itself), but a self-fulfilling chain's
 *    AdapterApplied evidence can be recorded before RequestQueued ever runs (its Adapter
 *    Contract call happens locally, in the same tx as — and possibly logged before — the
 *    domain event that would otherwise declare it); record_request_tx accepts
 *    AdapterBridgeExecuted/AdapterApplied calls in whatever order the recorder actually
 *    observed the underlying events, self-declaring a not-yet-seen chain rather than requiring
 *    RequestQueued to have listed it first. A request needing no cross-chain action at all
 *    (Hub vault, no weighted remote Adapters) declares an empty set at RequestQueued and is
 *    immediately RequestCompleted. Separately, once Valuation's DepositApproved/RedeemApproved
 *    event links a request to a settlement_id, a single SettlementApproved tx records that
 *    linkage too — deliberately named apart from RequestCompleted, which is about the
 *    request's own delivery pipeline finishing, not whether its settlement has (see
 *    RequestStep's dev notes).
 *  - Settlement (per settlement_id, fanned out per chain): a single trigger_tx
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
 *    action at all is Triggered with both sets empty.
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
    ///      AdapterApplied]` (length 2) for a genuinely remote chain — this leg is
    ///      done iff `steps[1].tx.recorded_at != 0`. For a chain that's the same as
    ///      the request's own origin vault chain, or Hub itself, the allocation is
    ///      fulfilled synchronously (no Bridge phase at all — see
    ///      record_request_tx's dev notes), so `steps` is just `[AdapterApplied]`
    ///      (length 1) instead; this leg is done iff `steps[0].tx.recorded_at != 0`.
    ///      Always check `steps.length` before indexing — it is NOT fixed the way
    ///      SettlementTxStep-family arrays with a truly constant shape are.
    /// @param chain_id The chain this leg is for
    /// @param steps    `[AdapterBridgeExecuted, AdapterApplied]` (remote chain) or
    ///                 `[AdapterApplied]` (self-fulfilling chain — origin vault's own
    ///                 chain, or Hub)
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
    ///      `RequestQueued`/`AdapterBridgeExecuted`/`AdapterApplied`/`SettlementApproved`
    ///      are the only six values record_request_tx ever accepts as input. Named after
    ///      the underlying Valuation Contract events wherever one exists 1:1 — `Requested`
    ///      (DepositRequested/RedeemRequested, at TrancheManager), `RequestQueued`
    ///      (DepositQueued/RedeemQueued, at Valuation), `AdapterApplied`
    ///      (Supplied/WithdrawRequested, at the MultichainAdapter) — so a recorder can map
    ///      "which event did I just see" to "which step do I record" without needing to
    ///      special-case Hub-vault vs Spoke-vault:
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
    ///      - `SettlementApproved` — Valuation's DepositApproved/RedeemApproved event,
    ///        linking this request to a settlement_id. Named to be unmistakable next to
    ///        `RequestCompleted` below: this is about the request being approved *into a
    ///        settlement*, not about the request's own delivery pipeline finishing (a
    ///        deliberately different concept). For a Multichain product this fires at Hub
    ///        Valuation right after every one of the settlement's collect_response_chain_ids
    ///        has reported NAV — i.e. at essentially the same moment the settlement's own
    ///        Hub-vault completion condition becomes true, via a separate,
    ///        independently-ordered record_settlement_tx call. Because of this race, the
    ///        pallet opportunistically self-closes this one request out of
    ///        get_investor_active_requests immediately if its settlement's completion
    ///        condition has already landed by the time this call runs — see
    ///        record_request_tx's dev notes.
    ///
    ///      `adapter_chain_ids` is only ever supplied at `RequestQueued` — never at
    ///      `Requested`, regardless of Hub or Spoke. `settlement_id` is only ever supplied
    ///      at `SettlementApproved`.
    ///
    ///      `None` and `RequestCompleted` are read-only sentinels, never valid
    ///      record_request_tx input. Unlike SettlementStep.Queued (which get_settlement
    ///      genuinely returns for an untriggered settlement), `None` is never actually
    ///      returned by get_request either — get_request reverts outright for a request_id
    ///      that was never opened, so there's no "not yet requested" state to report. Named
    ///      `None` rather than `Queued` specifically to avoid sitting next to
    ///      `RequestQueued` under a near-identical name while meaning something completely
    ///      different. `RequestCompleted` (renamed from `Completed`) is get_request's own
    ///      `status` value once `RequestQueued` has landed and every declared Adapter
    ///      chain's last step has too — it describes the request's own delivery pipeline
    ///      finishing, and is deliberately unrelated to whether the settlement it's linked
    ///      to (see `SettlementApproved` above) has itself settled; get_request's separate
    ///      `settled` return value answers that second question.
    enum RequestStep {
        None,
        Requested,
        RequestBridgeExecuted,
        RequestQueued,
        AdapterBridgeExecuted,
        AdapterApplied,
        RequestCompleted,
        SettlementApproved
    }

    /// @dev Step within the settlement pipeline. `Queued`/`Triggered`/`Settled` are the
    ///      three overall states get_settlement's own `status` moves through —
    ///      `Queued` (not yet triggered), `Triggered` (triggered, awaiting completion),
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
    ///      `Queued` and `Settled` are read-only sentinels — "nothing recorded yet" and
    ///      "settlement fully complete", respectively — and must never be passed to
    ///      record_settlement_tx (seven recordable values in total: `Triggered` plus the six
    ///      leg steps).
    ///
    ///      Each chain's own terminal step depends on its role, declared at Trigger time
    ///      (see record_settlement_tx's dev notes): `SettleApplied` if it's in
    ///      `finalize_chain_ids` (it has a vault to deliver a result to), otherwise
    ///      `NavReceived` (it only has an Adapter — Collect/Response-only chains
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
        NavReported,
        ResponseBridgeExecuted,
        NavReceived,
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

    /// @dev `investor`/`vault_chain_id`/`vault_address`/`amount`/`order_type` are only
    ///      meaningful when `step == Requested` (zero/empty otherwise) — same sentinel
    ///      convention as record_request_tx's own parameters. `adapter_chain_ids` is
    ///      meaningful (and may be empty) exclusively when `step == RequestQueued`, empty
    ///      otherwise. `settlement_id` is meaningful (and non-zero) exclusively when
    ///      `step == SettlementApproved`, zero otherwise.
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
        uint256 settlement_id
    );

    /// @dev `spoke_chain_id` is 0 only when `step == Triggered` (`collect_response_chain_ids`/
    ///      `finalize_chain_ids` are then meaningful, each independently possibly empty);
    ///      otherwise `spoke_chain_id` identifies the chain and both arrays are empty — same
    ///      sentinel convention as record_settlement_tx's own parameters. `settlement_id` is
    ///      only unique within `product_id`'s own namespace (each product's Valuation
    ///      Contract generates its own sequence), never globally.
    event SettlementTxRecorded(
        uint64 indexed product_id,
        uint256 indexed settlement_id,
        uint64 indexed spoke_chain_id,
        SettlementStep step,
        uint64[] collect_response_chain_ids,
        uint64[] finalize_chain_ids,
        TxAttestation attestation
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
    ///      needs it.
    event WhitelistTxRecorded(
        address indexed who,
        VaultInput vault,
        bool grant,
        uint256 nonce,
        WhitelistStep step,
        TxAttestation attestation
    );

    /**
     * @notice Attest to one tx in a request's pipeline — the single Requested tx, one
     *         Bridge/Hooks half of the Inbound leg (Spoke-vault requests only), one
     *         Bridge/Hooks half of a per-chain Adapter leg, or the single
     *         SettlementApproved tx.
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
     *      `attestation.chain_id` — preceded by AdapterBridgeExecuted for a genuinely remote
     *      chain, but not for a chain that's the same as the origin vault's own chain or Hub
     *      itself: that allocation is fulfilled synchronously (no Bridge phase at all), so
     *      AdapterApplied alone is valid for it, and calling AdapterBridgeExecuted for it is
     *      simply unnecessary (not rejected — it just never happens in practice). A chain
     *      does NOT need to already appear in a prior RequestQueued call's
     *      `adapter_chain_ids` before its AdapterBridgeExecuted/AdapterApplied can be
     *      recorded — this pallet self-declares a not-yet-seen chain on first touch, since a
     *      self-fulfilling chain's AdapterApplied evidence can arrive before RequestQueued
     *      ever runs (record_request_tx calls only need to follow the order the recorder
     *      actually observed the underlying events, not this pipeline's own conceptual
     *      order). `step == Requested` must not be called twice for the same request_id, and
     *      RequestBridgeExecuted reverts if called for a Hub-vault request.
     *      `settlement_id` MUST be non-zero when `step == SettlementApproved` and MUST be
     *      zero for every other step. Recording `SettlementApproved` links `request_id` into
     *      this settlement's request set and races with the settlement-side completion
     *      trigger for a Multichain product (see RequestStep's dev notes) — the pallet
     *      opportunistically self-closes this one request out of get_investor_active_requests
     *      immediately if the settlement's completion condition has already landed by the
     *      time this call runs, rather than relying solely on the settlement-side trigger to
     *      find it later.
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
     * @param settlement_id          The settlement this request is approved into — required
     *                               (non-zero) iff step == SettlementApproved, zero otherwise
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
        uint256 settlement_id
    ) external;

    /**
     * @notice Attest to one tx in a settlement's pipeline: either the single Trigger tx, or
     *         one bridge/hooks half of a Collect/Response/Finalize leg for one chain.
     * @dev Only callable by the pallet-registered tx recorder account. `attestation.tx_hash`
     *      MUST NOT be the zero hash (rejected otherwise) — same rationale as
     *      record_request_tx's own `tx_hash` check.
     *      `step` MUST NOT be `SettlementStep.Queued` or `SettlementStep.Settled` — both are read-only
     *      sentinels reserved for get_settlement's own `status`, never a
     *      valid attestation to record. `settlement_id` is only unique within
     *      `product_id`'s own namespace — each product's Valuation
     *      Contract generates its own sequence, same scoping as every settlement_id use in
     *      the Investments precompile (e.g. record_settlement) — so all storage
     *      here is keyed by (product_id, settlement_id), never settlement_id alone.
     *      `spoke_chain_id` and the two chain-set params are sentinel-gated, mirroring
     *      record_request_tx: for `step == Triggered`, `spoke_chain_id` MUST be 0 and both
     *      `collect_response_chain_ids`/`finalize_chain_ids` are meaningful — each may
     *      independently be empty, and both empty means the settlement needs no cross-chain
     *      action at all; for every leg step, `spoke_chain_id` MUST be non-zero and both
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
        uint64 product_id,
        uint256 settlement_id,
        uint64 spoke_chain_id,
        uint64[] calldata collect_response_chain_ids,
        uint64[] calldata finalize_chain_ids,
        SettlementStep step,
        TxAttestation calldata attestation
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
     *      Emits WhitelistTxRecorded.
     * @param vault        The tranche vault this whitelist action targets
     * @param who          The account whose whitelist status is being changed
     * @param grant        true = grant, false = revoke — fixed for this action, resupplied
     *                     at every step
     * @param nonce        Correlator for this action — Orchestrator-generated (Multichain)
     *                     or TrancheManager-generated (SingleChain)
     * @param step         Which pipeline step this attestation is for
     * @param attestation  The attested off-chain tx
     */
    function record_whitelist_tx(
        VaultInput calldata vault,
        address who,
        bool grant,
        uint256 nonce,
        WhitelistStep step,
        TxAttestation calldata attestation
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
     *      get_settlement's more lenient zeroed-response-for-not-yet-triggered behavior,
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
     *      zeroed-response-for-not-yet-triggered behavior, since there's no meaningful
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
     * @param vault The tranche vault this whitelist action targeted
     * @param who   The account whose whitelist status was being changed
     * @param nonce Correlator for this action — Orchestrator-generated (Multichain) or
     *              TrancheManager-generated (SingleChain)
     * @return grant  true = grant, false = revoke
     * @return steps  Ordered step history — length 1 (SingleChain), 2 (Hub-vault), or 3
     *                (Spoke-vault)
     * @return status The furthest step reached so far
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
            WhitelistStep status
        );

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
        uint64 product_id,
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
     *      `[AdapterBridgeExecuted, AdapterApplied]` (length 2) for a genuinely remote
     *      chain, but just `[AdapterApplied]` (length 1) for a chain that's the same as
     *      the origin vault's own chain, or this product's own local chain — same "absent
     *      means not applicable" convention as request_steps above, since such a chain
     *      never gets a Bridge phase at all (fulfilled synchronously — see
     *      record_request_tx's dev notes), not merely a pending one. `adapter_legs` itself
     *      is always empty for a SingleChain product — its Adapters are colocated too, so
     *      there's no leg to track at all. Check `adapter_legs[i].steps.length` (1 vs 2)
     *      the same way request_steps.length is checked. `RequestAdapterChains`' own
     *      order (and so `adapter_legs`' order) is normally the order declared at
     *      RequestQueued, but a self-fulfilling chain (recorded before RequestQueued
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
     *      entry, written by record_request_tx's `SettlementApproved` step (Valuation's
     *      DepositApproved/RedeemApproved event) — `settlement_id` is 0 until that step is
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
     *        was declared empty at Trigger) — NAV must be fully known before the Hub vault's
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
            bool settled
        );
}
