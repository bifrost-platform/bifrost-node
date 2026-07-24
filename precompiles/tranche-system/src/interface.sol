// SPDX-License-Identifier: GPL-3.0-only
pragma solidity >=0.8.0;

/**
 * @title Tranche System Precompile Interface (tranche-system draft)
 * @notice Owns product/tranche/adapter configuration for the OmniFi tranche-system
 *         model. Replaces the old single-source pallet-pools — a "product" here can
 *         be backed by multiple yield sources (Adapters) at once, priced by a single
 *         Hub-chain Valuation Contract registered per product.
 *
 * DRAFT — reflects the tranche-system pivot (2026-07-23), not yet locked in. pallet-
 * tranche-system does not exist yet; this interface is written ahead of the pallet so
 * the Solidity-facing shape can be reviewed first (see [[project_omnifi_revamp_callflow]]
 * in project memory for the full spec this was drafted from).
 *
 *   - Address reassignment as part of the pivot: the old Investments precompile
 *     moved from 0x...0200 to 0x...0201 (see precompiles/investments/src/interface.sol),
 *     vacating 0x...0200 for this new precompile. pallet-pools/its precompile (still
 *     at 0x...0201) is retired once pallet-tranche-system fully replaces it — the two
 *     addresses aren't reused 1:1 from pools, they're a three-way rotation.
 *   - `TrancheInput.apr` follows the same convention as the old Pools precompile's
 *     TrancheInput: only meaningful when `tranche_type == Senior` (fixed APR
 *     entitlement); Junior gets the residual/variable yield after the waterfall.
 *   - A tranche is identified by its `VaultInput` (chain_id, vault_address) — its
 *     ERC-7540 vault — not by `tranche_type`. A product can register more than one
 *     vault under the same `tranche_type` (e.g. Senior vaults on multiple Spoke
 *     chains feeding the same waterfall slot).
 *   - `priority` is a single ordered sequence across ALL of a product's tranches
 *     (not scoped per tranche_type) — confirmed by remove_tranche's behavior
 *     ("뒷 우선순위 트랜치는 하나씩 앞으로 당겨짐", i.e. every tranche after the
 *     removed one shifts up by one). 0 = highest priority (paid first in the
 *     waterfall). Inserting at an occupied priority (via set_tranche's `priority`
 *     field, on both Add and Update) shifts the existing tranche at that slot,
 *     and everything after it, down by one — this is an insert, not an overwrite.
 *   - `AdapterInput.borrower`/`AdapterInput.collaterals` are only meaningful when
 *     `source_type == OffchainSource` (mirrors the old Pools precompile's
 *     borrower_id/CollateralInput fields, now living per-adapter instead of
 *     per-pool). Both should be left empty/zero for `OnchainSource` entries.
 *   - Two distinct adapter concepts, per the source spec:
 *     `adapters` (this interface's `AdapterInput`) are individual single-yield-
 *     source registrations (an offchain RWA loan book, or one onchain money
 *     market) and carry no weight of their own — add/remove only, no update.
 *     `multichain_adapters` (`MultichainAdapterInput`) are the Hub-chain
 *     MultichainAdapter contract instances Valuation actually calls
 *     (`executeDeposit`/`collectEachNAV` etc. in the call-flow spec) and carry
 *     the `weight` Valuation uses to decide its top-level capital-distribution
 *     ratio. Weight can also exist *within* a MultichainAdapter (how it splits
 *     its allocation across the individual `adapters`/protocols it manages,
 *     e.g. Compound vs. Morpho vs. Aave), but that's the MultichainAdapter
 *     contract's own internal concern — Valuation, and therefore this pallet,
 *     only tracks the outer per-MultichainAdapter weight. This is why
 *     `AdapterInput` has no `weight` field.
 *   - `weight` (multichain adapter) and `apr` (Senior tranche) both use the same
 *     FixedU128-inner-value convention as the old Pools precompile: 1e18 = 100%.
 *   - A product's `multichain_adapters` weights must always sum to exactly 100%
 *     (1e18) — this is a hard invariant, not advisory (Valuation's entire
 *     capital-distribution decision is driven by this table). Because of that,
 *     `set_multichain_adapters` (see below) takes the full intended end-state
 *     list and replaces it atomically, rather than single-entity add/remove/
 *     update calls — a single-entity mutation can't preserve a cross-entry sum
 *     invariant without either silently rescaling every other entry (surprising
 *     side effect) or leaving the sum temporarily wrong between calls (a real
 *     fund-safety risk if Valuation acts on a stale/incomplete state).
 *   - Caller authorization is NOT modeled by a Gateway here — per
 *     pallet-tranche-permissions, the caller must hold the relevant role
 *     (e.g. ProductAdmin) for `product_id`, granted via that pallet's
 *     grant_permission/revoke_permission.
 *
 * Address: 0x0000000000000000000000000000000000000200
 */
