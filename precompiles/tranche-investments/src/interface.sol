// SPDX-License-Identifier: GPL-3.0-only
pragma solidity >=0.8.0;

/**
 * @title Investments Precompile Interface (tranche-system draft)
 * @notice Called exclusively by the product's registered Valuation contract
 *         (the `valuation.address` set in pallet-tranche-system's `create_product`) —
 *         there is no Gateway in this call path. This pallet is a pure ledger —
 *         all pricing and allocation decisions are made by the Valuation Contract;
 *         this interface only records those decisions into pallet-tranche-investments.
 *
 * Address: 0x0000000000000000000000000000000000000201
 */
interface Investments {
    /// @dev One slice of a request's approval: `amount` was allocated to the
    ///      MultichainAdapter identified by (`adapter_address`, `adapter_chain_id`).
    struct Allocation {
        address adapter_address;
        uint64 adapter_chain_id;
        uint256 amount;
    }

    /// @dev One entry of record_investment_approvals' batch input — the same three
    ///      fields record_investment_approval takes per call, bundled so a Valuation
    ///      Contract that resolves an entire settlement's approvals in one pass can
    ///      record all of them in a single tx instead of one call per request_id.
    /// @param request_id        The pending request being approved
    /// @param allocations       Per-Adapter allocation breakdown of the request's amount
    /// @param receivable_amount Finalized receivable amount for the investor — shares
    ///                          (deposit) or assets (redeem), per the request's order_type
    struct InvestmentApprovalInput {
        bytes32 request_id;
        Allocation[] allocations;
        uint256 receivable_amount;
    }

    /// @dev Adapter-level NAV breakdown for one settlement — one entry per Adapter within a
    ///      product. Field naming/shape intentionally mirrors the node's own base valuation
    ///      record format (chainId, adapter, epochId, valuationCutoff), not this file's usual
    ///      snake_case convention, so the pallet-side type and any off-chain indexer consuming
    ///      both share field semantics 1:1.
    /// @param chainId         EVM chain ID the adapter lives on — combined with `adapter`, this
    ///                        is the adapter's global identity (chainId, adapter)
    /// @param adapter         Adapter contract address on that chain
    /// @param epochId         Adapter's own local epoch/settlement counter
    /// @param valuationCutoff Timestamp this valuation was struck as-of
    /// @param principal       Cumulative principal deployed into this adapter
    /// @param positions       Per-asset breakdown of this adapter's current holdings
    struct AdapterValuation {
        uint64 chainId;
        address adapter;
        uint256 epochId;
        uint64 valuationCutoff;
        uint256 principal;
        AssetPosition[] positions;
    }

    /// @param asset    Asset's token address on `chainId` (native to that chain, not a Hub address)
    /// @param amount   Held amount, in `asset`'s own decimals
    /// @param priceUsd Price at valuation time, FixedU128-style 1e18 fixed-point (sourced from
    ///                 the Adapter's own NAV oracle, not this pallet)
    /// @param usdValue `amount * priceUsd`, recorded alongside `amount`/`priceUsd` rather than
    ///                 only stored, so downstream consumers can re-derive and cross-check it
    ///                 without re-fetching price data
    /// @param counted  Whether this position counts toward the adapter's NAV — pre-swap reward
    ///                 tokens are `false` (see design doc §4)
    struct AssetPosition {
        address asset;
        uint256 amount;
        uint256 priceUsd;
        uint256 usdValue;
        bool counted;
    }

    /// @param chain_id      EVM chain ID where the tranche's ERC-7540 vault is deployed
    /// @param vault_address ERC-7540 vault contract address identifying the tranche
    struct VaultInput {
        uint64 chain_id;
        address vault_address;
    }

