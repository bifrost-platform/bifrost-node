// SPDX-License-Identifier: GPL-3.0-only
pragma solidity >=0.8.0;

/**
 * @title Tranche Permissions Precompile Interface (tranche-system draft)
 * @notice Manages role-based permissions for OmniFi tranche-system products:
 *         ProductAdmin, OracleFeeder, and TrancheInvestor (whitelist). Replaces
 *         the old TrancheInvestor-only Permissions precompile — grant/revoke is
 *         now unified across all roles behind one function pair, rather than
 *         the old precompile's narrower `add_tranche_investor`/
 *         `remove_tranche_investor`.
 *
 * DRAFT — reflects the tranche-system pivot (2026-07-24), not yet locked in.
 * pallet-tranche-permissions does not exist yet; this interface is written
 * ahead of the pallet, same as tranche-system/investments (see
 * [[project_omnifi_revamp_callflow]] in project memory).
 *
 *   - Address reused from the old Permissions precompile (0x...0202) — same
 *     address-rotation pattern already applied to investments/tranche-system
 *     (each new precompile takes over the slot of the pallet it functionally
 *     replaces).
 *   - `Role.ProductAdmin` can only ever be granted by sudo/root, mirroring old
 *     pools' `Role::PoolAdmin` (pre-granted before `create_product` is called —
 *     see pallet-tranche-system). Calling grant_permission/revoke_permission
 *     with `role == ProductAdmin` through THIS precompile will always revert,
 *     since precompile-dispatched calls never carry a root origin. The variant
 *     still exists in this enum because other pallets need to represent/check
 *     it (e.g. pallet-tranche-system's ProductAdmin gate), not because it's
 *     grantable here.
 *   - `vault` is only meaningful when `role == TrancheInvestor` — mirrors the
 *     "always pass the full struct, ignore what doesn't apply" convention
 *     already used throughout this interface family (e.g. TrancheSystem's
 *     set_tranche/set_adapter). Pass zero/default values for any other role.
 *   - A `TrancheInvestor` grant/revoke whose `vault` lives on a Spoke chain
 *     (`vault.chain_id != this Hub chain's own EVM chain ID`) is propagated
 *     there automatically: after the on-chain grant/revoke succeeds, this
 *     precompile calls the Hub-chain Orchestrator contract's
 *     `sendWhitelist(chainId, productId, vaultAddress, who, action)`
 *     (`action`: 0 = revoke, 1 = grant), which relays it onward via CCCP to
 *     the target chain's Whitelist module. The Orchestrator's own address is
 *     a single global value stored in pallet-tranche-system
 *     (`OrchestratorAddress`, root-settable only — not exposed on this
 *     interface). If it isn't a Spoke-chain vault (same chain ID as the
 *     Hub), or `vault` is irrelevant (any role other than TrancheInvestor),
 *     no propagation happens — the grant/revoke is local-only. The whole
 *     call reverts if propagation was needed but `OrchestratorAddress` isn't
 *     configured, or if the Orchestrator call itself fails — triggering
 *     propagation is atomic with the permission grant/revoke; only what
 *     happens after that (Spoke-side relay) is safe to retry independently.
 *   - **No `Borrower` role here** — deliberately removed (2026-07-24). A
 *     product can have multiple OffchainSource adapters, each potentially a
 *     different institution, so there's no single product-scoped "Borrower"
 *     account the way old pools had `Role::Borrower`. Borrower identity now
 *     lives directly on each adapter instead — see pallet-tranche-system's
 *     `SourceType::OffchainSource { borrower, .. }` — set via
 *     TrancheSystem's `set_adapter`, not through this precompile at all.
 *     Borrow/repay bookkeeping itself is no longer an on-chain concern at
 *     all (2026-07-24): the earlier `pallet-rwa-loans` draft was dropped
 *     entirely — that ledger moved fully off-chain into the adapter itself,
 *     since nothing on-chain ever consumed it once NAV stopped being
 *     computed on-node. `borrower` here exists purely as adapter metadata.
 *
 * Address: 0x0000000000000000000000000000000000000202
 */
