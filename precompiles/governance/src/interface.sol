// SPDX-License-Identifier: GPL-3.0-only
pragma solidity >=0.8.0;

/**
 * @title Pallet Governance Interface
 *
 * The interface through which solidity contracts will interact with governance related pallets
 * Address :    0x0000000000000000000000000000000000000800
 */

interface Governance {
    struct deposit_of_meta_data {
        uint256 total_deposit;
        uint256 initial_deposit;
        address[] depositors;
    }

    struct voting_of_meta_data {
        uint256 ref_index;
        address[] voters;
        uint256[] voting_powers;
        bool[] voting_sides;
        uint256[] convictions;
    }

    struct account_votes_meta_data {
        uint256[] ref_index;
        uint256[] raw_votes;
        bool[] voting_sides;
        uint256[] convictions;
        uint256 delegated_votes;
        uint256 delegated_raw_votes;
        uint256 lock_expired_at;
        uint256 lock_balance;
    }

    struct ongoing_referendum_info_meta_data {
        uint256 end;
        bytes32 proposal_hash;
        uint256 threshold;
        uint256 delay;
        uint256 ayes;
        uint256 nays;
        uint256 turnout;
    }

    struct finished_referendum_info_meta_data {
        bool approved;
        uint256 end;
    }

    /**
     * Get The total number of public proposals
     * Selector: 56fdf547
     * @return The total number of public proposals
     */
    function public_prop_count() external view returns (uint256);

    /**
     * Get details of the deposit for a proposal.
     * Selector: a30305e9
     * @param prop_index The index of the proposal you are interested in
     * @return (total deposit, initial deposit, depositors)
     */
    function deposit_of(uint256 prop_index)
        external
        view
        returns (deposit_of_meta_data memory);

    /**
     * Get details of the votes for a referendum.
     * Selector: 09daa4d8
     * @param ref_index The index of the referendum you are interested in
     * @return (referenda index, voters, voting powers, voting sides, convictions)
     */
    function voting_of(uint256 ref_index)
        external
        view
        returns (voting_of_meta_data memory);

    /**
     * Get details of the votes for the given account.
     * Selector: 198a1bd9
     * @param account The account address you are interested in
     * @return A tuple including:
     * * The index of voted referendas (removable)
     * * The raw votes submitted for each referenda (conviction not applied)
     * * The voting side of each referenda (true: aye, false: nay)
     * * The conviction multiplier of each votes (0~6)
     * * The delegated amount of votes received for this account (conviction applied)
     * * The delegated raw amount of votes received for this account (conviction not applied)
     * * The block number that expires the locked balance
     * * The balance locked to the network
     */
    function account_votes(address account)
        external
        view
        returns (account_votes_meta_data memory);

    /**
     * Get the index of the lowest unbaked referendum
     * Selector: 0388f282
     * @return The lowest referendum index representing an unbaked referendum.
     */
    function lowest_unbaked() external view returns (uint256);

    /**
     * Get the details about an ongoing referendum.
     * Selector: 8b93d11a
     *
     * @dev This, along with `finished_referendum_info`, wraps the pallet's `referendum_info`
     * function. It is necessary to split it into two here because Solidity only has c-style enums.
     * @param ref_index The index of the referendum you are interested in
     * @return A tuple including:
     * * The block when voting on this referendum will end
     * * The proposal hash
     * * The biasing mechanism 0-SuperMajorityApprove, 1-SuperMajorityAgainst, 2-SimpleMajority
     * * The delay between passing and launching
     * * The total aye vote (including conviction)
     * * The total nay vote (including conviction)
     * * The total turnout (not including conviction)
     */
    function ongoing_referendum_info(uint256 ref_index)
        external
        view
        returns (ongoing_referendum_info_meta_data memory);

