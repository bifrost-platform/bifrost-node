// SPDX-License-Identifier: GPL-3.0-only
pragma solidity >=0.8.0;

/**
 * @title The interface through which solidity contracts will interact with pallet_bfc_offences
 * We follow this same interface including four-byte function selectors, in the precompile that
 * wraps the pallet
 * Address :    0x0000000000000000000000000000000000000500
 */

interface BfcOffences {
    struct validator_offence_meta_data {
        address validator;
        uint256 latest_offence_round_index;
        uint256 latest_offence_session_index;
        uint256 offence_count;
    }

    /// @dev Get the maximum offence count
    /// Selector: 42caa150
    /// @param tier the type of the validator tier (0: All, 1: Basic, 2: Full)
    /// @return The maximum offence count
    function maximum_offence_count(uint256 tier)
        external
        view
        returns (uint256[] memory);

    /// @dev Get the current offence state of the given validator
    /// Selector: c63c3f8a
    /// @return The current offence state of the given validator
    function validator_offence(address validator)
        external
        view
        returns (validator_offence_meta_data memory);

    /// @dev Get the current offence state of the given validators
    /// Selector: 2962bb0b
    /// @return The current offence state of the given validators
    function validator_offences(address[] calldata validators)
        external
        view
        returns (
            address[] memory,
            uint256[] memory,
            uint256[] memory,
            uint256[] memory
        );
}