    /// @dev Post-waterfall settlement result for a single tranche, one entry per tranche within
    ///      a product. Distinct from AdapterValuation.principal (that's capital deployed into
    ///      one yield source; this is a tranche's own Senior principal claim) — neither
    ///      substitutes for the other.
    /// @param vault_chain_id     EVM chain ID where the tranche's ERC-7540 vault is deployed
    /// @param vault_address      ERC-7540 vault contract address identifying the tranche
    /// @param tranche_nav        This tranche's NAV after the waterfall, in the product's base asset
    /// @param share_price        Tranche share-token price, FixedU128-style 1e18 fixed-point
    /// @param units_outstanding  Tranche share-token total supply after this settlement
    /// @param principal          Senior-tranche principal claim — pass zero for Junior, same
    ///                           Senior-only convention as TrancheSystem's TrancheInput.apr
    struct TrancheSettle {
        uint64 vault_chain_id;
        address vault_address;
        uint256 tranche_nav;
        uint256 share_price;
        uint256 units_outstanding;
        uint256 principal;
    }

    event InvestmentRequested(
        uint64 product_id,
        bytes32 request_id,
        uint256 settlement_id,
        uint64 vault_chain_id,
        address vault_address,
        address investor_address,
        uint256 amount,
        uint8 order_type
    );
    event InvestmentApproved(
        uint64 product_id,
        bytes32 request_id,
        uint256 settlement_id,
        Allocation[] allocations,
        uint256 receivable_amount
    );
    event AdapterValuationsRecorded(
        uint64 product_id,
        uint256 settlement_id,
        AdapterValuation[] valuations
    );
    event TrancheSettlementRecorded(
        uint64 product_id,
        uint256 settlement_id,
        uint256 pending_deposit_assets,
        uint256 product_nav
    );

    /**
     * @notice Record a pending deposit or redeem request as it arrives at the
     *         Valuation Contract, before any Adapter allocation has happened.
     * @dev Only callable by the calling product's registered Valuation contract
     *      address — `msg.sender` must equal the `valuation.address` stored for
     *      `product_id` in pallet-tranche-system.
     *      Stores the full requested amount under `request_id` in
     *      RequestedInvestments. Emits InvestmentRequested on success.
     * @param product_id       The product this request belongs to
     * @param request_id       Unique request identifier, generated by TrancheManager at
     *                         request time (before this request ever reaches the Valuation
     *                         Contract)
     * @param settlement_id    Valuation Contract's settlement cycle in effect at request time
     * @param vault_chain_id   EVM chain ID of the chain where the tranche vault is deployed
     * @param vault_address    ERC-7540 vault contract address on that chain
     * @param investor_address Investor address on the external chain
     * @param amount           Investor's full requested deposit/redeem amount (18-decimal U256, pre-allocation)
     * @param order_type       0 = redeem, 1 = deposit
     */
    function record_investment_request(
        uint64 product_id,
        bytes32 request_id,
        uint256 settlement_id,
        uint64 vault_chain_id,
        address vault_address,
        address investor_address,
        uint256 amount,
        uint8 order_type
    ) external;

    /**
     * @notice Record a pending request's full approval: the complete breakdown
     *         of how it was allocated across Adapters, plus what the investor
     *         can receive as a result.
     * @dev Only callable by the calling product's registered Valuation contract
     *      address. Called exactly once per request_id — `allocations` carries
     *      the full allocation breakdown in one call, so the pallet can mint/pay
     *      out immediately without needing to detect "last slice received."
     *      The sum of `allocations[i].amount` is expected to equal the original
     *      request's amount (any portion deliberately held back as reserve
     *      should still be accounted for by an explicit entry, not omitted).
     *      `order_type` is not a parameter here — it was already recorded
     *      against `request_id` by record_investment_request and is looked up
     *      from the pending entry rather than passed again; `receivable_amount`
     *      is interpreted using that stored order_type: the minted share-token
     *      amount for a deposit request, or the released underlying-asset
     *      amount for a redeem request.
     *      Moves the entry from RequestedInvestments to ApprovedInvestments.
     *      Emits InvestmentApproved on success.
     * @param product_id        The product this approval belongs to
     * @param request_id        The pending request being approved
     * @param settlement_id     Valuation Contract's settlement cycle this approval settles in
     * @param allocations       Per-Adapter allocation breakdown of the request's amount
     * @param receivable_amount Finalized receivable amount for the investor — shares (deposit) or assets (redeem), per the request's order_type
     */
    function record_investment_approval(
        uint64 product_id,
        bytes32 request_id,
        uint256 settlement_id,
        Allocation[] calldata allocations,
        uint256 receivable_amount
    ) external;

