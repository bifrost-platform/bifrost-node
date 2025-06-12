// SPDX-License-Identifier: GPL-3.0-only
pragma solidity >=0.8.0;

/**
 * @title The interface through which solidity contracts will interact with BLAZE
 * We follow this same interface including four-byte function selectors, in the precompile that
 * wraps the pallet
 * Address :    0x0000000000000000000000000000000000000102
 */

interface Blaze {
    /// @dev Returns BLAZE balance
    /// @custom:selector c1cfb99a
    /// @return The balance
    function get_balance() external view returns (uint256);

    /// @dev Returns whether BLAZE is activated
    /// @custom:selector 0e59cd4b
    /// @return The boolean result
    function is_activated() external view returns (bool);

    /// @dev Returns whether the given UTXO is submittable
    /// @custom:selector 854ac5f0
    /// @return The boolean result
    function is_submittable_utxo(
        bytes32 txid,
        uint256 vout,
        uint256 amount,
        address authority_id
    ) external view returns (bool);

    /// @dev Returns whether the given txid is broadcastable
    /// @custom:selector f9b3b5ee
    /// @return The boolean result
    function is_tx_broadcastable(
        bytes32 txid,
        address authority_id
    ) external view returns (bool);

    /// @dev Returns the entire outbound pool
    /// @custom:selector 5267d815
    /// @return The outbound pool
    function outbound_pool() external view returns (bytes[] memory);
}
