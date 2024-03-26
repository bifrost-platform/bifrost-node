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
    /** Get the members of the collective
     * Selector: bdd4d18d
     */
    function members() external view returns (address[] memory);

    /** Add a new proposal to either be voted on or executed directly.
     * Selector: 5f90ebaf
     *
     * @dev Requires the sender to be member. `threshold` determines whether `proposal`
     * is executed directly (`threshold < 2`) or put up for voting.
     *
     * @param threshold The required amount of aye votes
     * @param proposal The encoded proposal
     */
    function propose(uint256 threshold, bytes memory proposal) external;

    /** Add an aye or nay vote for the sender to the given proposal.
     * Selector: 2c729fd1
     *
     * @dev Transaction fees will be waived if the member is voting on any particular proposal
     * for the first time and the call is successful. Subsequent vote changes will charge a
     * fee.
     *
     * @param proposal_hash The hash of the target proposal
     * @param proposal_index The index of the target proposal
     * @param approve Whether if the vote is for aye or not
     */
    function vote(
        bytes32 proposal_hash,
        uint256 proposal_index,
        bool approve
    ) external;

    /** Close a vote that is either approved, disapproved or whose voting period has ended.
     * Selector: 077bab06
     *
     * @dev May be called by any signed account in order to finish voting and close the proposal.
     * If called before the end of the voting period it will only close the vote if it is
     * has enough votes to be approved or disapproved.
     *
     * If called after the end of the voting period abstentions are counted as rejections
     * unless there is a prime member set and the prime member cast an approval.
     *
     * If the close operation completes successfully with disapproval, the transaction fee will
     * be waived. Otherwise execution of the approved operation will be charged to the caller.
     *
     * @param proposal_hash The hash of the target proposal
     * @param proposal_index The index of the target proposal
     * @param proposal_weight_bound The maximum amount of weight consumed by executing the closed proposal
     * @param length_bound The upper bound for the length of the proposal in storage
     */
    function close(
        bytes32 proposal_hash,
        uint256 proposal_index,
        uint256 proposal_weight_bound,
        uint256 length_bound
    ) external;

    /** Dispatch a proposal from a member using the `Member` origin.
     * Selector: 09c5eabe
     *
     * @dev Origin must be a member of the collective.
     *
     * @param proposal The encoded proposal
     */
    function execute(bytes memory proposal) external;
}
