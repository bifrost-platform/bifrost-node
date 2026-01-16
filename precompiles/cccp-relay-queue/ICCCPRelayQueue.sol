// SPDX-License-Identifier: GPL-3.0-only
pragma solidity >=0.8.0;

/**
 * @title The interface through which solidity contracts will interact with CCCP Relay Queue
 * We follow this same interface including four-byte function selectors, in the precompile that
 * wraps the pallet
 * Address :    0x0000000000000000000000000000000000002001
 */

interface ICCCPRelayQueue {
    /// @dev Returns the oracle address for an asset by its asset index hash.
    /// @custom:selector 0a500830
    /// @return The oracle address
    function get_asset_oracle_by_hash(
        bytes32 asset_index_hash
    ) external view returns (address);
}