    /**
     * @notice Batch form of record_investment_approval — records every entry in
     *         `approvals` against the same (product_id, settlement_id) in one tx, for a
     *         Valuation Contract that resolves an entire settlement's approvals in one
     *         pass rather than one call per request_id.
     * @dev Only callable by the calling product's registered Valuation contract address.
     *      Does the exact same per-entry work record_investment_approval does (allocation
     *      validation, RequestedInvestments -> ApprovedInvestments move, SettlementRequests
     *      append), repeated once per `approvals` entry — record_investment_approval itself
     *      is unchanged and still the right call for a single request_id.
     *      Atomic like any other tx: if any entry fails, the whole batch reverts, including
     *      entries already applied earlier in the same call. A duplicate request_id within
     *      the same batch fails on its second occurrence (RequestNotFound) — same outcome
     *      as calling record_investment_approval twice for the same request_id.
     *      Emits one InvestmentApproved per entry, in the same shape a caller would see
     *      from record_investment_approval, so indexers don't need to special-case this
     *      batch entry point.
     * @param product_id    The product these approvals belong to
     * @param settlement_id Valuation Contract's settlement cycle these approvals settle in
     * @param approvals     One entry per request being approved — see InvestmentApprovalInput
     */
    function record_investment_approvals(
        uint64 product_id,
        uint256 settlement_id,
        InvestmentApprovalInput[] calldata approvals
    ) external;

    /**
     * @notice Record the finalized per-Adapter NAV breakdown for a settlement, once the
     *         Valuation Contract has completed that settlement cycle.
     * @dev Only callable by the product's registered Valuation contract address.
     *      Renamed from `record_settlement_info` (2026-07-28), and `nav_infos` changed from
     *      `bytes[]` to `AdapterValuation[]` — the old opaque-bytes shape existed to let the
     *      payload vary by a leading `structure_type` discriminant byte without changing this
     *      function's signature; now that the shape is a fixed struct (one entry per Adapter),
     *      that flexibility is gone, so the name narrows to match what this call actually
     *      records. The settlement's aggregate total is recorded separately, as
     *      part of `record_settlement` below.
     *      Emits AdapterValuationsRecorded on success.
     * @param product_id     The product this settlement belongs to
     * @param settlement_id  Valuation Contract's settlement cycle this info belongs to
     * @param valuations     Per-Adapter NAV breakdown for this settlement, one entry per Adapter
     */
    function record_adapter_valuations(
        uint64 product_id,
        uint256 settlement_id,
        AdapterValuation[] calldata valuations
    ) external;

    /**
     * @notice Record the post-waterfall per-tranche settlement result for a settlement cycle:
     *         each tranche's NAV/share price/units/principal, the product's pending
     *         (unconfirmed) deposit total, and the product's finalized aggregate NAV.
     * @dev Only callable by the product's registered Valuation contract address. Callable at
     *      most once per (product_id, settlement_id) — same as record_adapter_valuations.
     *      Folds what used to be the separate record_product_nav call in here, so both are
     *      recorded atomically in one transaction: `product_nav` is the finalized total across
     *      all of the product's sources (both OnchainSource adapters, read live by Valuation,
     *      and OffchainSource adapters, fed via pallet-rwa-nav-oracle) — the aggregate figure
     *      Valuation actually used for this settlement's waterfall/share-price computation. Not
     *      independently verified by this pallet against `record_adapter_valuations`'s
     *      breakdown — Valuation is trusted for the aggregation, same as it's trusted for every
     *      other value in this interface.
     *      `tranches[i].units_outstanding`/`tranches[i].principal` are overwritten wholesale by
     *      each new settlement — record_investment_approval does not separately accumulate
     *      tranche-level totals, so this call is the sole source of truth for them going
     *      forward, read back by the next settlement cycle.
     *      Emits TrancheSettlementRecorded on success.
     * @param product_id              The product this settlement belongs to
     * @param settlement_id           Valuation Contract's settlement cycle this result belongs to
     * @param tranches                Post-waterfall result for every tranche, one entry per tranche
     * @param pending_deposit_assets  Product-level pending/unconfirmed deposit amount as of this
     *                                settlement — not yet reflected in any tranche's units_outstanding
     * @param product_nav             Finalized total NAV across all sources for this settlement
     *                                (sum, not per-source)
     */
    function record_settlement(
        uint64 product_id,
        uint256 settlement_id,
        TrancheSettle[] calldata tranches,
        uint256 pending_deposit_assets,
        uint256 product_nav
    ) external;

