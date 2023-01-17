// SPDX-License-Identifier: GPL-3.0-only
pragma solidity >=0.8.0;

/**
 * @title The interface through which solidity contracts will interact with pallet_balances
 * We follow this same interface including four-byte function selectors, in the precompile that
 * wraps the pallet
 * Address :    0x0000000000000000000000000000000000001000
 */

interface Balances {
    /// @dev Total issuance of the network currency
    /// Selector: 7f5097b7
    /// @return The total issuance
    function total_issuance() external view returns (uint256);
}
