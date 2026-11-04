// SPDX-License-Identifier: GPL-3.0-only
pragma solidity >=0.8.0;

/// @dev The TrancheTxRegistryV3 precompile address.
address constant TRANCHE_TX_REGISTRY_V3_ADDRESS = 0x0000000000000000000000000000000000000703;

/// @dev The TrancheTxRegistryV3 precompile instance.
TrancheTxRegistryV3 constant TRANCHE_TX_REGISTRY_V3_CONTRACT = TrancheTxRegistryV3(
    TRANCHE_TX_REGISTRY_V3_ADDRESS
);

/// @title pallet-tranche-tx-registry-v3 — template-driven tx evidence registry
/// @notice Every OmniFi flow (deposit, redeem, settlement, whitelist, receive,
/// and any product-specific flow) is recorded as `(slot, lane, chain_id,
/// tx_hash)` attestations against an on-chain template. The recorder is the
/// single source of truth: the registry never enforces step order — it only
/// derives progress (completed slots, done lanes, instance status) by
/// counting. See docs/tranche-tx-registry-v3/design.md.
///
/// Conventions:
/// - `flow_id` and template names are `bytes16` ASCII slugs ("deposit",
///   "mc-deposit", ...), left-aligned, zero-padded.
/// - `lane_chain_id == 0` addresses the main track; any other value addresses
///   the lane of that chain in the track holding `slot_id`.
/// - investors and `scope` are `bytes32` (EVM addresses left-padded); `bytes32(0)` = none.
/// - empty `bytes` = none; `link_key == bytes32(0)` = no link.
/// - `part_count == 0` = single-part step (`part_index` ignored). A multi-part
///   step (settlement leg chunks, several payout moves from one chain) completes
///   once every index `0..part_count` has a completing attempt.
/// - `track == 255` in a view = the main track.
/// - instance status: 0 = Open, 1 = Completed, 2 = Failed.
interface TrancheTxRegistryV3 {
    struct LaneDeclaration {
        uint8 track;
        uint64 chain;
    }

    struct SlotDefView {
        uint8 id;
        string name;
        bool required;
    }

    struct TrackView {
        string name;
        uint16 required_count;
        SlotDefView[] slots;
    }

    struct DefaultFlowView {
        bytes16 flow_id;
        bytes16 template_name;
        uint16 template_version;
    }

    struct InstanceRefView {
        bytes16 flow_id;
        bytes32 instance_key;
    }

    struct LaneView {
        uint8 track;
        uint64 chain;
        bool done;
        uint16 done_required;
        uint16 required_count;
        bool has_stage;
        uint8 stage;
        string stage_name;
    }

    struct AttemptView {
        uint64 chain_id;
        bytes tx_hash;
        uint64 recorded_at;
        bool completes;
        uint16 part_index;
        uint16 part_count;
        bytes metadata;
    }

    struct SlotView {
        uint8 track;
        uint64 lane_chain_id;
        uint8 slot_id;
        string name;
        bool completed;
        uint16 part_count;   // 0 = single-part slot
        uint64 parts_done;   // bit i set once part i has a completing attempt
        AttemptView[] attempts;
    }

    /// @dev Emitted once per attested instance (a key listed twice in a batch is logged once).
    event TxRecorded(
        uint64 indexed product_id,
        bytes16 indexed flow_id,
        bytes32 indexed instance_key,
        uint64 lane_chain_id,
        uint8 slot_id,
        uint64 chain_id,
        bytes tx_hash,
        bool completes,
        uint16 part_index,
        uint16 part_count
    );

    /// @dev Emitted when an instance closes — by `close_instance`, or by the
    /// record that completed it. `outcome`: 0 = Completed, 1 = Failed.
    event InstanceClosed(
        uint64 indexed product_id,
        bytes16 indexed flow_id,
        bytes32 indexed instance_key,
        uint8 outcome
    );

    // ---------------------------------------------------------------------
    // Writes — recorder only
    // ---------------------------------------------------------------------

    /// @notice Attest one observed tx to one slot of one instance. The first
    /// main-track attestation for an unseen `instance_key` opens the instance
    /// with the product's currently assigned template for `flow_id`. A closed
    /// instance still accepts evidence; its progress counters stay as at close.
    /// @param lane_chain_id   0 = main track; else the chain the step belongs to
    /// @param chain_id        Chain the tx actually ran on (may differ from the lane — e.g. a CCTP burn)
    /// @param completes       false for a `Reverted` bridge tx, a failed hook tx, an ignored message
    /// @param part_index      With `part_count`, which part of a multi-part step this is
    /// @param part_count      0 = single-part step; must stay the same across a slot's attempts
    /// @param metadata        Opaque per-attempt bytes (raw bridge status, ...)
    /// @param investor        Required on the opening attestation of an investor-scoped flow
    /// @param instance_metadata Overwrites the instance's metadata if non-empty (it can't be cleared)
    /// @param scope           Optional sub-key on the opening attestation of an investor-scoped flow
    ///                        (e.g. the vault); `get_latest_by_scope` resolves the newest instance per scope.
    ///                        Reverts if given on the opening attestation of a non-investor-scoped flow;
    ///                        ignored on later attestations.
    /// @param declare_lanes   Lanes this instance has (e.g. a settlement's ROUTE lanes at Settled)
    /// @param link_flow_id    With `link_key`, another instance this one references (request -> settlement)
    function record_tx(
        uint64 product_id,
        bytes16 flow_id,
        bytes32 instance_key,
        uint64 lane_chain_id,
        uint8 slot_id,
        uint64 chain_id,
        bytes calldata tx_hash,
        bool completes,
        uint16 part_index,
        uint16 part_count,
        bytes calldata metadata,
        bytes32 investor,
        bytes calldata instance_metadata,
        bytes32 scope,
        LaneDeclaration[] calldata declare_lanes,
        bytes16 link_flow_id,
        bytes32 link_key
    ) external;

