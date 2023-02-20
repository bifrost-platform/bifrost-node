// SPDX-License-Identifier: GPL-3.0-only
pragma solidity >=0.8.0;

/**
 * @title The interface through which solidity contracts will interact with Bfc Staking
 * We follow this same interface including four-byte function selectors, in the precompile that
 * wraps the pallet
 * Address :    0x0000000000000000000000000000000000000400
 */

interface BfcStaking {
    struct candidate_meta_data {
        address candidate;
        address stash;
        uint256 bond;
        uint256 initial_bond;
        uint256 nomination_count;
        uint256 voting_power;
        uint256 lowest_top_nomination_amount;
        uint256 highest_bottom_nomination_amount;
        uint256 lowest_bottom_nomination_amount;
        uint256 top_capacity;
        uint256 bottom_capacity;
        uint256 status;
        bool is_selected;
        uint256 commission;
        uint256 last_block;
        uint256 blocks_produced;
        uint256 productivity;
        uint256 reward_dst;
        uint256 awarded_tokens;
        uint256 tier;
    }

    struct candidate_request_data {
        address candidate;
        uint256 amount;
        uint256 when_executable;
    }

    struct total_stake {
        uint256 total_self_bond;
        uint256 active_self_bond;
        uint256 total_nominations;
        uint256 total_top_nominations;
        uint256 total_bottom_nominations;
        uint256 active_nominations;
        uint256 active_top_nominations;
        uint256 active_bottom_nominations;
        uint256 total_nominators;
        uint256 total_top_nominators;
        uint256 total_bottom_nominators;
        uint256 active_nominators;
        uint256 active_top_nominators;
        uint256 active_bottom_nominators;
        uint256 total_stake;
        uint256 active_stake;
        uint256 total_voting_power;
        uint256 active_voting_power;
    }

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

    /// @dev Check whether the specified address is currently a staking nominator
    /// Selector: 8e5080e7
    /// @param nominator the address that we want to confirm is a nominator
    /// @return A boolean confirming whether the address is a nominator
    function is_nominator(address nominator) external view returns (bool);

    /// @dev Check whether the specified address is currently a validator candidate (full or basic)
    /// Selector: 5245c1e1
    /// @param candidate the address that we want to confirm is a validator andidate
    /// @param tier the type of the validator candidate (0: All, 1: Basic, 2: Full)
    /// @return A boolean confirming whether the address is a validator candidate
    function is_candidate(address candidate, uint256 tier)
        external
        view
        returns (bool);

    /// @dev Check whether the specified address is currently a part of the active set (full or basic)
    /// Selector: 4a079cfd
    /// @param candidate the address that we want to confirm is a part of the active set
    /// @param tier the type of the validator candidate (0: All, 1: Basic, 2: Full)
    /// @return A boolean confirming whether the address is a part of the active set
    function is_selected_candidate(address candidate, uint256 tier)
        external
        view
        returns (bool);

    /// @dev Check whether the specified address elements is currently a part of the active set
    /// Selector: 044527bd
    /// @param candidates the address array that we want to confirm is a part of the active set
    /// @param tier the type of the validator candidate (0: All, 1: Basic, 2: Full)
    /// @return A boolean confirming whether the address array is a part of the active set
    function is_selected_candidates(address[] calldata candidates, uint256 tier)
        external
        view
        returns (bool);

    /// @dev Check whether every specified address element is currently the active set (full or basic)
    /// Selector: 2e8c2a6a
    /// @param candidates the address array that we want to confirm is the active set
    /// @param tier the type of the validator candidate (0: All, 1: Basic, 2: Full)
    /// @return A boolean confirming whether the address array is the active set
    function is_complete_selected_candidates(
        address[] calldata candidates,
        uint256 tier
    ) external view returns (bool);

    /// @dev Check whether the specified address was a part of the given round active set
    /// Selector: 0b32e591
    /// @param round_index the round index that we want to confirm
    /// @param candidate the address that we want to confirm is the active set
    /// @return A boolean confirming whether the address was in the active set
    function is_previous_selected_candidate(
        uint256 round_index,
        address candidate
    ) external view returns (bool);

    /// @dev Check whether the specified address array was a part of the given round active set
    /// Selector: e200c8e3
    /// @param round_index the round index that we want to confirm
    /// @param candidates the address array that we want to confirm is the active set
    /// @return A boolean confirming whether the address array was in the active set
    function is_previous_selected_candidates(
        uint256 round_index,
        address[] calldata candidates
    ) external view returns (bool);

    /// @dev Get the maximum seat capacity of each tier
    /// Selector: 584eda98
    /// @return The maximum seat capacity (full, basic)
    function validator_seats() external view returns (uint256, uint256);

