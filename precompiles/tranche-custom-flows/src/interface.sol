// SPDX-License-Identifier: GPL-3.0-only
pragma solidity >=0.8.0;

/**
 * @title Tranche Custom Flows Precompile Interface (tranche-system draft)
 * @dev Deployed at 0x0000000000000000000000000000000000000204.
 * @notice An append-only on-chain evidence log for **product-specific extra tx flows** in
 *         the OmniFi tranche-system — reward-pool token claims, strategy swap+bridge
 *         sequences, and anything else that doesn't belong in one of
 *         pallet-tranche-tx-registry's four universal pipelines (request / settlement /
 *         whitelist / receive).
 *
 *         The step topology of each flow is defined by an on-chain `FlowDescriptor`,
 *         registered by governance (Root) via a pallet-native extrinsic that is *not*
 *         exposed here — so a new flow costs one governance call, not a pallet release.
 *         `record_flow_tx` below is the only write entry point this interface exposes, and
 *         it reverts unless `msg.sender` is the account currently registered as the shared
 *         tx recorder (the same account pallet-tranche-tx-registry uses).
 *
 *         Trust model: identical to pallet-tranche-tx-registry — the single recorder
 *         account's attestations are taken at face value. The pallet records what the
 *         recorder submits and computes flow completion structurally (counting satisfied
 *         required slots); it never re-verifies txs, parses `metadata`, or enforces step
 *         ordering. Consumers (front-ends / indexers) interpret step meaning, ordering, and
 *         the opaque `metadata` bytes via an off-chain schema keyed by `(flow_id, slot_id)`.
 *
 * ## Model
 *
 *  - **Descriptor axis**: `flow` -> `track` (one `main` track + N `sub_tracks`) -> `slot`.
 *  - **Runtime axis**: `instance` -> `lane` -> slot-record -> `attempt`.
 *
 *  A `sub_track` is a parallel-branch kind (e.g. "per adapter chain"): one step sequence
 *  shared by a fixed list of allowed chains. A `lane` is a track realised for one instance
 *  — the main track is always exactly one lane; sub track `k` has one lane per participating
 *  chain. A chain that appears in two sub tracks is two distinct lanes.
 *
 *  `slot_id`s are pairwise disjoint across tracks, so `slot_id` alone tells the pallet
 *  which track a step belongs to. Within a sub track, every chain lane shares the same
 *  `slot_id` sequence (the same logical step, run by each chain).
 *
 * ## Instance lifecycle
 *
 *  There is no explicit "open" flag. Recording the **first main slot** (`main_slots[0].id`)
 *  for an `instance_key` that has no instance yet *creates* the instance; every other
 *  record requires the instance to already exist (the recorder only learns `instance_key`
 *  from the opening event, so it can't legitimately record anything else first).
 *
 *  Completion is computed by the pallet, never signalled by the recorder: a lane is `done`
 *  once every non-optional, non-skipped slot on it has a `success: true` attempt; the
 *  instance is `closed` once every non-optional lane is `done` and every optional lane is
 *  `done` or untouched. Late records after `closed` are still accepted.
 *
 * ## ABI conventions
 *
 *  - `flow_id` is a short ASCII slug of at most 16 bytes, passed as a `bytes16`
 *    (e.g. `bytes16("reward-claim")`).
 *  - `track_chain_id == 0` means the main lane; any other value is that chain's sub lane.
 *    The pallet derives which sub track the lane belongs to from `slot_id`.
 *  - On `record_flow_tx`: `investor == address(0)` means "not investor-scoped / not the
 *    opening call". Empty `slot_metadata` means "leave the stored value untouched"; a
 *    non-empty value overwrites it (last-write-wins). Empty `attempt_metadata` is stored
 *    as empty bytes on the new attempt.
 *  - `recorded_at` on a returned `TxRecord` is this chain's block number when the
 *    attestation was accepted — not the block number on the chain where the tx occurred.
 *    Use `tx_hash` to look the tx up on its own chain.
 */
interface TrancheCustomFlows {
    // -----------------------------------------------------------------------
    // Structs — read side
    // -----------------------------------------------------------------------

    /// Stored tx evidence for one attempt.
    struct TxRecord {
        uint64 chain_id; // chain the tx actually landed on
        bytes32 tx_hash;
        uint256 recorded_at; // this chain's block number when the attestation was accepted
    }