    /**
     * @notice Read a product's current settlement_id — the value most recently passed to
     *         record_settlement.
     * @dev This pallet never generates or increments settlement_id itself — the Valuation
     *      Contract remains the source of truth for assignment; this just echoes back the
     *      last value it recorded. Zero if the product has never settled yet — settlement_id
     *      is 1-indexed protocol-wide (the Valuation Contract's first real settlement is 1,
     *      never 0) precisely so 0 stays free as this "never settled" sentinel, here and
     *      everywhere else in this interface that reads settlement_id back (e.g. get_request,
     *      get_approval).
     * @param product_id The product to look up
     */
    function get_settlement_id(
        uint64 product_id
    ) external view returns (uint256);

    /**
     * @notice Enumerate pending (unapproved) request IDs for a product, scoped to one
     *         settlement batch, with offset/limit pagination.
     * @dev `settlement_id` scopes to requests recorded (via record_investment_request)
     *      against that settlement cycle specifically — lets a caller process only the
     *      current batch and leave requests carried over from an earlier, still-unapproved
     *      batch for later.
     * @param product_id    The product to look up
     * @param settlement_id Only requests recorded against this settlement cycle are returned
     * @param offset        Number of matching entries to skip
     * @param limit         Maximum number of entries to return
     */
    function get_pending_requests(
        uint64 product_id,
        uint256 settlement_id,
        uint256 offset,
        uint256 limit
    ) external view returns (bytes32[] memory request_ids);

    /**
     * @notice Read a single request's current state — pending or approved.
     * @dev Reverts if no request exists for `request_id` (neither pending nor approved).
     *      `settlement_id` is the request-time cycle while pending, or the approval-time
     *      cycle once approved (these can differ — see record_investment_approval notes).
     * @param product_id The product the request belongs to
     * @param request_id The request to look up
     * @return investor           Investor address on the external chain
     * @return vault_chain_id     EVM chain ID of the tranche vault this request targets
     * @return vault              ERC-7540 vault contract address on that chain
     * @return amount             Investor's full requested amount (pre-allocation)
     * @return settlement_id      See dev notes above
     * @return order_type         0 = redeem, 1 = deposit
     * @return status             0 = pending, 1 = approved
     */
    function get_request(
        uint64 product_id,
        bytes32 request_id
    )
        external
        view
        returns (
            address investor,
            uint64 vault_chain_id,
            address vault,
            uint256 amount,
            uint256 settlement_id,
            uint8 order_type,
            uint8 status
        );

    /**
     * @notice Read a tranche's outstanding units and Senior principal claim, as of the
     *         product's most recently recorded settlement.
     * @dev Reverts if the product has no recorded settlement yet, or if `tranche` wasn't
     *      part of the latest settlement's tranches array.
     * @param product_id The product the tranche belongs to
     * @param tranche    The tranche's identifying vault
     * @return units_outstanding Tranche share-token total supply as of the latest settlement
     * @return principal         Senior-tranche principal claim (zero for Junior)
     */
    function get_tranche_state(
        uint64 product_id,
        VaultInput calldata tranche
    ) external view returns (uint256 units_outstanding, uint256 principal);

    /**
     * @notice Read a product's pending (unconfirmed) deposit total, as of the product's
     *         most recently recorded settlement.
     * @dev Zero if the product has never settled yet — distinct from "settled with zero
     *      pending deposits," but indistinguishable from it by this call alone (pair with
     *      get_settlement_id if that distinction matters to the caller).
     * @param product_id The product to look up
     */
    function get_pending_deposit_assets(
        uint64 product_id
    ) external view returns (uint256);