    /// @dev Get the minimum required self-bond amount of each tier
    /// Selector: b877ab9f
    /// @return The minimum required self-bond amount (full, basic)
    function candidate_minimum_self_bond()
        external
        view
        returns (uint256, uint256);

    /// @dev Get the minimum required voting power of each tier
    /// Selector: 4f237d04
    /// @return The minimum required voting power (full, basic)
    function candidate_minimum_voting_power()
        external
        view
        returns (uint256, uint256);

    /// @dev Get the current rounds info
    /// Selector: f8aa8ddd
    /// @return The current rounds index, first session index, current session index,
    ///         first round block, first session block, current block, round length, session length
    function round_info() external view returns (round_meta_data memory);

    /// @dev Get the current rounds index
    /// Selector: 6f31dd98
    /// @return The current rounds index
    function latest_round() external view returns (uint256);

    /// @dev Get the current rounds active validator sets majority
    /// Selector: b6e54bdf
    /// @return The current rounds majority
    function majority() external view returns (uint256);

    /// @dev Get the given rounds active validator sets majority
    /// Selector: e0f9ab40
    /// @return The given rounds majority
    function previous_majority(uint256 round_index)
        external
        view
        returns (uint256);

    /// @dev Total points awarded to all validators in a particular round
    /// Selector: 9799b4e7
    /// @param round_index the round for which we are querying the points total
    /// @return The total points awarded to all validators in the round
    function points(uint256 round_index) external view returns (uint256);

    /// @dev awarded points to the given validator in the given round
    /// Selector: 59a595fb
    /// @param round_index the round for which we are querying the points
    /// @return The awarded points to the validator in the given round
    function validator_points(uint256 round_index, address validator)
        external
        view
        returns (uint256);

    /// @dev The amount of awarded tokens to validators and nominators since genesis
    /// Selector: 9ec5a894
    /// @return The total amount of awarded tokens
    function rewards() external view returns (uint256);

    /// @dev Total capital locked information of self-bonds and nominations of the given round
    /// Selector: b119ebfe
    /// @return The total locked information
    function total(uint256 round_index)
        external
        view
        returns (total_stake memory);

    /// @dev Stake annual inflation parameters
    /// Selector: 10db2de9
    /// @return The annual inflation parameters (min, ideal, max)
    function inflation_config()
        external
        view
        returns (
            uint256,
            uint256,
            uint256
        );

    /// @dev Stake annual inflation rate
    /// Selector: 180692d0
    /// @return The annual inflation rate according to the current stake amount
    function inflation_rate() external view returns (uint256);

    /// @dev The estimated yearly return
    /// Selector: fd0c6dc1
    /// @return The estimated yearly return according to the requested data
    function estimated_yearly_return(
        address[] memory candidates,
        uint256[] memory amounts
    ) external view returns (uint256[] memory);

    /// @dev Get the minimum nomination amount
    /// Selector: c9f593b2
    /// @return The minimum nomination amount
    function min_nomination() external view returns (uint256);

    /// @dev Get the maximum nominations allowed per nominator
    /// Selector: 8b88f0e1
    /// @return The maximum nominations
    function max_nominations_per_nominator() external view returns (uint256);

    /// @dev Get the maximum top and bottom nominations counted per candidate
    /// Selector: 547eaba9
    /// @return The maximum top and bottom nominations per candidate (top, bottom)
    function max_nominations_per_candidate()
        external
        view
        returns (uint256, uint256);

    /// @dev Get the bond less delay information for candidates
    /// Selector: 7abd4bbb
    /// @return The bond less delay for candidates (`LeaveCandidatesDelay`, `CandidateBondLessDelay`)
    function candidate_bond_less_delay()
        external
        view
        returns (uint256, uint256);

    /// @dev Get the bond less delay information for nominators
    /// Selector: 804d185e
    /// @return The bond less delay for nominators (`LeaveNominatorsDelay`, `RevokeNominationDelay`, `NominationBondLessDelay`)
    function nominator_bond_less_delay()
        external
        view
        returns (
            uint256,
            uint256,
            uint256
        );

    /// @dev Get the CandidateCount weight hint
    /// Selector: 4b1c4c29
    /// @return The CandidateCount weight hint
    function candidate_count() external view returns (uint256);

    /// @dev Get the current rounds selected candidates
    /// Selector: a5542eea
    /// @param tier the type of the validator candidate (0: All, 1: Basic, 2: Full)
    /// @return The list of the selected candidates
    function selected_candidates(uint256 tier)
        external
        view
        returns (address[] memory);

    /// @dev Get the previous selected candidates of the given round index
    /// Selector: d9c62dc8
    /// @return The list of the previous selected candidates
    function previous_selected_candidates(uint256 round_index)
        external
        view
        returns (address[] memory);

