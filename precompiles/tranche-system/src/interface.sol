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
 *   - Every Senior tranche must precede every Junior tranche in priority order —
 *     a hard invariant, not advisory. `create_product` sorts its `tranches` array
 *     by each entry's own `priority` field (NOT array position — see below) and
 *     reverts if that produces a Junior-before-Senior ordering, or if two entries
 *     share a `priority`. `set_tranche`'s Add/Remove/Update all re-check this
 *     invariant on the resulting full list, since any of them can change relative
 *     order. `set_tranche`'s Update additionally cannot change a tranche's
 *     Junior/Senior discriminant at all (reverts if attempted) — only `apr` and
 *     `priority` are mutable there; changing Junior<->Senior requires remove + re-add.
 *   - `AdapterInput.borrower`/`AdapterInput.collaterals` are only meaningful when
 *     `source_type == OffchainSource` (mirrors the old Pools precompile's
 *     borrower_id/CollateralInput fields, now living per-adapter instead of
 *     per-pool). Both should be left empty/zero for `OnchainSource` entries.
 *   - Two distinct adapter concepts, per the source spec:
 *     `adapters` (this interface's `AdapterInput`) are individual single-yield-
 *     source registrations (an offchain RWA loan book, or one onchain money
 *     market) — add/remove only, no update. Nested under their parent
 *     `MultichainAdapterInput` (2026-07-27) rather than a separate flat, product-wide
 *     list — a MultichainAdapter's internal split across the protocols it manages
 *     (e.g. Compound vs. Morpho vs. Aave) is a parent-child relationship, not two
 *     independent registries. As a consequence `AdapterInput` carries no `chain_id`
 *     of its own: a nested adapter always lives on its parent MultichainAdapter's
 *     chain, so `set_adapters` (see below) takes `parent_adapter_address`/
 *     `parent_chain_id` to both identify the parent and supply that implied chain.
 *     `multichain_adapters` (`MultichainAdapterInput`) are the Hub-chain
 *     MultichainAdapter contract instances Valuation actually calls
 *     (`executeDeposit`/`collectEachNAV` etc. in the call-flow spec) and carry
 *     the `weightBps` Valuation uses to decide its top-level capital-distribution
 *     ratio. An Adapter (address, chain_id) can belong to at most one
 *     MultichainAdapter at a time, globally — `create_product`/`set_multichain_adapters`
 *     revert if the same nested Adapter appears under two different parents in
 *     one call, and `set_adapters`/`set_multichain_adapters` both revert if it's
 *     already registered under a *different* parent than the one being written.
 *   - `AdapterInput.weightBps` (added 2026-07-27) represents a given adapter's
 *     sub-allocation weight *within its parent MultichainAdapter's* internal split
 *     (e.g. Compound vs. Morpho vs. Aave) — informational/audit bookkeeping in this
 *     pallet; the earlier draft of this interface omitted it on the reasoning that
 *     this split was purely the MultichainAdapter contract's own internal concern,
 *     but it's tracked here now. Same invariant as `multichain_adapters`' weights
 *     (2026-07-27): a single MultichainAdapter's nested `adapters` weightBps must
 *     always sum to exactly 10_000 — see `set_adapters` for why that's a
 *     full-array replace scoped to one parent, mirroring `set_multichain_adapters`.
 *   - `weightBps`/`apr` unit conventions differ: `apr` (Senior tranche) is a
 *     FixedU128 inner value as in the old Pools precompile (1e18 = 100%);
 *     `weightBps` (both `MultichainAdapterInput` and `AdapterInput`) is
 *     **basis points** (10_000 = 100%), a `uint16`.
 *   - A product's `multichain_adapters` weights must always sum to exactly 100%
 *     (10_000 bps) — this is a hard invariant, not advisory (Valuation's entire
 *     capital-distribution decision is driven by this table). Because of that,
 *     `set_multichain_adapters` (see below) takes the full intended end-state
 *     list and replaces it atomically, rather than single-entity add/remove/
 *     update calls — a single-entity mutation can't preserve a cross-entry sum
 *     invariant without either silently rescaling every other entry (surprising
 *     side effect) or leaving the sum temporarily wrong between calls (a real
 *     fund-safety risk if Valuation acts on a stale/incomplete state).
 *   - Caller authorization is NOT modeled by a Gateway here — per
 *     pallet-tranche-permissions, the caller must hold ProductAdmin for
 *     `product_id`, granted via that pallet's grant_permission/revoke_permission.
 *     Every function on this interface enforces this at the pallet level, not
 *     just here: pallet-tranche-system's extrinsics only accept an origin this
 *     precompile constructs after checking ProductAdmin itself, so calling the
 *     pallet directly (bypassing this precompile) is impossible regardless of
 *     role — there is no signed-origin fallback path.
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

    /// @dev Discriminant for the unified set_tranche mutation function.
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

    /// @param valuation_address           Hub-chain Valuation contract address for this product
    /// @param settlement_start_timestamp  Unix timestamp the first settlement cycle begins; can
    ///                                     be in the future. Every later cycle starts at
    ///                                     settlement_start_timestamp + k * settlement_length_secs.
    ///                                     Purely configuration — read off-chain by the settlement
    ///                                     bot that calls Valuation.tryUpdateNav(); no on-chain
    ///                                     logic acts on it
    /// @param settlement_length_secs      Length of one settlement cycle, in seconds, counted
    ///                                     from settlement_start_timestamp; admin-set, recommended
    ///                                     to be at least the GCD of the underlying yield sources'
    ///                                     cycles
    /// @param settlement_offset_secs      Width, in seconds, of the settlement window at the
    ///                                     *end* of each cycle (e.g. 3600 for a 1-hour window) —
    ///                                     not seconds since the cycle started. Order submission
    ///                                     closes ("market close") when the window opens
    struct ValuationInput {
        address valuation_address;
        uint64 settlement_start_timestamp;
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

    /// @param source_type    OffchainSource or OnchainSource
    /// @param source_address Yield source's own address (e.g. an onchain money-market address);
    ///                        for OffchainSource this identifies the RWA loan book instance.
    ///                        No separate `chain_id` here (removed 2026-07-27) — a nested
    ///                        adapter always lives on its parent MultichainAdapter's `chain_id`,
    ///                        see `MultichainAdapterInput.adapters` and `set_adapters` below
    /// @param weightBps      This adapter's sub-allocation weight within its parent
    ///                       MultichainAdapter's internal split (e.g. Compound vs. Morpho vs.
    ///                       Aave), basis points (10_000 = 100%); across one parent's nested
    ///                       `adapters`, must sum to exactly 10_000, see notes above
    /// @param borrower       OffchainSource only: institution's EVM address; zero address otherwise
    /// @param collaterals    OffchainSource only: collateral NFTs backing the loan book; empty otherwise
    struct AdapterInput {
        SourceType source_type;
        address source_address;
        uint16 weightBps;
        address borrower;
        CollateralInput[] collaterals;
    }

    /// @param adapter_address Hub-chain MultichainAdapter contract address
    /// @param chain_id        EVM chain ID this MultichainAdapter routes capital to
    /// @param weightBps       Allocation weight Valuation uses for its top-level distribution, basis points (10_000 = 100%)
    /// @param adapters        Individual yield-source Adapters this MultichainAdapter internally
    ///                        manages/routes to (nested 2026-07-27 — see notes above)
    struct MultichainAdapterInput {
        address adapter_address;
        uint64 chain_id;
        uint16 weightBps;
        AdapterInput[] adapters;
    }

    /// @dev `product_admin` is not a function input on create_product (see below) —
    ///      it's the caller's own EVM address, already proven to hold ProductAdmin
    ///      for `product_id` by the precompile before dispatch. Included here only
    ///      for indexers/observability.
    event ProductCreated(
        uint256 product_id,
        address product_admin,
        address valuation_address,
        uint64 settlement_start_timestamp,
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
    event AdaptersSet(
        uint256 product_id,
        address parent_adapter_address,
        uint64 parent_chain_id,
        AdapterInput[] adapters
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
     *      `tranches` is sorted by each entry's own `priority` field (0 = highest) to
     *      establish the product's stored waterfall order — NOT by array position.
     *      Reverts if `product_id` is already taken, `tranches` is empty, two entries
     *      share a `priority`, sorting by `priority` doesn't put every Senior tranche
     *      before every Junior one, weightBps across `multichain_adapters` don't sum
     *      to 100% (10_000 bps), any entry's nested `adapters` weightBps don't
     *      themselves sum to 100%, or the same nested Adapter (address, chain_id)
     *      appears under two different `multichain_adapters` entries.
     *      Emits ProductCreated on success.
     * @param product_id          Hub product ID (already granted to the caller via ProductAdmin)
     * @param valuation           Valuation contract binding + settlement cadence config
     * @param tranches            Array of tranche configurations (each identified by its vault);
     *                             `priority`, not array order, determines final stored order
     * @param multichain_adapters Array of MultichainAdapter routing entries, each carrying its
     *                             own nested individual-Adapter registrations (address, chain_id,
     *                             weightBps, adapters)
     */
    function create_product(
        uint256 product_id,
        ValuationInput calldata valuation,
        TrancheInput[] calldata tranches,
        MultichainAdapterInput[] calldata multichain_adapters
    ) external;

    /**
     * @notice Add, remove, or update a tranche on an existing product, identified
     *         by its vault (chain_id, vault_address).
     * @dev Caller must hold the ProductAdmin role for `product_id` — enforced by the
     *      pallet itself, not just this precompile: it only accepts an origin this
     *      precompile constructs after checking ProductAdmin, so there is no signed-origin
     *      path that bypasses this check.
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
     *                  `tranche_type`'s Junior/Senior discriminant is immutable —
     *                  reverts if it doesn't match the existing tranche's (remove +
     *                  re-add to actually change it); `apr` may still change freely
     *                  for a Senior tranche, since only the discriminant is checked.
     *                  If `priority` differs from the tranche's current priority, it
     *                  re-inserts using the same shift semantics as Add.
     *      Every branch reverts if the resulting full tranche list would put any
     *      Junior tranche before a Senior one (see notes above).
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
     * @notice Replace, atomically, the entire set of individual yield-source
     *         Adapters nested under one of an existing product's MultichainAdapter
     *         entries (identified by `parent_adapter_address`, `parent_chain_id`).
     * @dev Caller must hold the ProductAdmin role for `product_id` — enforced by the
     *      pallet itself, not just this precompile: it only accepts an origin this
     *      precompile constructs after checking ProductAdmin, so there is no signed-origin
     *      path that bypasses this check.
     *      Mirrors `set_multichain_adapters`'s full-array-replace shape, scoped to
     *      one parent's nested `adapters` instead of the whole table: callers
     *      submit the full intended end-state list every time, not deltas — the
     *      same rationale applies, now that adapter-level `weightBps` also carries
     *      a hard 100% sum invariant (see below), a single-entity add/remove can't
     *      preserve it without either silently rescaling every other entry or
     *      leaving the sum temporarily wrong between calls.
     *      Reverts if no MultichainAdapter matching (`parent_adapter_address`,
     *      `parent_chain_id`) exists for `product_id`, unless
     *      `sum(adapters[i].weightBps) == 10_000`, if `adapters` contains a
     *      duplicate `source_address`, or if a `source_address` is already
     *      registered under a *different* parent (an Adapter belongs to at most
     *      one MultichainAdapter globally, see notes above — re-registering the
     *      same `source_address` under *this* parent, e.g. just to change its
     *      `weightBps`, is fine).
     *      Emits AdaptersSet on success.
     * @param product_id             The product whose adapters are being replaced
     * @param parent_adapter_address The parent MultichainAdapter's contract address
     * @param parent_chain_id        The parent MultichainAdapter's chain ID
     * @param adapters               The full intended end-state list of nested adapters
     */
    function set_adapters(
        uint256 product_id,
        address parent_adapter_address,
        uint64 parent_chain_id,
        AdapterInput[] calldata adapters
    ) external;

    /**
     * @notice Replace a product's entire MultichainAdapter routing table atomically.
     * @dev Caller must hold the ProductAdmin role for `product_id` — enforced by the
     *      pallet itself, not just this precompile: it only accepts an origin this
     *      precompile constructs after checking ProductAdmin, so there is no signed-origin
     *      path that bypasses this check.
     *      Unlike set_tranche, this is NOT a single-entity add/remove/update call —
     *      callers must submit the full intended end-state list every time, not
     *      deltas. This is deliberate: a product's multichain_adapters weightBps
     *      must always sum to exactly 100% (10_000 bps), and a single-entity
     *      mutation can't preserve that cross-entry invariant without either
     *      silently rescaling every other entry (a surprising side effect) or
     *      leaving the sum temporarily wrong between calls (a real fund-safety risk
     *      if Valuation acts on a stale/incomplete state). A full-array replace
     *      checked atomically avoids both — same rationale as `set_adapters` uses
     *      for a single parent's nested adapters. The replace is deep: each entry's
     *      nested `adapters` is replaced wholesale along with it, same as
     *      `set_adapters` would leave it — callers changing only e.g. a top-level
     *      weightBps must still resupply that entry's unchanged `adapters` array,
     *      or they'll be wiped.
     *      Reverts unless `sum(multichain_adapters[i].weightBps) == 10_000`, if
     *      `multichain_adapters` contains a duplicate (`adapter_address`, `chain_id`)
     *      pair, if any entry's nested `adapters` contains a duplicate
     *      `source_address`, or if the same nested `source_address` (chain-aware,
     *      via its parent's `chain_id`) appears under two different entries — an
     *      Adapter belongs to at most one MultichainAdapter globally, see notes above.
     *      Emits MultichainAdaptersSet on success.
     * @param product_id          The product whose MultichainAdapter table is being replaced
     * @param multichain_adapters The full intended end-state list of routing entries
     */
    function set_multichain_adapters(
        uint256 product_id,
        MultichainAdapterInput[] calldata multichain_adapters
    ) external;
}