interface TranchePermissions {
    enum Role {
        ProductAdmin,
        OracleFeeder,
        TrancheInvestor
    }

    /// @param chain_id      EVM chain ID where the tranche's ERC-7540 vault is deployed
    /// @param vault_address ERC-7540 vault contract address identifying the tranche
    ///                      (mirrors TrancheSystem's VaultInput — duplicated here
    ///                      since Solidity interfaces don't share type declarations
    ///                      across files)
    struct VaultInput {
        uint64 chain_id;
        address vault_address;
    }

    event PermissionGranted(
        uint64 product_id,
        Role role,
        address who,
        uint64 vault_chain_id,
        address vault_address
    );
    event PermissionRevoked(
        uint64 product_id,
        Role role,
        address who,
        uint64 vault_chain_id,
        address vault_address
    );

    /**
     * @notice Grant `role` to `who` for `product_id`.
     * @dev Authorization: `role == ProductAdmin` always reverts through this
     *      precompile (sudo-only, see notes above). `OracleFeeder`/
     *      `TrancheInvestor` require the caller to already hold ProductAdmin
     *      for `product_id`.
     *      `vault` is only used when `role == TrancheInvestor`; ignored otherwise.
     *      Reverts if `who` already holds `role` for `product_id` (and, for
     *      TrancheInvestor, the given `vault`).
     *      For TrancheInvestor on a Spoke-chain vault, also propagates to
     *      Orchestrator.sendWhitelist(..., action: 1) — see notes above.
     *      Emits PermissionGranted on success.
     * @param product_id The product this permission applies to
     * @param role       ProductAdmin, OracleFeeder, or TrancheInvestor
     * @param who        EVM address receiving the role
     * @param vault      TrancheInvestor-only: identifies which tranche the whitelist applies to
     */
    function grant_permission(
        uint64 product_id,
        Role role,
        address who,
        VaultInput calldata vault
    ) external;

    /**
     * @notice Revoke `role` from `who` for `product_id`.
     * @dev Same authorization rules as grant_permission.
     *      Reverts if `who` does not currently hold `role` for `product_id`
     *      (and, for TrancheInvestor, the given `vault`).
     *      For TrancheInvestor on a Spoke-chain vault, also propagates to
     *      Orchestrator.sendWhitelist(..., action: 0) — see notes above.
     *      Emits PermissionRevoked on success.
     * @param product_id The product this permission applies to
     * @param role       ProductAdmin, OracleFeeder, or TrancheInvestor
     * @param who        EVM address losing the role
     * @param vault      TrancheInvestor-only: identifies which tranche the whitelist applies to
     */
    function revoke_permission(
        uint64 product_id,
        Role role,
        address who,
        VaultInput calldata vault
    ) external;

    /**
     * @notice Read whether `who` currently holds the TrancheInvestor whitelist for `vault`.
     * @dev `product_id` is accepted for signature symmetry with grant_permission/
     *      revoke_permission but isn't part of the actual check — the whitelist is keyed
     *      by `vault` alone (globally unique across all products).
     * @param product_id Accepted for signature symmetry; not used in the lookup itself
     * @param vault      Identifies the tranche whose whitelist is being checked
     * @param who        EVM address to check
     */
    function is_tranche_investor(
        uint64 product_id,
        VaultInput calldata vault,
        address who
    ) external view returns (bool);

    /**
     * @notice Read whether `who` holds `role` for `product_id`.
     * @dev Only ProductAdmin and OracleFeeder are supported here — TrancheInvestor
     *      reverts (it has no `vault` parameter on this signature); use
     *      is_tranche_investor for that role instead.
     * @param product_id The product to check
     * @param role       ProductAdmin or OracleFeeder
     * @param who        EVM address to check
     */
    function has_role(
        uint64 product_id,
        Role role,
        address who
    ) external view returns (bool);
}