    /// @dev Get the current state of joined validator candidates
    /// Selector: 96b41b5b
    /// @return The list of the joined validator candidates
    function candidate_pool()
        external
        view
        returns (address[] memory, uint256[] memory);

    /// @dev Get the current state of the given candidate
    /// Selector: 36f3b497
    /// @param candidate the address for which we are querying the state
    /// @return The current state of the queried candidate
    function candidate_state(address candidate)
        external
        view
        returns (candidate_meta_data memory);

    /// @dev Get every candidate states
    /// Selector: 3b368c8c
    /// @param tier the type of the validator candidate (0: All, 1: Basic, 2: Full)
    /// @return An array of every candidate states
    function candidate_states(uint256 tier)
        external
        view
        returns (
            address[] memory,
            address[] memory,
            uint256[] memory,
            uint256[] memory,
            uint256[] memory,
            uint256[] memory,
            uint256[] memory,
            uint256[] memory,
            uint256[] memory,
            uint256[] memory,
            uint256[] memory,
            uint256[] memory,
            bool[] memory,
            uint256[] memory,
            uint256[] memory,
            uint256[] memory,
            uint256[] memory,
            uint256[] memory,
            uint256[] memory,
            uint256[] memory
        );

    /// @dev Get every candidate states by the given selector
    /// Selector: d631e15c
    /// @param tier the type of the validator candidate (0: All, 1: Basic, 2: Full)
    /// @param is_selected the boolean for which it is selected for the current round
    /// @return An array of every candidate states that matches the selector
    function candidate_states_by_selection(uint256 tier, bool is_selected)
        external
        view
        returns (
            address[] memory,
            address[] memory,
            uint256[] memory,
            uint256[] memory,
            uint256[] memory,
            uint256[] memory,
            uint256[] memory,
            uint256[] memory,
            uint256[] memory,
            uint256[] memory,
            uint256[] memory,
            uint256[] memory,
            bool[] memory,
            uint256[] memory,
            uint256[] memory,
            uint256[] memory,
            uint256[] memory,
            uint256[] memory,
            uint256[] memory,
            uint256[] memory
        );

    /// @dev Get the request status of the given candidate
    /// Selector: 2e388768
    /// @param candidate the address for which we are querying the state
    /// @return The current status of the queried candidate
    function candidate_request(address candidate)
        external
        view
        returns (candidate_request_data memory);

    /// @dev Get the top nominations of the given candidate
    /// Selector: 2a9cdf2b
    /// @param candidate the address for which we are querying the state
    /// @return The current status of the queried candidate
    function candidate_top_nominations(address candidate)
        external
        view
        returns (
            address,
            uint256,
            address[] memory,
            uint256[] memory
        );

    /// @dev Get the bottom nominations of the given candidate
    /// Selector: 9be794c0
    /// @param candidate the address for which we are querying the state
    /// @return The current status of the queried candidate
    function candidate_bottom_nominations(address candidate)
        external
        view
        returns (
            address,
            uint256,
            address[] memory,
            uint256[] memory
        );

    /// @dev Get the CandidateNominationCount weight hint
    /// Selector: 1c8ad6fe
    /// @param candidate The address for which we are querying the nomination count
    /// @return The number of nominations backing the validator
    function candidate_nomination_count(address candidate)
        external
        view
        returns (uint256);

    /// @dev Get the current state of the given nominator
    /// Selector: 3f97be51
    /// @param nominator the address for which we are querying the state
    /// @return The current state of the queried nominator
    function nominator_state(address nominator)
        external
        view
        returns (
            address,
            uint256,
            uint256,
            uint256,
            uint256,
            address[] memory,
            uint256[] memory,
            uint256[] memory,
            uint256,
            uint256,
            uint256[] memory
        );

    /// @dev Get the pending requests of the given nominator
    /// Selector: 24f81326
    /// @param nominator the address for which we are querying the state
    /// @return The pending requests of the queried nominator
    function nominator_requests(address nominator)
        external
        view
        returns (
            address,
            uint256,
            uint256,
            address[] memory,
            uint256[] memory,
            uint256[] memory,
            uint256[] memory
        );

    /// @dev Get the NominatorNominationCount weight hint
    /// Selector: dae5659b
    /// @param nominator The address for which we are querying the nomination count
    /// @return The number of nominations made by the nominator
    function nominator_nomination_count(address nominator)
        external
        view
        returns (uint256);

    /// @dev Temporarily leave the set of validator candidates without unbonding
    /// Selector: 767e0450
    function go_offline() external;

    /// @dev Rejoin the set of validator candidates if previously had called `go_offline`
    /// Selector: d2f73ceb
    function go_online() external;

