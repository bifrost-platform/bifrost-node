// SPDX-License-Identifier: GPL-3.0-only
pragma solidity >=0.8.0;

/**
 * @title The interface through which solidity contracts will interact with Btc Socket Queue
 * We follow this same interface including four-byte function selectors, in the precompile that
 * wraps the pallet
 * Address :    0x0000000000000000000000000000000000000101
 */

interface BtcSocketQueue {
    /// @dev Returns the whether the authority has submitted the given signed PSBT bytes
    /// @custom:selector db99ae7e
    /// @return The boolean result
    function is_signed_psbt_submitted(
        bytes32 txid,
        bytes memory signed_psbt,
        address authority_id
    ) external view returns (bool);

    /// @dev Returns the current pending request's unsigned PSBT bytes
    /// @custom:selector 60b55f8f
    /// @return The list of the current pending request's unsigned PSBT bytes
    function unsigned_psbts() external view returns (bytes[] memory);

    /// @dev Returns the finalized PSBT bytes
    /// @custom:selector a848ca0d
    /// @return The list of the finalized PSBT bytes
    function finalized_psbts() external view returns (bytes[] memory);

    /// @dev Returns the current pending rollback PSBT bytes
    /// @custom:selector 97edc6ce
    /// @return The list of the rollback PSBT bytes
    function rollback_psbts() external view returns (bytes[] memory);

    /// @dev Returns the rollback request information
    /// @custom:selector a3b23b30
    /// @return The rollback request information
    function rollback_request(
        bytes32 txid
    )
        external
        view
        returns (
            bytes memory,
            address,
            bytes32,
            uint256,
            string memory,
            uint256,
            address[] memory,
            bool[] memory,
            bool
        );

    /// @dev Returns the socket messages used for the given transaction
    /// @custom:selector d6da279c
    /// @return The list of the socket messages used for the given transaction
    function outbound_tx(bytes32 txid) external view returns (bytes[] memory);

    /// @dev Returns the PSBT txid that contains the given socket message.
    /// @custom:selector 2bce5722
    /// @return The PSBT txid
    function sequence_to_tx_hash(
        uint256 sequence
    ) external view returns (bytes32);

    /// @dev Returns the bonded PSBT transaction hash of the given output information.
    /// @custom:selector abbfb5ed
    /// @return The bonded PSBT transaction hash
    function rollback_output(
        bytes32 txid,
        uint256 vout
    ) external view returns (bytes32);

    /// @dev Filter out executable socket messages from the given sequence ID's.
    /// @custom:selector 7cd4510f
    /// @return The list of executable sequence ID's.
    function filter_executable_msgs(
        uint256[] memory sequences
    ) external view returns (uint256[] memory);
}