    /// One observation of a step.
    struct AttemptView {
        bool success; // whether the recorder marked this observation as satisfying the slot
        bytes metadata; // opaque per-attempt bytes, consumer-interpreted
        TxRecord tx;
    }

    /// The evidence recorded for one `(lane, slot)` of an instance.
    struct SlotView {
        uint8 slot_id;
        bool satisfied; // has any `success: true` attempt landed
        bytes slot_metadata; // opaque per-slot bytes, last-write-wins
        AttemptView[] attempts; // in observation order
    }

    /// One chain's lane within a sub track, for `get_flow_instance`. Only lanes with at
    /// least one recorded slot appear.
    struct SubLaneView {
        uint64 chain;
        SlotView[] slots; // recorded slots only, ascending by slot_id
    }

    /// All recorded chain lanes of one sub track, for `get_flow_instance`. A sub track with
    /// no recorded lane at all is omitted from the `sub_tracks` return value.
    struct SubLaneGroup {
        uint8 track_index; // index into the descriptor's sub_tracks
        SubLaneView[] lanes; // ascending by chain
    }

    /// One descriptor step.
    struct SlotDefView {
        uint8 id;
        bool optional; // an optional step doesn't count toward completion
    }

    /// One chain's participation in a sub track.
    struct TrackChainView {
        uint64 chain_id;
        bool optional; // this chain may be absent from a given instance
        uint8[] skip_slots; // steps in the track sequence this one chain doesn't do
    }

    /// One sub track — a step sequence shared by a list of allowed chains.
    struct DescriptorTrackView {
        SlotDefView[] slots;
        TrackChainView[] chains;
    }

    // NOTE: `get_flow_descriptor` / `get_flow_instance` return their fields as a flat
    // tuple, not a single wrapping struct — this matches how the precompile encodes an
    // N-value return (same convention as `precompile-tranche-tx-registry`). Decode into
    // the individual `returns (...)` values below, not into an aggregate struct.

    // -----------------------------------------------------------------------
    // Events
    // -----------------------------------------------------------------------

    /// Emitted by `record_flow_tx` for every accepted attestation — mirrors the pallet's
    /// own `FlowTxRecorded` event so EVM-side indexers can follow a flow via `eth_getLogs`.
    /// `FlowOpened` / `FlowClosed` are surfaced only as native runtime events for now.
    event FlowTxRecorded(
        uint64 indexed product_id,
        bytes16 indexed flow_id,
        bytes32 indexed instance_key,
        uint64 track_chain_id,
        uint8 slot_id,
        bool success
    );

    // -----------------------------------------------------------------------
    // Write — recorder only
    // -----------------------------------------------------------------------

    /**
     * @notice Record one observed on-chain tx for a flow step. Reverts unless `msg.sender`
     *         is the registered tx recorder. Purely observational — completion is computed
     *         by the pallet, not signalled here.
     *
     *         Recording `main_slots[0].id` for an unseen `instance_key` opens the instance;
     *         any other slot with no instance reverts. Recording the same `tx_hash` twice
     *         for the same `(instance, lane, slot)` reverts.
     *
     * @param product_id      Product the flow belongs to.
     * @param flow_id         Flow slug (`bytes16`).
     * @param instance_key    This flow-execution's correlation id (claim_id / request_id / claim tx_hash).
     * @param track_chain_id  0 for the main lane; otherwise the sub-lane chain id.
     * @param slot_id         Which descriptor step this attests to.
     * @param chain_id        Chain the tx actually landed on (for explorer lookup only — not matched against the lane).
     * @param tx_hash         The tx hash. MUST be non-zero.
     * @param success         Whether this observation satisfies the slot.
     * @param attempt_metadata Opaque per-attempt bytes. Empty is fine.
     * @param slot_metadata    Opaque per-slot bytes. Empty leaves the stored value untouched; non-empty overwrites it.
     * @param investor        Investor for the opening call iff the flow is investor-scoped; address(0) otherwise.
     */
    function record_flow_tx(
        uint64 product_id,
        bytes16 flow_id,
        bytes32 instance_key,
        uint64 track_chain_id,
        uint8 slot_id,
        uint64 chain_id,
        bytes32 tx_hash,
        bool success,
        bytes memory attempt_metadata,
        bytes memory slot_metadata,
        address investor
    ) external;

