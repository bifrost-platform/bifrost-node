// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

/// @title IPermissions
/// @notice Manages TrancheInvestor whitelist on the Hub chain and propagates
///         changes to the Spoke chain via the Gateway cross-chain message protocol.
/// @dev    Precompile address: 0x0000000000000000000000000000000000000202
interface IPermissions {
    /// @notice Whitelist `investor` as a TrancheInvestor for the given tranche,
    ///         then send a grantTrancheInvestor message to the Spoke chain Gateway.
    /// @dev    Caller must hold the PoolAdmin role for `poolId`.
    /// @param  poolId       Hub pool ID
    /// @param  chainId      EVM chain ID where the vault is deployed
    /// @param  vaultAddress ERC-7540 vault contract address on the Spoke chain
    /// @param  investor_id  Investor address to whitelist
    function addTrancheInvestor(
        uint64 poolId,
        uint64 chainId,
        address vaultAddress,
        address investor_id
    ) external;

    /// @notice Remove `investor` from the TrancheInvestor whitelist and send
    ///         a revokeTrancheInvestor message to the Spoke chain Gateway.
    /// @dev    Caller must hold the PoolAdmin role for `poolId`.
    /// @param  poolId       Hub pool ID
    /// @param  chainId      EVM chain ID where the vault is deployed
    /// @param  vaultAddress ERC-7540 vault contract address on the Spoke chain
    /// @param  investor_id  Investor address to remove
    function removeTrancheInvestor(
        uint64 poolId,
        uint64 chainId,
        address vaultAddress,
        address investor_id
    ) external;

    /// @notice Emitted when an investor is successfully whitelisted.
    event TrancheInvestorAdded(
        uint64 indexed poolId,
        uint64 chainId,
        address vaultAddress,
        address investor_id
    );

    /// @notice Emitted when an investor is removed from the whitelist.
    event TrancheInvestorRemoved(
        uint64 indexed poolId,
        uint64 chainId,
        address vaultAddress,
        address investor_id
    );
}

/// @title IGateway (permissions subset)
/// @notice Subset of the Bifrost Gateway contract interface used by this precompile
///         to propagate whitelist changes cross-chain via CCCP-v2.
/// @dev    This interface will be finalised when the full Gateway contract is implemented.
interface IGateway {
    /// @notice Instruct the Spoke chain to add `investor` to the Whitelist for `vaultAddress`.
    function grantTrancheInvestor(
        uint64 chainId,
        address vaultAddress,
        address investor_id
    ) external;

    /// @notice Instruct the Spoke chain to remove `investor` from the Whitelist for `vaultAddress`.
    function revokeTrancheInvestor(
        uint64 chainId,
        address vaultAddress,
        address investor_id
    ) external;
}
