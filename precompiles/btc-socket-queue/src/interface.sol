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
    /// @custom:selector 60b55f8f
    /// @return The list of the current pending request's unsigned PSBT bytes
    function unsigned_psbts() external view returns (bytes[] memory);

    /// @dev Returns the finalized PSBT bytes
    /// @custom:selector a848ca0d
    /// @return The list of the finalized PSBT bytes
    function finalized_psbts() external view returns (bytes[] memory);

    /// @dev Returns the socket messages used for the given transaction
    /// @custom:selector d6da279c
    /// @return The list of the socket messages used for the given transaction
    function outbound_tx(bytes32 txid) external view returns (bytes[] memory);

    /// @dev Filter out executable socket messages from the given sequence ID's.
    /// @custom:selector 7cd4510f
    /// @return The list of executable sequence ID's.
    function filter_executable_msgs(
        uint256[] memory sequences
    ) external view returns (uint256[] memory);
}