    /// @dev Join the set of validator candidates
    /// Selector: f98e1021
    /// @param controller The paired controller to the stash account
    /// @param relayer The relayer account for cross-chain functionality (optional)
    /// @param amount The amount self-bonded by the caller to become a validator candidate
    /// @param candidateCount The number of candidates in the CandidatePool
    function join_candidates(
        address controller,
        address relayer,
        uint256 amount,
        uint256 candidateCount
    ) external;

    /// @dev Request to bond more for validator candidates
    /// Selector: c57bd3a8
    /// @param more The additional amount self-bonded
    function candidate_bond_more(uint256 more) external;

    /// @dev Request to leave the set of validator candidates
    /// Selector: 60afbac6
    /// @param candidateCount The number of candidates in the CandidatePool
    function schedule_leave_candidates(uint256 candidateCount) external;

    /// @dev Request to bond less for validator candidates
    /// Selector: 034c47bc
    /// @param less The amount to be subtracted from self-bond and unreserved
    function schedule_candidate_bond_less(uint256 less) external;

    /// @dev Execute due request to leave the set of validator candidates
    /// Selector: e33a8f25
    /// @param candidateNominationCount The number of nominations for the candidate to be revoked
    function execute_leave_candidates(uint256 candidateNominationCount)
        external;

    /// @dev Execute pending candidate bond request
    /// Selector: 6c76b502
    function execute_candidate_bond_less() external;

    /// @dev Cancel request to leave the set of validator candidates
    /// Selector: 0880b3e2
    /// @param candidateCount The number of candidates in the CandidatePool
    function cancel_leave_candidates(uint256 candidateCount) external;

    /// @dev Cancel pending candidate bond request
    /// Selector: 583d0fdc
    function cancel_candidate_bond_less() external;

    /// @dev Set the commission ratio of the given candidate
    /// Selector: 6492d2e0
    function set_validator_commission(uint256 commission) external;

    /// @dev Reset the paired controller account of the requested stash
    /// Selector: 91b10ffa
    /// @param new_controller The new controller to be set
    function set_controller(address new_controller) external;

    /// @dev Set the validator candidate reward destination
    /// Selector: 4b4323fb
    /// @param new_reward_dst The new reward destination to be set (Staked = 0, Account = 1)
    function set_candidate_reward_dst(uint256 new_reward_dst) external;

    /// @dev Make a nomination in support of a validator candidate
    /// Selector: 49df6eb3
    /// @param candidate The address of the supported validator candidate
    /// @param amount The amount bonded in support of the validator candidate
    /// @param candidateNominationCount The number of nominations in support of the candidate
    /// @param nominatorNominationCount The number of existing nominations by the caller
    function nominate(
        address candidate,
        uint256 amount,
        uint256 candidateNominationCount,
        uint256 nominatorNominationCount
    ) external;

    /// @dev Bond more for nominators with respect to a specific validator candidate
    /// Selector: 971d44c8
    /// @param candidate The address of the validator candidate for which nomination shall increase
    /// @param more The amount by which the nomination is increased
    function nominator_bond_more(address candidate, uint256 more) external;

    /// @dev Request to leave the set of nominators
    /// Selector: 13153b19
    function schedule_leave_nominators() external;

    /// @dev Request to revoke an existing nomination
    /// Selector: 5b84b7c7
    /// @param candidate The address of the validator candidate which will no longer be supported
    function schedule_revoke_nomination(address candidate) external;

    /// @dev Request to bond less for nominators with respect to a specific validator candidate
    /// Selector: 774bef4d
    /// @param candidate The address of the validator candidate for which nomination shall decrease
    /// @param less The amount by which the nomination is decreased (upon execution)
    function schedule_nominator_bond_less(address candidate, uint256 less)
        external;

    /// @dev Execute request to leave the set of nominators and revoke all nominations
    /// Selector: 4480de22
    /// @param nominatorNominationCount The number of active nominations to be revoked by nominator
    function execute_leave_nominators(uint256 nominatorNominationCount)
        external;

    /// @dev Execute pending nomination request (if exists && is due)
    /// Selector: bfb13332
    /// @param candidate The address of the candidate
    function execute_nomination_request(address candidate) external;

    /// @dev Cancel request to leave the set of nominators
    /// Selector: e48105f0
    function cancel_leave_nominators() external;

    /// @dev Cancel pending nomination request (already made in support of input by caller)
    /// Selector: bdb20cae
    /// @param candidate The address of the candidate
    function cancel_nomination_request(address candidate) external;

    /// @dev Set the nominator reward destination
    /// Selector: 5706390d
    /// @param new_reward_dst The new reward destination to be set (Staked = 0, Account = 1)
    function set_nominator_reward_dst(uint256 new_reward_dst) external;
}