interface TrancheSystem {
    enum TrancheType {
        Junior,
        Senior
    }

    enum SourceType {
        OffchainSource,
        OnchainSource
    }

    /// @dev Discriminant for the unified set_tranche/set_adapter mutation functions.
    ///      `Update` is not meaningful for set_adapter (adapters have no mutable
    ///      field) and must revert.
    enum CrudAction {
        Add,
        Remove,
        Update
    }

    /// @param nft_contract ERC-721 contract address of the collateral
    /// @param nft_token_id Token ID of the collateral NFT
    struct CollateralInput {
        address nft_contract;
        uint256 nft_token_id;
    }

    /// @param valuation_address      Hub-chain Valuation contract address for this product
    /// @param settlement_length_secs Settlement interval length; admin-set, recommended to be
    ///                                at least the GCD of the underlying yield sources' epochs
    /// @param settlement_offset_secs Window within each interval during which pallet-auto-pilot
    ///                                repeatedly calls Valuation.tryUpdateNAV() to trigger settlement
    struct ValuationInput {
        address valuation_address;
        uint64 settlement_length_secs;
        uint64 settlement_offset_secs;
    }

    /// @param chain_id      EVM chain ID where the tranche's ERC-7540 vault is deployed
    /// @param vault_address ERC-7540 vault contract address identifying this tranche
    struct VaultInput {
        uint64 chain_id;
        address vault_address;
    }

    /// @param tranche_type Junior or Senior
    /// @param apr          Fixed APR as a FixedU128 inner value (1e18 = 100%); Senior-only, see notes above
    /// @param vault        The ERC-7540 vault identifying this tranche
    /// @param priority     Waterfall priority within the product; 0 = highest priority, see notes above
    struct TrancheInput {
        TrancheType tranche_type;
        uint256 apr;
        VaultInput vault;
        uint8 priority;
    }

    /// @param adapter_address Hub-chain MultichainAdapter contract address
    /// @param chain_id        EVM chain ID this MultichainAdapter routes capital to
    /// @param weight          Allocation weight Valuation uses for its top-level distribution, FixedU128 inner (1e18 = 100%)
    struct MultichainAdapterInput {
        address adapter_address;
        uint64 chain_id;
        uint256 weight;
    }

    /// @param source_type    OffchainSource or OnchainSource
    /// @param source_address Yield source's own address (e.g. an onchain money-market address);
    ///                        for OffchainSource this identifies the RWA loan book instance
    /// @param chain_id       EVM chain ID where the yield source lives
    /// @param borrower       OffchainSource only: institution's EVM address; zero address otherwise
    /// @param collaterals    OffchainSource only: collateral NFTs backing the loan book; empty otherwise
    struct AdapterInput {
        SourceType source_type;
        address source_address;
        uint64 chain_id;
        address borrower;
        CollateralInput[] collaterals;
    }

    /// @dev `product_admin` is not a function input on create_product (see below) —
    ///      it's the caller's own EVM address, already proven to hold ProductAdmin
    ///      for `product_id` by the precompile before dispatch. Included here only
    ///      for indexers/observability.
    event ProductCreated(
        uint256 product_id,
        address product_admin,
        address valuation_address,
        uint64 settlement_length_secs,
        uint64 settlement_offset_secs
    );
    event TrancheSet(
        uint256 product_id,
        CrudAction action,
        TrancheType tranche_type,
        uint256 apr,
        uint64 vault_chain_id,
        address vault_address,
        uint8 priority
    );
    event AdapterSet(
        uint256 product_id,
        CrudAction action,
        SourceType source_type,
        address source_address,
        uint64 chain_id
    );
    event MultichainAdaptersSet(
        uint256 product_id,
        MultichainAdapterInput[] multichain_adapters
    );

    /**
     * @notice Create a new tranche-system product: its Valuation contract binding,
     *         its tranches, its MultichainAdapter routing table, and its individual
     *         yield-source Adapter registrations.
     * @dev Flow mirrors old pallet-pools: sudo grants ProductAdmin for `product_id`
     *      via pallet-tranche-permissions BEFORE this is ever called (`product_id`
     *      is reserved to an admin up front, not decided here) — that admin then
     *      calls create_product using the `product_id` they were issued. Caller
     *      identity is therefore NOT a function input: the precompile itself
     *      verifies `handle.context().caller` holds ProductAdmin for `product_id`
     *      before dispatch and only then constructs the ProductAdmin origin, so
     *      this function trusts that check rather than re-taking the admin address
     *      as a parameter.
     *      Reverts if `product_id` is already taken, `tranches` is empty, or
     *      weights across `multichain_adapters` don't sum to 100% (1e18).
     *      Emits ProductCreated on success.
     * @param product_id          Hub product ID (already granted to the caller via ProductAdmin)
     * @param valuation           Valuation contract binding + settlement cadence config
     * @param tranches            Array of tranche configurations (each identified by its vault)
     * @param multichain_adapters Array of MultichainAdapter routing entries (address, chain_id, weight)
     * @param adapters            Array of individual yield-source Adapter registrations
     */
    function create_product(
        uint256 product_id,
        ValuationInput calldata valuation,
        TrancheInput[] calldata tranches,
        MultichainAdapterInput[] calldata multichain_adapters,
        AdapterInput[] calldata adapters
    ) external;

