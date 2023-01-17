// SPDX-License-Identifier: GPL-3.0-only
pragma solidity >=0.8.0;

/**
 * @title The interface through which solidity contracts will interact with pallet_relay_manager
 * We follow this same interface including four-byte function selectors, in the precompile that
 * wraps the pallet
 * Address :    0x0000000000000000000000000000000000002000
 */

interface RelayManager {
    /// @dev Check whether the specified address is currently a part of the relayer pool
    /// Selector: 976a75f1
    /// @param relayer the address that we want to confirm
    /// @return A boolean confirming whether the address is a part of the relayer pool
    function is_relayer(address relayer) external view returns (bool);

    /// @dev Check whether the specified address is currently or was a part of the active relayer set
    /// Selector: b6f0d1d0
    /// @param relayer the address that we want to confirm
    /// @param is_initial the flag that determines whether to confirm for the current state or at the beginning of the current round
    /// @return A boolean confirming whether the address is a part of the active relayer set
    function is_selected_relayer(address relayer, bool is_initial)
        external
        view
        returns (bool);

    /// @dev Check whether the specified address elements is currently a part of the relayer pool
    /// Selector: 1768adb0
    /// @param relayers the address array that we want to confirm
    /// @return A boolean confirming whether the address array is a part of the relayer pool
    function is_relayers(address[] calldata relayers)
        external
        view
        returns (bool);

    /// @dev Check whether the specified address elements is currently or was a part of the active relayer set
    /// Selector: f6bdf202
    /// @param relayers the address array that we want to confirm
    /// @param is_initial the flag that determines whether to confirm for the current state or at the beginning of the current round
    /// @return A boolean confirming whether the address array is a part of the active relayer set
    function is_selected_relayers(address[] calldata relayers, bool is_initial)
        external
        view
        returns (bool);

    /// @dev Check whether the specified address elements is currently or was identical with the active relayer set
    /// Selector: 17cc85f8
    /// @param relayers the address array that we want to confirm
    /// @param is_initial the flag that determines whether to confirm for the current state or at the beginning of the current round
    /// @return A boolean confirming whether the address array is identical with the active relayer set
    function is_complete_selected_relayers(
        address[] calldata relayers,
        bool is_initial
    ) external view returns (bool);

    /// @dev Check whether the specified address was a part of the given round active set
    /// Selector: f8448c30
    /// @param round_index the round index that we want to confirm
    /// @param relayer the address that we want to confirm is the active set
    /// @param is_initial the flag that determines whether to confirm for the current state or at the beginning of the current round
    /// @return A boolean confirming whether the address was in the active set
    function is_previous_selected_relayer(
        uint256 round_index,
        address relayer,
        bool is_initial
    ) external view returns (bool);

    /// @dev Check whether the specified address array was a part of the given round active set
    /// Selector: 39bae210
    /// @param round_index the round index that we want to confirm
    /// @param relayers the address array that we want to confirm is the active set
    /// @param is_initial the flag that determines whether to confirm for the current state or at the beginning of the current round
    /// @return A boolean confirming whether the address array was in the active set
    function is_previous_selected_relayers(
        uint256 round_index,
        address[] calldata relayers,
        bool is_initial
    ) external view returns (bool);

    /// @dev Check whether the given address sent a heartbeat
    /// Selector: 1ace2613
    /// @return A boolean that represents whether the given address has sent a heartbeat in the current session
    function is_heartbeat_pulsed(address relayer) external view returns (bool);

    /// @dev Get the active relayers of the current round
    /// Selector: dcc7e6e0
    /// @param is_initial the flag that determines whether to confirm for the current state or at the beginning of the current round
    /// @return The list of the active relayers
    function selected_relayers(bool is_initial)
        external
        view
        returns (address[] memory);

    /// @dev Get the previous active relayers of the given round
    /// Selector: 6d709a20
    /// @param round_index the round index that we want to confirm
    /// @param is_initial the flag that determines whether to confirm for the current state or at the beginning of the current round
    /// @return The list of the previous selected relayers
    function previous_selected_relayers(uint256 round_index, bool is_initial)
        external
        view
        returns (address[] memory);

    /// @dev Get the current state of joined relayers
    /// Selector: 6e93ba34
    /// @return The list of the joined relayers
    function relayer_pool()
        external
        view
        returns (address[] memory, address[] memory);

    /// @dev Get the active relayer sets majority of the current round
    /// Selector: d2ea63fb
    /// @param is_initial the flag that determines whether to confirm for the current state or at the beginning of the current round
    /// @return The majority of the current round
    function majority(bool is_initial) external view returns (uint256);

    /// @dev Get the active relayer sets majority of the given round
    /// Selector: ea6ce574
    /// @param round_index the round index that we want to confirm
    /// @param is_initial the flag that determines whether to confirm for the current state or at the beginning of the current round
    /// @return The majority of the given round
    function previous_majority(uint256 round_index, bool is_initial)
        external
        view
        returns (uint256);

    /// @dev Get the current rounds index
    /// Selector: 6f31dd98
    /// @return The current rounds index
    function latest_round() external view returns (uint256);

    /// @dev Get the current state of the given relayer
    /// Selector: 3f4e4fae
    /// @return The current state of the given relayer address. (relayer, controller, status)
    function relayer_state(address relayer)
        external
        view
        returns (
            address,
            address,
            uint256
        );

    /// @dev Get the current state of relayers
    /// Selector: a77293f0
    /// @return The current state of relayers registered in the network
    function relayer_states()
        external
        view
        returns (
            address[] memory,
            address[] memory,
            uint256[] memory
        );

    /// @dev Sends a heartbeat that sets relayer unresponsiveness
    /// Selector: 3defb962
    function heartbeat() external;
}
