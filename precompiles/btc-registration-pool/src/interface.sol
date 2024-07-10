// SPDX-License-Identifier: GPL-3.0-only
pragma solidity >=0.8.0;

/**
 * @title The interface through which solidity contracts will interact with Btc Registration Pool
 * We follow this same interface including four-byte function selectors, in the precompile that
 * wraps the pallet
 * Address :    0x0000000000000000000000000000000000000100
 */

interface BtcRegistrationPool {
    /// @dev A new user registered to the pool and requested a vault address.
    /// @custom:selector 74c27c8e12077f7a75a6835488f5eb938a8f9f8b66aaac0ebcf7dcbb6d1324f2
    /// @param user_bfc_address The registered Bifrost address.
    /// @param refund_address The registered refund address.
    event VaultPending(address user_bfc_address, string refund_address);

    /// @dev Returns the current round number.
    /// @custom:selector 319c068c
    /// @return The current round number.
    function current_round() external view returns (uint32);

    /// @dev Returns the registration information of the user.
    /// @custom:selector a8d1d421
    /// @param user_bfc_address the address that we want to check
    /// @return The registration information
    function registration_info(
        address user_bfc_address,
        uint32 pool_round
    )
        external
        view
        returns (
            address,
            string memory,
            string memory,
            address[] memory,
            bytes[] memory
        );

    /// @dev Returns the current registration pool
    /// @custom:selector cc65f61a
    /// @return The list of the current registration pool (0: Bifrost addresses, 1: refund addresses, 2: vault addresses)
    function registration_pool(uint32 pool_round)
        external
        view
        returns (address[] memory, string[] memory, string[] memory);

    /// @dev Returns the current pending registrations
    /// @custom:selector 752507ef
    /// @return The list of the current pending registrations (0: Bifrost addresses, 1: refund addresses)
    function pending_registrations(uint32 pool_round)
        external
        view
        returns (address[] memory, string[] memory);

    /// @dev Returns the current bonded vault addresses
    /// @custom:selector fd26a335
    /// @return The list of the current bonded vault addresses
    function vault_addresses(uint32 pool_round) external view returns (string[] memory);

    /// @dev Returns the current bonded descriptors
    /// @custom:selector f8f5c229
    /// @return The list of the current bonded descriptors
    function descriptors(uint32 pool_round) external view returns (string[] memory);

    /// @dev Returns the current bonded refund addresses
    /// @custom:selector 4e9b6a3b
    /// @return The list of the current bonded refund addresses
    function refund_addresses(uint32 pool_round) external view returns (string[] memory);

    /// @dev Returns the bonded vault address mapped to the Bifrost address
    /// @custom:selector 414628e3
    /// @param user_bfc_address the address that we want to check
    /// @return A Bitcoin vault address
    function vault_address(
        address user_bfc_address, uint32 pool_round
    ) external view returns (string memory);

    /// @dev Returns the bonded refund address mapped to the Bifrost address
    /// @custom:selector e3c8a422
    /// @param user_bfc_address the address that we want to check
    /// @return A Bitcoin refund address
    function refund_address(
        address user_bfc_address, uint32 pool_round
    ) external view returns (string memory);

    /// @dev Returns the bonded user address mapped to the given vault address
    /// @custom:selector 8d1bc821
    /// @param vault_address the vault address
    /// @return The users Bifrost address
    function user_address(
        string memory vault_address, uint32 pool_round
    ) external view returns (address);

    /// @dev Returns the bonded descriptor(string) mapped to the given vault address
    /// @custom:selector 6ccaec24
    /// @param vault_address the vault or refund address
    /// @return The descriptor (in string)
    function descriptor(
        string memory vault_address, uint32 pool_round
    ) external view returns (string memory);

    /// @dev Join the registration pool and request a Bitcoin vault address.
    /// @custom:selector f65d6a74
    /// @param refund_address The Bitcoin refund address
    function request_vault(string memory refund_address) external;
}
