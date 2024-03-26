// SPDX-License-Identifier: GPL-3.0-only
pragma solidity >=0.8.0;

/**
 * @title The interface through which solidity contracts will interact with Btc Socket Queue
 * We follow this same interface including four-byte function selectors, in the precompile that
 * wraps the pallet
 * Address :    0x0000000000000000000000000000000000000101
 */

interface BtcSocketQueue {
    /// @dev Returns the current pending request's unsigned PSBT bytes
    /// @custom:selector e9db6a30
    /// @return The list of the current pending request's unsigned PSBT bytes
    function get_unsigned_psbts() external view returns (bytes[] memory);
}
