// SPDX-License-Identifier: GPL-3.0-only
pragma solidity >=0.8.0;

/**
 * @title Pallet Collective Interface
 *
 * The interface through which solidity contracts will interact with collective related pallets
 * Address :    0x0000000000000000000000000000000000000801 - Council
 * Address :    0x0000000000000000000000000000000000000802 - Tech. Comm.
 * Address :    0x0000000000000000000000000000000000000803 - Relay Exec.
 */

interface Collective {
    /** Check whether the given address is a member.
     * Selector: b0c90f90
     */
    function is_member(address who) external view returns (bool);

    /** Get the members of the collective
     * Selector: bdd4d18d
     */
    function members() external view returns (address[] memory);
}
