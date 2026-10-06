// SPDX-License-Identifier: GPL-3.0-only
pragma solidity >=0.8.0;

/**
 * @custom:universal Non-EVM-compatible interface (Tranche Permissions). Same storage as the
 *     EVM-only precompile at 0x0000000000000000000000000000000000000202, whose
 *     original ABI is unchanged; here every product/spoke-chain address is a
 *     `bytes32` (EVM addresses left-padded: `bytes32(uint256(uint160(a)))`, Solana/
 *     Stellar keys as-is) and every foreign tx hash is `bytes` (up to 64 bytes).
 *
 * @title Tranche Permissions Precompile Interface
 * @notice Manages role-based permissions for OmniFi tranche-system products:
 *         ProductAdmin and OracleFeeder roles, plus the per-tranche investor
 *         whitelist.
 *
 *   - `Role.ProductAdmin` can only ever be granted by sudo/root (pre-granted
 *     before `create_product` is called — see pallet-tranche-system). Calling
 *     grant_permission/revoke_permission
 *     with `role == ProductAdmin` through THIS precompile will always revert,
 *     since precompile-dispatched calls never carry a root origin. The variant
 *     still exists in this enum because other pallets need to represent/check
 *     it (e.g. pallet-tranche-system's ProductAdmin gate), not because it's
 *     grantable here.
 *   - Tranche investors are managed separately from roles (non-EVM support):
 *     an investor is a `bytes32` address on the vault's own chain (EVM
 *     addresses left-padded; Solana/Stellar keys as-is), not a Hub account, so
 *     grant_tranche_investor/revoke_tranche_investor/is_tranche_investor take
 *     `bytes32 investor` and a `bytes32`-addressed `VaultInput`.
 *   - Every tranche-investor grant/revoke is propagated automatically: after
 *     the on-chain grant/revoke succeeds, this precompile calls the Hub-chain
 *     Orchestrator contract's
 *     `sendWhitelist(uint64 chainId, uint64 productId, bytes32 vault, bytes32 investor, uint8 action)`
 *     (`action`: 0 = revoke, 1 = grant), which applies it locally for a Hub
 *     vault or relays it to the target chain's Whitelist module for a Spoke
 *     vault. The Orchestrator's own address is a single global value stored in
 *     pallet-tranche-system (`OrchestratorAddress`, root-settable only — not
 *     exposed on this interface). The whole call reverts if
 *     `OrchestratorAddress` isn't configured, or if the Orchestrator call itself
 *     fails — triggering propagation is atomic with the grant/revoke; only what
 *     happens after that (Spoke-side relay) is safe to retry independently.
 *   - **No `Borrower` role here** — deliberately removed (2026-07-24). A
 *     product can have multiple OffchainSource adapters, each potentially a
 *     different institution, so there's no single product-scoped "Borrower"
 *     account to represent. Borrower identity now lives directly on each
 *     adapter instead — see pallet-tranche-system's
 *     `SourceType::OffchainSource { borrower, .. }` — set via
 *     TrancheSystem's `set_adapter`, not through this precompile at all.
 *     Borrow/repay bookkeeping itself is no longer an on-chain concern at
 *     all (2026-07-24): the earlier `pallet-rwa-loans` draft was dropped
 *     entirely — that ledger moved fully off-chain into the adapter itself,
 *     since nothing on-chain ever consumed it once NAV stopped being
 *     computed on-node. `borrower` here exists purely as adapter metadata.
 *
 * Address: 0x0000000000000000000000000000000000000602
 */
interface TranchePermissionsUniversal {
    /// @dev Tranche investors are not a role — see grant_tranche_investor.
    enum Role {
        ProductAdmin,
        OracleFeeder
    }

    /// @param chain_id      Chain ID where the tranche's vault is deployed
    /// @param vault_address The vault's address on chain_id, as bytes32 (EVM addresses
    ///                      left-padded) — mirrors TrancheSystem's VaultInput
    struct VaultInput {
        uint64 chain_id;
        bytes32 vault_address;
    }

    event PermissionGranted(uint64 product_id, Role role, address who);
    event PermissionRevoked(uint64 product_id, Role role, address who);
    event TrancheInvestorGranted(
        uint64 product_id,
        uint64 vault_chain_id,
        bytes32 vault_address,
        bytes32 investor
    );
    event TrancheInvestorRevoked(
        uint64 product_id,
        uint64 vault_chain_id,
        bytes32 vault_address,
        bytes32 investor
    );

    /**
     * @notice Grant `role` to `who` for `product_id`.
     * @dev Authorization: `role == ProductAdmin` always reverts through this
     *      precompile (sudo-only, see notes above). `OracleFeeder` requires the
     *      caller to already hold ProductAdmin for `product_id`.
     *      Reverts if `who` already holds `role` for `product_id`.
     *      Emits PermissionGranted on success.
     * @param product_id The product this permission applies to
     * @param role       ProductAdmin or OracleFeeder
     * @param who        Hub (EVM) address receiving the role
     */
    function grant_permission(uint64 product_id, Role role, address who) external;

    /**
     * @notice Revoke `role` from `who` for `product_id`.
     * @dev Same authorization rules as grant_permission.
     *      Reverts if `who` does not currently hold `role` for `product_id`.
     *      Emits PermissionRevoked on success.
     * @param product_id The product this permission applies to
     * @param role       ProductAdmin or OracleFeeder
     * @param who        Hub (EVM) address losing the role
     */
    function revoke_permission(uint64 product_id, Role role, address who) external;

    /**
     * @notice Whitelist `investor` for `vault` of `product_id`.
     * @dev Caller must hold ProductAdmin for `product_id`; `vault` must be an active
     *      tranche of it and the product must be Multichain. Reverts if already granted.
     *      Propagates to Orchestrator.sendWhitelist(chainId, productId, vault, investor, 1)
     *      atomically — see notes above. Emits TrancheInvestorGranted on success.
     * @param product_id The product `vault` belongs to
     * @param vault      Identifies the tranche the whitelist applies to
     * @param investor   The investor's address on vault.chain_id, as bytes32 (EVM
     *                   addresses left-padded)
     */
    function grant_tranche_investor(
        uint64 product_id,
        VaultInput calldata vault,
        bytes32 investor
    ) external;

    /**
     * @notice Remove `investor` from `vault`'s whitelist of `product_id`.
     * @dev Same rules as grant_tranche_investor; reverts if not granted. Propagates
     *      Orchestrator.sendWhitelist(..., 0). Emits TrancheInvestorRevoked on success.
     * @param product_id The product `vault` belongs to
     * @param vault      Identifies the tranche the whitelist applies to
     * @param investor   The investor's address on vault.chain_id, as bytes32
     */
    function revoke_tranche_investor(
        uint64 product_id,
        VaultInput calldata vault,
        bytes32 investor
    ) external;

    /**
     * @notice Read whether `investor` currently holds the TrancheInvestor whitelist for
     *         `vault` under `product_id`.
     * @dev `product_id` IS part of the actual lookup key — the whitelist is keyed by
     *      `(product_id, vault, investor)`, not `vault` alone. Pass `vault`'s real,
     *      currently-bound `product_id`; a stale or mismatched one reads back `false`.
     * @param product_id The product `vault` is bound to
     * @param vault      Identifies the tranche whose whitelist is being checked
     * @param investor   The investor's address on vault.chain_id, as bytes32
     */
    function is_tranche_investor(
        uint64 product_id,
        VaultInput calldata vault,
        bytes32 investor
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
