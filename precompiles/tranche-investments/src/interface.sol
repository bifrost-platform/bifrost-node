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
        uint256 product_id,
        uint256 request_id,
        uint256 settlement_id,
        uint64 vault_chain_id,
        address vault_address,
        address investor_address,
        uint256 amount,
        uint8 order_type
    );
    event InvestmentApproved(
        uint256 product_id,
        uint256 request_id,
        uint256 settlement_id,
        Allocation[] allocations,
        uint256 claimable_assets
    );
    event AdapterValuationsRecorded(
        uint256 product_id,
        uint256 settlement_id,
        AdapterValuation[] valuations
    );
    event TrancheSettlementRecorded(
        uint256 product_id,
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
     * @param request_id       Unique request identifier, generated by the Valuation Contract
     * @param settlement_id    Valuation Contract's settlement cycle in effect at request time
     * @param vault_chain_id   EVM chain ID of the chain where the tranche vault is deployed
     * @param vault_address    ERC-7540 vault contract address on that chain
     * @param investor_address Investor address on the external chain
     * @param amount           Investor's full requested deposit/redeem amount (18-decimal U256, pre-allocation)
     * @param order_type       0 = redeem, 1 = deposit
     */
    function record_investment_request(
        uint256 product_id,
        uint256 request_id,
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
     *         can claim as a result.
     * @dev Only callable by the calling product's registered Valuation contract
     *      address. Called exactly once per request_id — `allocations` carries
     *      the full allocation breakdown in one call, so the pallet can mint/pay
     *      out immediately without needing to detect "last slice received."
     *      The sum of `allocations[i].amount` is expected to equal the original
     *      request's amount (any portion deliberately held back as reserve
     *      should still be accounted for by an explicit entry, not omitted).
     *      `order_type` is not a parameter here — it was already recorded
     *      against `request_id` by record_investment_request and is looked up
     *      from the pending entry rather than passed again; `claimable_assets`
     *      is interpreted using that stored order_type: the minted share-token
     *      amount for a deposit request, or the released underlying-asset
     *      amount for a redeem request.
     *      Moves the entry from RequestedInvestments to ApprovedInvestments.
     *      Emits InvestmentApproved on success.
     * @param product_id       The product this approval belongs to
     * @param request_id       The pending request being approved
     * @param settlement_id    Valuation Contract's settlement cycle this approval settles in
     * @param allocations      Per-Adapter allocation breakdown of the request's amount
     * @param claimable_assets Finalized claimable amount for the investor — shares (deposit) or assets (redeem), per the request's order_type
     */
    function record_investment_approval(
        uint256 product_id,
        uint256 request_id,
        uint256 settlement_id,
        Allocation[] calldata allocations,
        uint256 claimable_assets
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
     *      part of `record_tranche_settlement` below.
     *      Emits AdapterValuationsRecorded on success.
     * @param product_id     The product this settlement belongs to
     * @param settlement_id  Valuation Contract's settlement cycle this info belongs to
     * @param valuations     Per-Adapter NAV breakdown for this settlement, one entry per Adapter
     */
    function record_adapter_valuations(
        uint256 product_id,
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
    function record_tranche_settlement(
        uint256 product_id,
        uint256 settlement_id,
        TrancheSettle[] calldata tranches,
        uint256 pending_deposit_assets,
        uint256 product_nav
    ) external;

    /// @param chain_id      EVM chain ID where the tranche's ERC-7540 vault is deployed
    /// @param vault_address ERC-7540 vault contract address identifying the tranche
    struct VaultInput {
        uint64 chain_id;
        address vault_address;
    }

    /**
     * @notice Read a product's current settlement_id — the value most recently passed to
     *         record_tranche_settlement.
     * @dev This pallet never generates or increments settlement_id itself — the Valuation
     *      Contract remains the source of truth for assignment; this just echoes back the
     *      last value it recorded. Zero if the product has never settled yet.
     * @param product_id The product to look up
     */
    function get_settlement_id(
        uint256 product_id
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
        uint256 product_id,
        uint256 settlement_id,
        uint256 offset,
        uint256 limit
    ) external view returns (uint256[] memory request_ids);

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
        uint256 product_id,
        uint256 request_id
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
        uint256 product_id,
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
        uint256 product_id
    ) external view returns (uint256);

    /**
     * @notice Read a product's most recently recorded settlement in full: its
     *         settlement_id, each tranche's share price and NAV, and the product's
     *         finalized aggregate NAV.
     * @dev Reverts if the product has no recorded settlement yet. `share_prices`/
     *      `tranche_navs` are ordered by the product's CURRENT tranche priority order
     *      (TrancheSystem.get_tranches' order), not by whatever order Valuation happened
     *      to submit `tranches` in when it called record_tranche_settlement — reverts if
     *      a currently-registered tranche is missing from the latest settlement's
     *      tranches array (a partial settlement can't be summarized this way).
     * @param product_id The product to look up
     */
    function get_last_settlement(
        uint256 product_id
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
     * @notice Read a request's approval details.
     * @dev Reverts if no approval is recorded for `request_id`.
     * @param product_id The product the request belongs to
     * @param request_id The request to look up
     * @return settlement_id      Settlement cycle this approval settled in
     * @return claimable_assets   Finalized claimable amount for the investor
     * @return status             Always 1 (approved) — a distinct status value from
     *                            get_request's 0/1 pair only exists there, not here
     */
    function get_approval(
        uint256 product_id,
        uint256 request_id
    )
        external
        view
        returns (uint256 settlement_id, uint256 claimable_assets, uint8 status);
}