    // -----------------------------------------------------------------------
    // Read
    // -----------------------------------------------------------------------

    /**
     * @notice The `instance_key`s `investor` has open (not yet closed) for one
     *         `(product_id, flow_id)`. Only investor-scoped flows are indexed here, and an
     *         entry is removed on close — a flow that opens and closes in one call never
     *         appears (find it in `get_investor_flow_history`). Reverts if the investor
     *         somehow has more than 500 flows open across all products (should never happen).
     * @param investor   The investor address to look up.
     * @param product_id The product the flow belongs to.
     * @param flow_id     The flow slug (`bytes16`).
     * @return instance_keys The investor's open `instance_key`s for this `(product_id, flow_id)`.
     */
    function get_investor_active_flows(
        address investor,
        uint64 product_id,
        bytes16 flow_id
    ) external view returns (bytes32[] memory instance_keys);

    /**
     * @notice Page through the `instance_key`s `investor` has completed for one
     *         `(product_id, flow_id)`, most-recent first. `offset`/`limit` index into that
     *         order (`offset == 0` is the single most recent). `offset >= total` returns an
     *         empty array rather than reverting, so a caller can page forward until it gets
     *         one back.
     *
     *         `limit` MUST NOT exceed 50 (reverts otherwise). This bounds the response, not
     *         the read cost: the product's whole history `Vec` is read and decoded, then
     *         filtered to `flow_id` and sliced in memory.
     *
     * @param investor   The investor address to look up.
     * @param product_id The product the flow belongs to.
     * @param flow_id     The flow slug (`bytes16`) to filter history by.
     * @param offset     How many of the most-recent matching entries to skip.
     * @param limit      Max entries to return — MUST NOT exceed 50.
     * @return instance_keys Up to `limit` `instance_key`s, most-recent first.
     * @return total         Total number of completed instances for this `(investor, product_id, flow_id)`.
     */
    function get_investor_flow_history(
        address investor,
        uint64 product_id,
        bytes16 flow_id,
        uint256 offset,
        uint256 limit
    ) external view returns (bytes32[] memory instance_keys, uint256 total);

    /**
     * @notice The flow's step topology — the chains it can touch, the slots on each, and
     *         which chains/slots are optional. Front-ends use this to lay out expected vs
     *         recorded progress. `version == 0` iff no descriptor is registered.
     * @param product_id The product the flow belongs to.
     * @param flow_id     Flow slug (`bytes16`).
     * @return version         Descriptor version (0 iff unregistered).
     * @return main_chain_id    Chain the main-track steps occur on (self-describing).
     * @return investor_scoped  Whether instances are indexed per-investor.
     * @return main_slots       The main track's step definitions.
     * @return sub_tracks       The sub tracks, in descriptor order.
     */
    function get_flow_descriptor(
        uint64 product_id,
        bytes16 flow_id
    )
        external
        view
        returns (
            uint16 version,
            uint64 main_chain_id,
            bool investor_scoped,
            SlotDefView[] memory main_slots,
            DescriptorTrackView[] memory sub_tracks
        );

    /**
     * @notice Full timeline for one flow execution: every recorded slot, grouped into the
     *         main lane and per-sub-track / per-chain sub lanes.
     *
     *         Scans every recorded slot for the instance. Reverts if that exceeds 1024
     *         (the descriptor allows more in theory, but a real flow records far fewer).
     *
     * @param product_id   The product the flow belongs to.
     * @param flow_id       Flow slug (`bytes16`).
     * @param instance_key  The flow-execution to look up.
     * @return investor       address(0) unless the flow is investor-scoped.
     * @return closed         Whether every required lane has completed.
     * @return pending_lanes  Lanes still needed for completion (0 iff closed).
     * @return opened_at      Block number the instance opened; 0 iff it does not exist.
     * @return main_lane      Recorded main-track slots, ascending by slot_id.
     * @return sub_tracks     Recorded sub lanes, grouped by track then chain.
     */
    function get_flow_instance(
        uint64 product_id,
        bytes16 flow_id,
        bytes32 instance_key
    )
        external
        view
        returns (
            address investor,
            bool closed,
            uint16 pending_lanes,
            uint64 opened_at,
            SlotView[] memory main_lane,
            SubLaneGroup[] memory sub_tracks
        );
}