    /**
     * @notice Add, remove, or update a tranche on an existing product, identified
     *         by its vault (chain_id, vault_address).
     * @dev Caller must hold the ProductAdmin role for `product_id`.
     *      Field usage differs by `action` — unused fields are ignored, but callers
     *      must still supply the full struct (e.g. pass zero/default values for
     *      `tranche_type`/`apr`/`priority` on a `Remove` call):
     *        - Add:    `tranche.vault` becomes the new tranche's identity (reverts if
     *                  a tranche with the same vault already exists for this product).
     *                  `tranche_type`, `apr` (Senior-only), and `priority` are used.
     *                  If `priority` is already occupied, the existing tranche at that
     *                  slot (and everything after it) shifts down by one.
     *        - Remove: only `tranche.vault` is used, to identify which tranche to
     *                  remove (reverts if not found, or if it has outstanding
     *                  investments). Every tranche with a lower priority ranking
     *                  (higher numeric value) than the removed one shifts up by
     *                  one, closing the gap.
     *        - Update: `tranche.vault` identifies which tranche to update (reverts
     *                  if not found); `apr` and `priority` are applied as new values.
     *                  `tranche_type` is ignored — a tranche's type is fixed at Add
     *                  and cannot be changed. If `priority` differs from the
     *                  tranche's current priority, it re-inserts using the same
     *                  shift semantics as Add.
     *      Emits TrancheSet on success.
     * @param product_id The product whose tranche is being mutated
     * @param action     Add, Remove, or Update
     * @param tranche    The tranche data; see field usage per `action` above
     */
    function set_tranche(
        uint256 product_id,
        CrudAction action,
        TrancheInput calldata tranche
    ) external;

    /**
     * @notice Add or remove an individual yield-source Adapter on an existing
     *         product, identified by (`source_address`, `chain_id`).
     * @dev Caller must hold the ProductAdmin role for `product_id`.
     *        - Add:    registers a new adapter (reverts if one with the same
     *                  (`source_address`, `chain_id`) already exists for this
     *                  product). `source_type`, `borrower`, and `collaterals` are used.
     *        - Remove: only `source_address`/`chain_id` are used, to identify which
     *                  adapter to remove.
     *        - Update: NOT SUPPORTED — reverts. Adapters have no mutable field
     *                  (unlike multichain_adapters, they carry no weight); once
     *                  added, an adapter can only be removed and re-added.
     *      Emits AdapterSet on success.
     * @param product_id The product whose adapter is being mutated
     * @param action     Add or Remove (Update reverts)
     * @param adapter    The adapter data; see field usage per `action` above
     */
    function set_adapter(
        uint256 product_id,
        CrudAction action,
        AdapterInput calldata adapter
    ) external;

    /**
     * @notice Replace a product's entire MultichainAdapter routing table atomically.
     * @dev Caller must hold the ProductAdmin role for `product_id`.
     *      Unlike set_tranche/set_adapter, this is NOT a single-entity add/remove/
     *      update call — callers must submit the full intended end-state list every
     *      time, not deltas. This is deliberate: a product's multichain_adapters
     *      weights must always sum to exactly 100% (1e18), and a single-entity
     *      mutation can't preserve that cross-entry invariant without either
     *      silently rescaling every other entry (a surprising side effect) or
     *      leaving the sum temporarily wrong between calls (a real fund-safety risk
     *      if Valuation acts on a stale/incomplete state). A full-array replace
     *      checked atomically avoids both.
     *      Reverts unless `sum(multichain_adapters[i].weight) == 1e18`, or if
     *      `multichain_adapters` contains a duplicate (`adapter_address`, `chain_id`)
     *      pair.
     *      Emits MultichainAdaptersSet on success.
     * @param product_id          The product whose MultichainAdapter table is being replaced
     * @param multichain_adapters The full intended end-state list of routing entries
     */
    function set_multichain_adapters(
        uint256 product_id,
        MultichainAdapterInput[] calldata multichain_adapters
    ) external;
}