    /**
     * Get the details about a finished referendum.
     * Selector: b1fd383f
     *
     * @dev This, along with `ongoing_referendum_info`, wraps the pallet's `referendum_info`
     * function. It is necessary to split it into two here because Solidity only has c-style enums.
     * @param ref_index The index of the referendum you are interested in
     * @return A tuple including whether the referendum passed, and the block at which it finished.
     */
    function finished_referendum_info(uint256 ref_index)
        external
        view
        returns (finished_referendum_info_meta_data memory);

    /**
     * Make a new proposal
     * Selector: 7824e7d1
     *
     * @param proposal_hash The hash of the proposal you are making
     * @param value The number of tokens to be locked behind this proposal.
     */
    function propose(bytes32 proposal_hash, uint256 value) external;

    /**
     * Signal agreement (endorsement) with a proposal
     * Selector: c7a76601
     *
     * @dev No amount is necessary here. Seconds are always for the same amount that the original
     * proposer locked. You may second multiple times.
     *
     * @param prop_index index of the proposal you want to second
     * @param seconds_upper_bound A number greater than or equal to the current number of seconds.
     * This is necessary for calculating the weight of the call.
     */
    function second(uint256 prop_index, uint256 seconds_upper_bound) external;

    /**
     * Vote on a referendum.
     * Selector: f56cb3b3
     *
     * @param ref_index index of the referendum you want to vote in
     * @param aye `true` is a vote to enact the proposal; `false` is a vote to keep the status quo.
     * @param vote_amount The number of tokens you are willing to lock if you get your way
     * @param conviction How strongly you want to vote. Higher conviction means longer lock time.
     * This must be an integer in the range 0 to 6
     *
     * @dev This function only supposrts `Standard` votes where you either vote aye or nay.
     * It does not support `Split` votes where you vote on both sides. If such a need
     * arises, we should add an additional function to this interface called `split_vote`.
     */
    function vote(
        uint256 ref_index,
        bool aye,
        uint256 vote_amount,
        uint256 conviction
    ) external;

    /** Remove a vote for a referendum.
     * Selector: 2042f50b
     *
     * @dev Locks get complex when votes are removed. See pallet-democracy's docs for details.
     * @param ref_index The index of the referendum you are interested in
     */
    function remove_vote(uint256 ref_index) external;

    /**
     * Delegate voting power to another account.
     * Selector: 0185921e
     *
     * @dev The balance delegated is locked for as long as it is delegated, and thereafter for the
     * time appropriate for the conviction's lock period.
     * @param target The account to whom the vote shall be delegated.
     * @param conviction The conviction with which you are delegating.
     * This must be an integer in the range 0 to 6
     * @param amount The number of tokens whose voting power shall be delegated.
     */
    function delegate(
        address target,
        uint256 conviction,
        uint256 amount
    ) external;

    /**
     * Undelegate voting power
     * Selector: cb37b8ea
     *
     * @dev Tokens may be unlocked once the lock period corresponding to the conviction with which
     * the delegation was issued has elapsed.
     */
    function undelegate() external;

    /**
     * Unlock tokens that have an expired lock.
     * Selector: 2f6c493c
     *
     * @param target The account whose tokens should be unlocked. This may be any account.
     */
    function unlock(address target) external;

    /**
     * Register the preimage for an upcoming proposal. This doesn't require the proposal to be
     * in the dispatch queue but does require a deposit, returned once enacted.
     * Selector: 200881f5
     *
     * @param encoded_proposal The scale-encoded proposal whose hash has been submitted on-chain.
     */
    function note_preimage(bytes memory encoded_proposal) external;

    /**
     * Register the preimage for an upcoming proposal. This requires the proposal to be
     * in the dispatch queue. No deposit is needed. When this call is successful, i.e.
     * the preimage has not been uploaded before and matches some imminent proposal,
     * no fee is paid.
     * Selector: cf205f96
     *
     * @param encoded_proposal The scale-encoded proposal whose hash has been submitted on-chain.
     */
    function note_imminent_preimage(bytes memory encoded_proposal) external;
}
