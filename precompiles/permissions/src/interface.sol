// SPDX-License-Identifier: GPL-3.0-only
pragma solidity >=0.8.0;

/**
 * @title Permissions Precompile Interface
 * @notice Manages the TrancheInvestor whitelist on the Hub chain and propagates
 *         changes to the Spoke chain via the Gateway cross-chain message protocol.
 * Address: 0x0000000000000000000000000000000000000202
 */
interface IPermissions {
    event TrancheInvestorAdded(
        uint64 indexed pool_id,
        uint64 chain_id,
        address vault_address,
        address investor_id
    );
    event TrancheInvestorRemoved(
        uint64 indexed pool_id,
        uint64 chain_id,
        address vault_address,
        address investor_id
    );

    /**
     * @notice Whitelist `investor_id` as a TrancheInvestor for the given tranche,
     *         then send a grant_tranche_investor message to the Spoke chain Gateway.
     * @dev Caller must hold the PoolAdmin role for `pool_id`.
     *      Emits TrancheInvestorAdded on success.
     * @param pool_id       Hub pool ID
     * @param chain_id      EVM chain ID where the vault is deployed
     * @param vault_address ERC-7540 vault contract address on the Spoke chain
     * @param investor_id   Investor address to whitelist
     */
    function add_tranche_investor(
        uint64 pool_id,
        uint64 chain_id,
        address vault_address,
        address investor_id
    ) external;

    /**
     * @notice Remove `investor_id` from the TrancheInvestor whitelist and send
     *         a revoke_tranche_investor message to the Spoke chain Gateway.
     * @dev Caller must hold the PoolAdmin role for `pool_id`.
     *      Emits TrancheInvestorRemoved on success.
     * @param pool_id       Hub pool ID
     * @param chain_id      EVM chain ID where the vault is deployed
     * @param vault_address ERC-7540 vault contract address on the Spoke chain
     * @param investor_id   Investor address to remove
     */
    function remove_tranche_investor(
        uint64 pool_id,
        uint64 chain_id,
        address vault_address,
        address investor_id
    ) external;
}

/**
 * @title IGateway (permissions subset)
 * @notice Subset of the Bifrost Gateway contract interface used by the permissions
 *         precompile to propagate whitelist changes cross-chain via CCCP-v2.
 * @dev This interface will be finalised when the full Gateway contract is implemented.
 */
interface IGateway {
    /// @notice Instruct the Spoke chain to add `investor_id` to the whitelist for `vault_address`.
    function grant_tranche_investor(
        uint64 chain_id,
        address vault_address,
        address investor_id
    ) external;

    /// @notice Instruct the Spoke chain to remove `investor_id` from the whitelist for `vault_address`.
    function revoke_tranche_investor(
        uint64 chain_id,
        address vault_address,
        address investor_id
    ) external;
}
