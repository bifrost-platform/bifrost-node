// SPDX-License-Identifier: GPL-3.0-only
pragma solidity >=0.8.0;

/**
 * @title The interface through which solidity contracts will interact with pallet_permissioned_authority
 * We follow this same interface including four-byte function selectors, in the precompile that
 * wraps the pallet
 * Address :    0x0000000000000000000000000000000000000400
 *
 * Deployed at the `bfc-staking` precompile's address on chains that replace staking with a
 * root-managed authority set. `round_info()` / `latest_round()` keep the exact `BfcStaking`
 * ABI (selectors and return layout), so CCCP relayers read it as their `authority_address`
 * unchanged. Rounds only advance when the authority (or relayer) set changes; `round_length`
 * is a nominal value.
 *
 * Every function is also exposed under its camelCase name (e.g. `roundInfo()`), like the
 * `BfcStaking` precompile.
 */

interface PermissionedAuthority {
    struct round_meta_data {
        uint256 current_round_index;
        uint256 first_session_index;
        uint256 current_session_index;
        uint256 first_round_block;
        uint256 first_session_block;
        uint256 current_block;
        uint256 round_length;
        uint256 session_length;
    }

    /// @dev Get the current rounds info
    /// Selector: f8aa8ddd (camelCase `roundInfo()`: cace12e6)
    /// @return The current rounds index, first session index, current session index,
    ///         first round block, first session block, current block, round length (nominal),
    ///         session length
    function round_info() external view returns (round_meta_data memory);

    /// @dev Get the current rounds index
    /// Selector: 6f31dd98 (camelCase `latestRound()`: 668a0f02)
    /// @return The current rounds index
    function latest_round() external view returns (uint256);

    /// @dev Get the validators of the current round
    /// Selector: b021fa3c (camelCase `activeAuthorities()`: ff15e011)
    /// @return The active authority (validator) addresses
    function active_authorities() external view returns (address[] memory);

    /// @dev Get the requested validators, applied at the next session rotation
    /// Selector: 2b0d1816
    /// @return The requested authority (validator) addresses
    function authorities() external view returns (address[] memory);

    /// @dev Check whether the given address is a validator of the current round
    /// Selector: 21a5223a (camelCase `isActiveAuthority(address)`: cb38e1a9)
    /// @param who the address that we want to confirm
    /// @return A boolean confirming whether the address is an active authority
    function is_active_authority(address who) external view returns (bool);
}
