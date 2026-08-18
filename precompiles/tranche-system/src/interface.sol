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
 *   - `multichain_tranche_managers` (added 2026-08-14): each product independently
 *     binds its own TrancheManager contract address per chain one of its vaults is
 *     deployed on — distinct from `MultichainAdapterInput`'s per-chain table
 *     (routes capital to yield sources) even though both are keyed by `chain_id`.
 *     Hub included: a product with a Hub-deployed vault needs a Hub-chain entry
 *     here too, same as any Spoke chain.
 *   - Single-chain products (added 2026-08-18): everything above describes the
 *     hub-spoke multichain model (`create_product` and friends), where a Hub-chain
 *     Valuation Contract prices vaults that may live on other chains. A product can
 *     instead be entirely single-chain — Vault(s), TrancheManager, Valuation,
 *     Adapters, and a Ledger contract all on one EVM chain (not necessarily the
 *     Hub) — via `create_single_chain_product`. `get_product`, `get_tranches`, and
 *     `get_adapters` are model-agnostic: they work for both kinds (see each
 *     function's own doc for how single-chain values map onto their return shape).
 *     `get_multichain_adapters`/`get_multichain_tranche_managers` only work for the
 *     multichain kind (revert otherwise); `get_tranche_manager`/`get_ledger` only
 *     work for the single-chain kind (revert otherwise, no multichain equivalent
 *     for `get_ledger` — see below).
 *   - Single-chain products additionally support `SettlementMode.Sync`: request and
 *     settlement happen atomically, in the same transaction, since there's no
 *     cross-chain leg to wait on. Only structurally possible single-chain — a
 *     multichain product is always `Async` (implicitly, via `ValuationInput`'s
 *     settlement fields), since a cross-chain leg always takes more than one block.
 *     `SettlementModeInput.is_sync` discriminates the two; the three settlement_*
 *     fields are `0` and not meaningful when `is_sync == true`.
 *   - A single-chain product has no `pallet-tranche-investments` interaction at all
 *     (that pallet assumes a Hub-deployed Valuation) — instead, its `ledger`
 *     contract, on the same chain, mirrors pallet-tranche-investments' interface
 *     and plays that role locally. This pallet only stores its address.
 *   - A single-chain product's Adapters are a flat, ungrouped list — there is no
 *     MultichainAdapter routing layer above them (`MultichainAdapterInput` simply
 *     doesn't exist in this model), since there's nothing to route between with
 *     only one chain. `AdapterInput.weightBps` across the whole flat list must sum
 *     to exactly 10_000, same invariant as one `MultichainAdapterInput`'s nested
 *     `adapters`.
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

    /// @param base_asset                  The product's denomination asset — its token address
    ///                                     on the Hub chain. Immutable after create_product; every
    ///                                     NAV/price value recorded against this product elsewhere
    ///                                     (Investments' TrancheSettle.tranche_nav, product_nav,
    ///                                     etc.) is denominated in this asset
    /// @param valuation_address           Hub-chain Valuation contract address for this product
    /// @param settlement_start_timestamp  Unix timestamp the first settlement cycle begins; must
    ///                                     be strictly after the block time create_product
    ///                                     executes in (reverts otherwise) — can be scheduled
    ///                                     ahead, never backdated. Every later cycle starts at
    ///                                     settlement_start_timestamp + k * settlement_length_secs.
    ///                                     Purely configuration — read off-chain by the settlement
    ///                                     bot that calls Valuation.tryUpdateNav(); no on-chain
    ///                                     logic acts on it
    /// @param settlement_length_secs      Length of one settlement cycle, in seconds, counted
    ///                                     from settlement_start_timestamp; admin-set, recommended
    ///                                     to be at least the GCD of the underlying yield sources'
    ///                                     cycles. Must be strictly greater than
    ///                                     settlement_offset_secs (reverts otherwise)
    /// @param settlement_offset_secs      Width, in seconds, of the settlement window at the
    ///                                     *end* of each cycle (e.g. 3600 for a 1-hour window) —
    ///                                     not seconds since the cycle started. Order submission
    ///                                     closes ("market close") when the window opens. Must be
    ///                                     strictly less than settlement_length_secs (reverts
    ///                                     otherwise)
    struct ValuationInput {
        address base_asset;
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
    /// @param asset        The asset investors deposit when depositing into `vault` — a token
    ///                     address on `vault.chain_id` (not necessarily the Hub chain, and not
    ///                     necessarily the same asset across different tranches of the same
    ///                     product). Distinct from ValuationInput.base_asset, which is the
    ///                     Hub-chain asset NAV/pricing is denominated in
    /// @param shares       This tranche's own share-token contract address — the ERC-7540
    ///                     vault's share token investors receive/burn on deposit/redeem, on
    ///                     `vault.chain_id`. Distinct from `asset` (what's deposited in)
    /// @param priority     Waterfall priority within the product; 0 = highest priority, see notes above
    struct TrancheInput {
        TrancheType tranche_type;
        uint256 apr;
        VaultInput vault;
        address asset;
        address shares;
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

    /// @param chain_id                EVM chain ID this TrancheManager is deployed on. Hub
    ///                                included: a product with a Hub-deployed vault needs a
    ///                                Hub-chain entry here too, same as any Spoke chain
    /// @param tranche_manager_address This product's TrancheManager contract address on `chain_id`.
    ///                                Independent per product — two products sharing a chain
    ///                                each bind their own TrancheManager instance there
    struct MultichainTrancheManagerInput {
        uint64 chain_id;
        address tranche_manager_address;
    }

    /// @param is_sync                      True = settle atomically in the same transaction as
    ///                                     the request, no settlement cycle at all. Only valid
    ///                                     for single-chain products, see notes above. False =
    ///                                     same cadence semantics as ValuationInput's fields
    /// @param settlement_start_timestamp  Ignored when is_sync; see ValuationInput's field of
    ///                                     the same name otherwise
    /// @param settlement_length_secs      Ignored when is_sync; see ValuationInput's field of
    ///                                     the same name otherwise
    /// @param settlement_offset_secs      Ignored when is_sync; see ValuationInput's field of
    ///                                     the same name otherwise
    struct SettlementModeInput {
        bool is_sync;
        uint64 settlement_start_timestamp;
        uint64 settlement_length_secs;
        uint64 settlement_offset_secs;
    }

    /// @param base_asset          The product's denomination asset, on the product's chain_id
    /// @param valuation_address   The Valuation contract address, on the product's chain_id —
    ///                            NOT necessarily the Hub chain, unlike ValuationInput's field
    ///                            of the same name
    /// @param settlement_mode     Sync or Async settlement — see SettlementModeInput
    /// @dev A parallel type to ValuationInput, not a reuse of it — ValuationInput's settlement
    ///      fields are already live on-chain via create_product, so this stays separate rather
    ///      than reshaping that already-shipped interface just to share a type here.
    struct SingleChainValuationInput {
        address base_asset;
        address valuation_address;
        SettlementModeInput settlement_mode;
    }

    /// @dev `product_admin` is not a function input on create_product (see below) —
    ///      it's the caller's own EVM address, already proven to hold ProductAdmin
    ///      for `product_id` by the precompile before dispatch. Included here only
    ///      for indexers/observability.
    event ProductCreated(
        uint256 product_id,
        address product_admin,
        address base_asset,
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
        address asset,
        address shares,
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
    event MultichainTrancheManagersSet(
        uint256 product_id,
        MultichainTrancheManagerInput[] multichain_tranche_managers
    );
    /// @dev `product_admin` is not a function input on create_single_chain_product, same
    ///      reasoning as ProductCreated's `product_admin` above.
    event SingleChainProductCreated(
        uint256 product_id,
        address product_admin,
        uint64 chain_id,
        address base_asset,
        address valuation_address,
        address tranche_manager,
        address ledger,
        bool is_sync,
        uint64 settlement_start_timestamp,
        uint64 settlement_length_secs,
        uint64 settlement_offset_secs
    );

    /**
     * @notice Create a new tranche-system product: its Valuation contract binding,
     *         its tranches, its MultichainAdapter routing table, its individual
     *         yield-source Adapter registrations, and its per-chain
     *         TrancheManager bindings.
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
     *      themselves sum to 100%, the same nested Adapter (address, chain_id)
     *      appears under two different `multichain_adapters` entries,
     *      `valuation.settlement_offset_secs >= valuation.settlement_length_secs`,
     *      `valuation.settlement_start_timestamp` is not strictly after the current
     *      block time.
     *      Emits ProductCreated on success.
     * @param product_id                   Hub product ID (already granted to the caller via
     *                                     ProductAdmin)
     * @param valuation                    Valuation contract binding + settlement cadence config
     * @param tranches                     Array of tranche configurations (each identified by its
     *                                     vault); `priority`, not array order, determines final
     *                                     stored order
     * @param multichain_adapters          Array of MultichainAdapter routing entries, each
     *                                     carrying its own nested individual-Adapter
     *                                     registrations (address, chain_id, weightBps, adapters)
     * @param multichain_tranche_managers  Array of this product's per-chain TrancheManager
     *                                     bindings (chain_id, tranche_manager_address); Hub
     *                                     included, see MultichainTrancheManagerInput
     */
    function create_product(
        uint256 product_id,
        ValuationInput calldata valuation,
        TrancheInput[] calldata tranches,
        MultichainAdapterInput[] calldata multichain_adapters,
        MultichainTrancheManagerInput[] calldata multichain_tranche_managers
    ) external;

    /**
     * @notice Create a new single-chain product: every contract (Vault(s),
     *         TrancheManager, Valuation, Adapters, Ledger) lives on one EVM chain
     *         (`chain_id`), not necessarily the Hub — see notes above for how this
     *         differs from create_product's hub-spoke model.
     * @dev Same caller-authorization model as create_product (ProductAdmin for
     *      `product_id`, checked by the precompile before dispatch).
     *      `tranches` uses the same priority-sort-and-validate rules as
     *      create_product's `tranches`, plus one extra check: every entry's
     *      `vault.chain_id` must equal `chain_id` (reverts otherwise).
     *      `adapters` is a flat list — see notes above — and its `weightBps` must
     *      sum to exactly 10_000, same invariant as one MultichainAdapterInput's
     *      nested `adapters`.
     *      When `valuation.settlement_mode.is_sync` is false, the same
     *      `settlement_offset_secs < settlement_length_secs` and
     *      `settlement_start_timestamp > now` checks as create_product apply.
     *      Emits SingleChainProductCreated on success.
     * @param product_id        Hub product ID (already granted to the caller via ProductAdmin)
     * @param chain_id          The single EVM chain every contract in this product lives on
     * @param valuation         (base_asset, valuation_address, settlement_mode) — see
     *                          SingleChainValuationInput
     * @param tranches          Tranche configs; every entry's vault.chain_id must equal chain_id
     * @param tranche_manager   The single TrancheManager contract address, on chain_id
     * @param adapters          Flat individual-Adapter registrations (no MultichainAdapter
     *                          routing layer above them)
     * @param ledger            The Ledger contract address, on chain_id — mirrors
     *                          pallet-tranche-investments' interface locally for this product
     */
    function create_single_chain_product(
        uint256 product_id,
        uint64 chain_id,
        SingleChainValuationInput calldata valuation,
        TrancheInput[] calldata tranches,
        address tranche_manager,
        AdapterInput[] calldata adapters,
        address ledger
    ) external;

    /**
     * @notice Add, remove, or update a tranche on an existing product, identified
     *         by its vault (chain_id, vault_address). Works for both Multichain and
     *         single-chain products.
     * @dev For a single-chain product, Add/Update additionally revert unless
     *      `tranche.vault`'s `chain_id` equals the product's own `chain_id` (same
     *      constraint create_single_chain_product enforces at creation time).
     *      Caller must hold the ProductAdmin role for `product_id` — enforced by the
     *      pallet itself, not just this precompile: it only accepts an origin this
     *      precompile constructs after checking ProductAdmin, so there is no signed-origin
     *      path that bypasses this check.
     *      Field usage differs by `action` — unused fields are ignored, but callers
     *      must still supply the full struct (e.g. pass zero/default values for
     *      `tranche_type`/`apr`/`asset`/`shares`/`priority` on a `Remove` call):
     *        - Add:    `tranche.vault` becomes the new tranche's identity (reverts if
     *                  a tranche with the same vault already exists for this product).
     *                  `tranche_type`, `apr` (Senior-only), `asset`, `shares`, and
     *                  `priority` are used. If `priority` is already occupied, the
     *                  existing tranche at that slot (and everything after it) shifts
     *                  down by one.
     *        - Remove: only `tranche.vault` is used, to identify which tranche to
     *                  remove (reverts if not found, or if it has outstanding
     *                  investments). Every tranche with a lower priority ranking
     *                  (higher numeric value) than the removed one shifts up by
     *                  one, closing the gap.
     *        - Update: `tranche.vault` identifies which tranche to update (reverts
     *                  if not found); `apr`, `asset`, `shares`, and `priority` are
     *                  applied as new values. `tranche_type`'s Junior/Senior discriminant
     *                  is immutable — reverts if it doesn't match the existing tranche's
     *                  (remove + re-add to actually change it); `apr` may still change
     *                  freely for a Senior tranche, since only the discriminant is checked.
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
     * @notice Replace, atomically, a product's entire flat individual-Adapter set —
     *         works for both Multichain and single-chain products.
     * @dev For a Multichain product, this replaces one MultichainAdapter's nested
     *      `adapters` (identified by `parent_adapter_address`, `parent_chain_id` —
     *      reverts if no such parent exists for `product_id`). For a single-chain
     *      product, `parent_adapter_address`/`parent_chain_id` are ignored (there's
     *      no MultichainAdapter parent concept at all) — this replaces the
     *      product's whole flat `adapters` map instead.
     *      Caller must hold the ProductAdmin role for `product_id` — enforced by the
     *      pallet itself, not just this precompile: it only accepts an origin this
     *      precompile constructs after checking ProductAdmin, so there is no signed-origin
     *      path that bypasses this check.
     *      Mirrors `set_multichain_adapters`'s full-array-replace shape, scoped to
     *      one parent's nested `adapters` (or, for single-chain, the whole flat set)
     *      instead of the whole routing table: callers submit the full intended
     *      end-state list every time, not deltas — the same rationale applies, now
     *      that adapter-level `weightBps` also carries a hard 100% sum invariant
     *      (see below), a single-entity add/remove can't preserve it without either
     *      silently rescaling every other entry or leaving the sum temporarily
     *      wrong between calls.
     *      Reverts unless `sum(adapters[i].weightBps) == 10_000`, if `adapters`
     *      contains a duplicate `source_address`, or if a `source_address` is
     *      already registered elsewhere (an Adapter address belongs to at most one
     *      such set globally — re-registering the same `source_address` in *this*
     *      call's own previous set, e.g. just to change its `weightBps`, is fine).
     *      Emits AdaptersSet on success.
     * @param product_id             The product whose adapters are being replaced
     * @param parent_adapter_address Multichain only: the parent MultichainAdapter's
     *                               contract address (ignored for single-chain)
     * @param parent_chain_id        Multichain only: the parent MultichainAdapter's
     *                               chain ID (ignored for single-chain)
     * @param adapters               The full intended end-state list of adapters
     */
    function set_adapters(
        uint256 product_id,
        address parent_adapter_address,
        uint64 parent_chain_id,
        AdapterInput[] calldata adapters
    ) external;

    /**
     * @notice Replace a product's entire MultichainAdapter routing table atomically.
     * @dev Only applies to Multichain products — reverts if `product_id` is a
     *      single-chain product (it has no MultichainAdapter routing layer at all).
     *      Caller must hold the ProductAdmin role for `product_id` — enforced by the
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

    /**
     * @notice Replace a product's entire per-chain TrancheManager table atomically.
     * @dev Only applies to Multichain products — reverts if `product_id` is a
     *      single-chain product (it has a single fixed `tranche_manager` address instead,
     *      set at create_single_chain_product time).
     *      Caller must hold the ProductAdmin role for `product_id` — enforced by the
     *      pallet itself, not just this precompile: it only accepts an origin this
     *      precompile constructs after checking ProductAdmin, so there is no signed-origin
     *      path that bypasses this check.
     *      Not a single-entity add/remove/update call — callers must submit the full
     *      intended end-state list every time, not deltas, same full-array-replace
     *      rationale as set_multichain_adapters (here: simplicity, since there's no
     *      cross-entry invariant like weightBps to protect).
     *      Emits MultichainTrancheManagersSet on success.
     * @param product_id                   The product whose TrancheManager table is being replaced
     * @param multichain_tranche_managers  The full intended end-state list of per-chain
     *                                     bindings
     */
    function set_multichain_tranche_managers(
        uint256 product_id,
        MultichainTrancheManagerInput[] calldata multichain_tranche_managers
    ) external;

    /**
     * @notice Read a product's Valuation binding and settlement-cadence config. Works for
     *         both Multichain and single-chain products.
     * @dev Reverts if `product_id` doesn't exist. For a single-chain product settling SYNC
     *      (see SettlementModeInput), the three settlement_* fields below are all 0 and not
     *      meaningful — treat all-zero as SYNC.
     * @param product_id The product to look up
     */
    function get_product(
        uint256 product_id
    )
        external
        view
        returns (
            address base_asset,
            address valuation,
            uint64 settlement_start_timestamp,
            uint64 settlement_length_secs,
            uint64 settlement_offset_secs
        );

    /**
     * @notice Read a product's tranches, in waterfall priority order (index 0 = highest
     *         priority — see notes above).
     * @dev Reverts if `product_id` doesn't exist. Works for both Multichain and
     *      single-chain products. Each returned entry's `priority` field reflects
     *      current stored order, not necessarily whatever `priority` value the
     *      tranche was originally added/updated with.
     * @param product_id The product to look up
     */
    function get_tranches(
        uint256 product_id
    ) external view returns (TrancheInput[] memory tranches);

    /**
     * @notice Read a Multichain product's MultichainAdapter routing table, each entry
     *         carrying its own nested individual-Adapter registrations.
     * @dev Reverts if `product_id` doesn't exist, or if it's a single-chain product (see
     *      get_adapters for a simpler, model-agnostic alternative).
     * @param product_id The product to look up
     */
    function get_multichain_adapters(
        uint256 product_id
    )
        external
        view
        returns (MultichainAdapterInput[] memory multichain_adapters);

    /// @param chain_id EVM chain ID these adapters live on
    /// @param adapters Individual Adapters on this chain
    struct AdaptersByChain {
        uint64 chain_id;
        AdapterInput[] adapters;
    }

    /**
     * @notice Read a product's Adapters, grouped by the chain they live on. Works for both
     *         Multichain and single-chain products.
     * @dev Reverts if `product_id` doesn't exist. For a Multichain product, one entry per
     *      MultichainAdapter routing entry — `chain_id` is that entry's own `chain_id`,
     *      `adapters` its nested individual-Adapter list (the MultichainAdapter's own
     *      routing address/weightBps aren't included here — use get_multichain_adapters for
     *      that full detail). For a single-chain product, always exactly one entry: the
     *      product's single `chain_id` and its flat Adapter list.
     * @param product_id The product to look up
     */
    function get_adapters(
        uint256 product_id
    ) external view returns (AdaptersByChain[] memory adapters_by_chain);

    /**
     * @notice Read a Multichain product's per-chain TrancheManager bindings.
     * @dev Reverts if `product_id` doesn't exist, or if it's a single-chain product
     *      (which has a single tranche_manager address instead — see
     *      get_tranche_manager). See MultichainTrancheManagerInput.
     * @param product_id The product to look up
     */
    function get_multichain_tranche_managers(
        uint256 product_id
    )
        external
        view
        returns (
            MultichainTrancheManagerInput[] memory multichain_tranche_managers
        );

    /**
     * @notice Read a single-chain product's single TrancheManager contract address.
     * @dev Reverts if `product_id` doesn't exist, or if it's a Multichain product (which has
     *      a per-chain table instead — see get_multichain_tranche_managers).
     * @param product_id The product to look up
     * @return tranche_manager The TrancheManager contract address, on the product's chain_id
     */
    function get_tranche_manager(
        uint256 product_id
    ) external view returns (address tranche_manager);

    /**
     * @notice Read a single-chain product's Ledger contract address — mirrors
     *         pallet-tranche-investments' interface locally for this product.
     * @dev Reverts if `product_id` doesn't exist, or if it's a Multichain product (which has
     *      no Ledger contract at all; it interacts with pallet-tranche-investments directly).
     * @param product_id The product to look up
     * @return ledger The Ledger contract address, on the product's chain_id
     */
    function get_ledger(
        uint256 product_id
    ) external view returns (address ledger);

    /**
     * @notice Read the single, global Hub-chain Orchestrator contract address.
     * @dev Not per-product — same value regardless of caller. Never reverts; defaults to
     *      the zero address until root calls set_orchestrator_address (not exposed on this
     *      interface — see pallet_tranche_system::OrchestratorAddress's doc comment).
     * @return orchestrator The Orchestrator contract address, or the zero address if unset
     */
    function get_orchestrator() external view returns (address orchestrator);
}