    /**
     * @notice Read a product's most recently recorded settlement in full: its
     *         settlement_id, each tranche's share price and NAV, and the product's
     *         finalized aggregate NAV.
     * @dev Reverts if the product has no recorded settlement yet. `share_prices`/
     *      `tranche_navs` are ordered by the product's CURRENT tranche priority order
     *      (TrancheSystem.get_tranches' order), not by whatever order Valuation happened
     *      to submit `tranches` in when it called record_settlement — reverts if
     *      a currently-registered tranche is missing from the latest settlement's
     *      tranches array (a partial settlement can't be summarized this way).
     * @param product_id The product to look up
     */
    function get_last_settlement(
        uint64 product_id
    )
        external
        view
        returns (
            uint256 settlement_id,
            uint256[] memory share_prices,
            uint256[] memory tranche_navs,
            uint256 product_nav
        );

    /**
     * @notice Read one specific settlement's full state — unlike get_last_settlement
     *         (which only ever reads the product's most recent one), this looks up any
     *         settlement_id that's ever been recorded for product_id.
     * @dev Returns the raw per-tranche breakdown (TrancheSettle[], keyed by vault rather
     *      than pre-matched against pallet-tranche-system's tranche ordering the way
     *      get_last_settlement's share_prices/tranche_navs arrays are) alongside
     *      pending_deposit_assets, product_nav, and both write-time fields
     *      (recorded_at/timestamp — recorded_at is this chain's own block number,
     *      timestamp is pallet_timestamp's value in ms since Unix epoch at that same
     *      moment; stored separately since a block number alone doesn't let a caller
     *      compute a real-world date without also knowing this chain's block time).
     *      Reverts if no settlement with this settlement_id was ever recorded for
     *      product_id — same convention as get_last_settlement.
     * @param product_id    The product the settlement belongs to
     * @param settlement_id The settlement to look up
     * @return tranches Per-tranche breakdown, one entry per tranche settled
     * @return pending_deposit_assets Product-level pending/unconfirmed deposit total as of
     *                                this settlement
     * @return product_nav Product's finalized aggregate NAV as of this settlement
     * @return recorded_at This chain's own block number when this settlement was recorded
     * @return timestamp This chain's pallet_timestamp value (ms since Unix epoch) at the
     *                    same moment as recorded_at
     */
    function get_settlement_state(
        uint64 product_id,
        uint256 settlement_id
    )
        external
        view
        returns (
            TrancheSettle[] memory tranches,
            uint256 pending_deposit_assets,
            uint256 product_nav,
            uint256 recorded_at,
            uint256 timestamp
        );

    /**
     * @notice Read one settlement's full per-Adapter NAV breakdown, as recorded by
     *         record_adapter_valuations — one entry per Adapter, each with its own
     *         per-asset position breakdown.
     * @dev No other function exposes this; it's only otherwise observable via
     *      AdapterValuationsRecorded. Reverts if no Adapter valuations were ever recorded
     *      for this (product_id, settlement_id) — same convention as get_settlement_state.
     * @param product_id    The product the settlement belongs to
     * @param settlement_id The settlement to look up
     * @return valuations Per-Adapter NAV breakdown, one entry per Adapter
     */
    function get_adapter_valuations(
        uint64 product_id,
        uint256 settlement_id
    ) external view returns (AdapterValuation[] memory valuations);

    /**
     * @notice Read a request's approval details.
     * @dev Reverts if no approval is recorded for `request_id`.
     * @param product_id The product the request belongs to
     * @param request_id The request to look up
     * @return settlement_id      Settlement cycle this approval settled in
     * @return receivable_amount  Finalized receivable amount for the investor
     * @return status             Always 1 (approved) — a distinct status value from
     *                            get_request's 0/1 pair only exists there, not here
     */
    function get_approval(
        uint64 product_id,
        bytes32 request_id
    )
        external
        view
        returns (
            uint256 settlement_id,
            uint256 receivable_amount,
            uint8 status
        );
}