    /// @notice Attest one tx to the same slot of many already open instances
    /// (a settlement tx approving / paying out N requests). Atomic. At most 50
    /// keys per call; a key listed twice is attested once.
    function record_tx_batch(
        uint64 product_id,
        bytes16 flow_id,
        bytes32[] calldata instance_keys,
        uint64 lane_chain_id,
        uint8 slot_id,
        uint64 chain_id,
        bytes calldata tx_hash,
        bool completes,
        uint16 part_index,
        uint16 part_count,
        bytes calldata metadata,
        bytes16 link_flow_id,
        bytes32 link_key
    ) external;

    /// @notice Close an open instance explicitly (reverts if it's already closed).
    /// `outcome`: 0 = Completed, 1 = Failed.
    function close_instance(
        uint64 product_id,
        bytes16 flow_id,
        bytes32 instance_key,
        uint8 outcome
    ) external;

    // ---------------------------------------------------------------------
    // Views
    // ---------------------------------------------------------------------

    function get_template(bytes16 name, uint16 version)
        external
        view
        returns (
            bool exists,
            bool investor_scoped,
            uint16 main_required_count,
            SlotDefView[] memory main,
            TrackView[] memory tracks
        );

    /// @notice The template new instances of `(product_id, flow_id)` open with.
    function get_product_flow(uint64 product_id, bytes16 flow_id)
        external
        view
        returns (bool assigned, bytes16 template_name, uint16 template_version);

    /// @param model 0 = Multichain, 1 = SingleChain
    function get_default_flows(uint8 model) external view returns (DefaultFlowView[] memory);

    /// @notice Summary of one instance. `main_stage` is the highest completed
    /// main slot (the instance's current step), named by `stage_name`.
    function get_instance(uint64 product_id, bytes16 flow_id, bytes32 instance_key)
        external
        view
        returns (
            bool exists,
            bytes16 template_name,
            uint16 template_version,
            bytes32 investor,
            uint8 status,
            uint64 opened_at,
            uint64 closed_at,
            uint16 main_required_count,
            uint16 main_done,
            bool has_stage,
            uint8 main_stage,
            string memory stage_name,
            uint16 lane_count,
            uint16 pending_lanes,
            bytes memory metadata,
            InstanceRefView[] memory links
        );

    function get_instance_lanes(uint64 product_id, bytes16 flow_id, bytes32 instance_key)
        external
        view
        returns (LaneView[] memory);

    /// @notice Every recorded slot with its attempts. Gas scales with the bytes
    /// read; reverts past 1,040 slots or 2 MiB — use `get_slot` for one slot.
    function get_instance_slots(uint64 product_id, bytes16 flow_id, bytes32 instance_key)
        external
        view
        returns (SlotView[] memory);

    /// @notice One slot with its attempts. `lane_chain_id` = 0 for a main slot.
    function get_slot(
        uint64 product_id,
        bytes16 flow_id,
        bytes32 instance_key,
        uint64 lane_chain_id,
        uint8 slot_id
    ) external view returns (bool exists, SlotView memory slot);

    /// @notice Open instances, in storage order (not chronological). `limit` <= 128.
    function get_investor_active(
        bytes32 investor,
        uint64 product_id,
        bytes16 flow_id,
        uint256 offset,
        uint256 limit
    ) external view returns (bytes32[] memory instance_keys, uint256 total);

    /// @notice Newest instance `investor` opened with `scope` (closed ones
    /// included), or `bytes32(0)`. For whitelist: the latest nonce for a vault.
    function get_latest_by_scope(bytes32 investor, uint64 product_id, bytes16 flow_id, bytes32 scope)
        external
        view
        returns (bytes32 instance_key);

    /// @notice Closed instances, most recent first. `limit` <= 128.
    function get_investor_history(
        bytes32 investor,
        uint64 product_id,
        bytes16 flow_id,
        uint256 offset,
        uint256 limit
    ) external view returns (bytes32[] memory instance_keys, uint256 total);

    /// @notice Instances linking to this one (a settlement's requests), most
    /// recent first. `limit` <= 128.
    function get_linked_from(
        uint64 product_id,
        bytes16 flow_id,
        bytes32 instance_key,
        uint256 offset,
        uint256 limit
    ) external view returns (InstanceRefView[] memory refs, uint256 total);
}
