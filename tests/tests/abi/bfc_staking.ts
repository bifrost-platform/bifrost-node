export const BFC_STAKING_ABI = [
  {
    "inputs": [],
    "name": "cancel_candidate_bond_less",
    "outputs": [],
    "stateMutability": "nonpayable",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "uint256",
        "name": "candidateCount",
        "type": "uint256"
      }
    ],
    "name": "cancel_leave_candidates",
    "outputs": [],
    "stateMutability": "nonpayable",
    "type": "function"
  },
  {
    "inputs": [],
    "name": "cancel_leave_nominators",
    "outputs": [],
    "stateMutability": "nonpayable",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "address",
        "name": "candidate",
        "type": "address"
      }
    ],
    "name": "cancel_nomination_request",
    "outputs": [],
    "stateMutability": "nonpayable",
    "type": "function"
  },
  {
    "inputs": [],
    "name": "candidate_bond_less_delay",
    "outputs": [
      {
        "internalType": "uint256",
        "name": "",
        "type": "uint256"
      },
      {
        "internalType": "uint256",
        "name": "",
        "type": "uint256"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "uint256",
        "name": "more",
        "type": "uint256"
      }
    ],
    "name": "candidate_bond_more",
    "outputs": [],
    "stateMutability": "nonpayable",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "address",
        "name": "candidate",
        "type": "address"
      }
    ],
    "name": "candidate_bottom_nominations",
    "outputs": [
      {
        "components": [
          {
            "internalType": "address",
            "name": "candidate",
            "type": "address"
          },
          {
            "internalType": "uint256",
            "name": "total",
            "type": "uint256"
          },
          {
            "internalType": "address[]",
            "name": "nominators",
            "type": "address[]"
          },
          {
            "internalType": "uint256[]",
            "name": "nominations",
            "type": "uint256[]"
          }
        ],
        "internalType": "struct BfcStaking.candidate_nominations",
        "name": "",
        "type": "tuple"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  },
  {
    "inputs": [],
    "name": "candidate_count",
    "outputs": [
      {
        "internalType": "uint256",
        "name": "",
        "type": "uint256"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "address",
        "name": "candidate",
        "type": "address"
      }
    ],
    "name": "candidate_nomination_count",
    "outputs": [
      {
        "internalType": "uint256",
        "name": "",
        "type": "uint256"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  },
  {
    "inputs": [],
    "name": "candidate_pool",
    "outputs": [
      {
        "components": [
          {
            "internalType": "address[]",
            "name": "candidates",
            "type": "address[]"
          },
          {
            "internalType": "uint256[]",
            "name": "bonds",
            "type": "uint256[]"
          }
        ],
        "internalType": "struct BfcStaking.candidate_pool_meta_data",
        "name": "",
        "type": "tuple"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "address",
        "name": "candidate",
        "type": "address"
      }
    ],
    "name": "candidate_request",
    "outputs": [
      {
        "components": [
          {
            "internalType": "address",
            "name": "candidate",
            "type": "address"
          },
          {
            "internalType": "uint256",
            "name": "amount",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "when_executable",
            "type": "uint256"
          }
        ],
        "internalType": "struct BfcStaking.candidate_request_data",
        "name": "",
        "type": "tuple"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "address",
        "name": "candidate",
        "type": "address"
      }
    ],
    "name": "candidate_state",
    "outputs": [
      {
        "components": [
          {
            "internalType": "address",
            "name": "candidate",
            "type": "address"
          },
          {
            "internalType": "address",
            "name": "stash",
            "type": "address"
          },
          {
            "internalType": "uint256",
            "name": "bond",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "initial_bond",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "nomination_count",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "voting_power",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "lowest_top_nomination_amount",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "highest_bottom_nomination_amount",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "lowest_bottom_nomination_amount",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "top_capacity",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "bottom_capacity",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "status",
            "type": "uint256"
          },
          {
            "internalType": "bool",
            "name": "is_selected",
            "type": "bool"
          },
          {
            "internalType": "uint256",
            "name": "commission",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "last_block",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "blocks_produced",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "productivity",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "reward_dst",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "awarded_tokens",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "tier",
            "type": "uint256"
          }
        ],
        "internalType": "struct BfcStaking.candidate_meta_data",
        "name": "",
        "type": "tuple"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "uint256",
        "name": "tier",
        "type": "uint256"
      }
    ],
    "name": "candidate_states",
    "outputs": [
      {
        "components": [
          {
            "internalType": "address[]",
            "name": "candidates",
            "type": "address[]"
          },
          {
            "internalType": "address[]",
            "name": "stashes",
            "type": "address[]"
          },
          {
            "internalType": "uint256[]",
            "name": "bonds",
            "type": "uint256[]"
          },
          {
            "internalType": "uint256[]",
            "name": "initial_bonds",
            "type": "uint256[]"
          },
          {
            "internalType": "uint256[]",
            "name": "nominations_counts",
            "type": "uint256[]"
          },
          {
            "internalType": "uint256[]",
            "name": "voting_powers",
            "type": "uint256[]"
          },
          {
            "internalType": "uint256[]",
            "name": "lowest_top_nomination_amounts",
            "type": "uint256[]"
          },
          {
            "internalType": "uint256[]",
            "name": "highest_bottom_nomination_amounts",
            "type": "uint256[]"
          },
          {
            "internalType": "uint256[]",
            "name": "lowest_bottom_nomination_amounts",
            "type": "uint256[]"
          },
          {
            "internalType": "uint256[]",
            "name": "top_capacities",
            "type": "uint256[]"
          },
          {
            "internalType": "uint256[]",
            "name": "bottom_capacities",
            "type": "uint256[]"
          },
          {
            "internalType": "uint256[]",
            "name": "status",
            "type": "uint256[]"
          },
          {
            "internalType": "bool[]",
            "name": "is_selected",
            "type": "bool[]"
          },
          {
            "internalType": "uint256[]",
            "name": "commissions",
            "type": "uint256[]"
          },
          {
            "internalType": "uint256[]",
            "name": "last_blocks",
            "type": "uint256[]"
          },
          {
            "internalType": "uint256[]",
            "name": "blocks_produced",
            "type": "uint256[]"
          },
          {
            "internalType": "uint256[]",
            "name": "productivity",
            "type": "uint256[]"
          },
          {
            "internalType": "uint256[]",
            "name": "reward_dst",
            "type": "uint256[]"
          },
          {
            "internalType": "uint256[]",
            "name": "awarded_tokens",
            "type": "uint256[]"
          },
          {
            "internalType": "uint256[]",
            "name": "tiers",
            "type": "uint256[]"
          }
        ],
        "internalType": "struct BfcStaking.candidates_meta_data",
        "name": "",
        "type": "tuple"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "uint256",
        "name": "tier",
        "type": "uint256"
      },
      {
        "internalType": "bool",
        "name": "is_selected",
        "type": "bool"
      }
    ],
    "name": "candidate_states_by_selection",
    "outputs": [
      {
        "components": [
          {
            "internalType": "address[]",
            "name": "candidates",
            "type": "address[]"
          },
          {
            "internalType": "address[]",
            "name": "stashes",
            "type": "address[]"
          },
          {
            "internalType": "uint256[]",
            "name": "bonds",
            "type": "uint256[]"
          },
          {
            "internalType": "uint256[]",
            "name": "initial_bonds",
            "type": "uint256[]"
          },
          {
            "internalType": "uint256[]",
            "name": "nominations_counts",
            "type": "uint256[]"
          },
          {
            "internalType": "uint256[]",
            "name": "voting_powers",
            "type": "uint256[]"
          },
          {
            "internalType": "uint256[]",
            "name": "lowest_top_nomination_amounts",
            "type": "uint256[]"
          },
          {
            "internalType": "uint256[]",
            "name": "highest_bottom_nomination_amounts",
            "type": "uint256[]"
          },
          {
            "internalType": "uint256[]",
            "name": "lowest_bottom_nomination_amounts",
            "type": "uint256[]"
          },
          {
            "internalType": "uint256[]",
            "name": "top_capacities",
            "type": "uint256[]"
          },
          {
            "internalType": "uint256[]",
            "name": "bottom_capacities",
            "type": "uint256[]"
          },
          {
            "internalType": "uint256[]",
            "name": "status",
            "type": "uint256[]"
          },
          {
            "internalType": "bool[]",
            "name": "is_selected",
            "type": "bool[]"
          },
          {
            "internalType": "uint256[]",
            "name": "commissions",
            "type": "uint256[]"
          },
          {
            "internalType": "uint256[]",
            "name": "last_blocks",
            "type": "uint256[]"
          },
          {
            "internalType": "uint256[]",
            "name": "blocks_produced",
            "type": "uint256[]"
          },
          {
            "internalType": "uint256[]",
            "name": "productivity",
            "type": "uint256[]"
          },
          {
            "internalType": "uint256[]",
            "name": "reward_dst",
            "type": "uint256[]"
          },
          {
            "internalType": "uint256[]",
            "name": "awarded_tokens",
            "type": "uint256[]"
          },
          {
            "internalType": "uint256[]",
            "name": "tiers",
            "type": "uint256[]"
          }
        ],
        "internalType": "struct BfcStaking.candidates_meta_data",
        "name": "",
        "type": "tuple"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "address",
        "name": "candidate",
        "type": "address"
      }
    ],
    "name": "candidate_top_nominations",
    "outputs": [
      {
        "components": [
          {
            "internalType": "address",
            "name": "candidate",
            "type": "address"
          },
          {
            "internalType": "uint256",
            "name": "total",
            "type": "uint256"
          },
          {
            "internalType": "address[]",
            "name": "nominators",
            "type": "address[]"
          },
          {
            "internalType": "uint256[]",
            "name": "nominations",
            "type": "uint256[]"
          }
        ],
        "internalType": "struct BfcStaking.candidate_nominations",
        "name": "",
        "type": "tuple"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "address[]",
        "name": "candidates",
        "type": "address[]"
      },
      {
        "internalType": "uint256[]",
        "name": "amounts",
        "type": "uint256[]"
      }
    ],
    "name": "estimated_yearly_return",
    "outputs": [
      {
        "internalType": "uint256[]",
        "name": "",
        "type": "uint256[]"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  },
  {
    "inputs": [],
    "name": "execute_candidate_bond_less",
    "outputs": [],
    "stateMutability": "nonpayable",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "uint256",
        "name": "candidateNominationCount",
        "type": "uint256"
      }
    ],
    "name": "execute_leave_candidates",
    "outputs": [],
    "stateMutability": "nonpayable",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "uint256",
        "name": "nominatorNominationCount",
        "type": "uint256"
      }
    ],
    "name": "execute_leave_nominators",
    "outputs": [],
    "stateMutability": "nonpayable",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "address",
        "name": "candidate",
        "type": "address"
      }
    ],
    "name": "execute_nomination_request",
    "outputs": [],
    "stateMutability": "nonpayable",
    "type": "function"
  },
  {
    "inputs": [],
    "name": "go_offline",
    "outputs": [],
    "stateMutability": "nonpayable",
    "type": "function"
  },
  {
    "inputs": [],
    "name": "go_online",
    "outputs": [],
    "stateMutability": "nonpayable",
    "type": "function"
  },
  {
    "inputs": [],
    "name": "inflation_config",
    "outputs": [
      {
        "internalType": "uint256",
        "name": "",
        "type": "uint256"
      },
      {
        "internalType": "uint256",
        "name": "",
        "type": "uint256"
      },
      {
        "internalType": "uint256",
        "name": "",
        "type": "uint256"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  },
  {
    "inputs": [],
    "name": "inflation_rate",
    "outputs": [
      {
        "internalType": "uint256",
        "name": "",
        "type": "uint256"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "address",
        "name": "candidate",
        "type": "address"
      },
      {
        "internalType": "uint256",
        "name": "tier",
        "type": "uint256"
      }
    ],
    "name": "is_candidate",
    "outputs": [
      {
        "internalType": "bool",
        "name": "",
        "type": "bool"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "address[]",
        "name": "candidates",
        "type": "address[]"
      },
      {
        "internalType": "uint256",
        "name": "tier",
        "type": "uint256"
      }
    ],
    "name": "is_complete_selected_candidates",
    "outputs": [
      {
        "internalType": "bool",
        "name": "",
        "type": "bool"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "address",
        "name": "nominator",
        "type": "address"
      }
    ],
    "name": "is_nominator",
    "outputs": [
      {
        "internalType": "bool",
        "name": "",
        "type": "bool"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "uint256",
        "name": "round_index",
        "type": "uint256"
      },
      {
        "internalType": "address",
        "name": "candidate",
        "type": "address"
      }
    ],
    "name": "is_previous_selected_candidate",
    "outputs": [
      {
        "internalType": "bool",
        "name": "",
        "type": "bool"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "uint256",
        "name": "round_index",
        "type": "uint256"
      },
      {
        "internalType": "address[]",
        "name": "candidates",
        "type": "address[]"
      }
    ],
    "name": "is_previous_selected_candidates",
    "outputs": [
      {
        "internalType": "bool",
        "name": "",
        "type": "bool"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "address",
        "name": "candidate",
        "type": "address"
      },
      {
        "internalType": "uint256",
        "name": "tier",
        "type": "uint256"
      }
    ],
    "name": "is_selected_candidate",
    "outputs": [
      {
        "internalType": "bool",
        "name": "",
        "type": "bool"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "address[]",
        "name": "candidates",
        "type": "address[]"
      },
      {
        "internalType": "uint256",
        "name": "tier",
        "type": "uint256"
      }
    ],
    "name": "is_selected_candidates",
    "outputs": [
      {
        "internalType": "bool",
        "name": "",
        "type": "bool"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "address",
        "name": "controller",
        "type": "address"
      },
      {
        "internalType": "address",
        "name": "relayer",
        "type": "address"
      },
      {
        "internalType": "uint256",
        "name": "amount",
        "type": "uint256"
      },
      {
        "internalType": "uint256",
        "name": "candidateCount",
        "type": "uint256"
      }
    ],
    "name": "join_candidates",
    "outputs": [],
    "stateMutability": "nonpayable",
    "type": "function"
  },
  {
    "inputs": [],
    "name": "latest_round",
    "outputs": [
      {
        "internalType": "uint256",
        "name": "",
        "type": "uint256"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  },
  {
    "inputs": [],
    "name": "majority",
    "outputs": [
      {
        "internalType": "uint256",
        "name": "",
        "type": "uint256"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  },
  {
    "inputs": [],
    "name": "max_nominations_per_candidate",
    "outputs": [
      {
        "internalType": "uint256",
        "name": "",
        "type": "uint256"
      },
      {
        "internalType": "uint256",
        "name": "",
        "type": "uint256"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  },
  {
    "inputs": [],
    "name": "max_nominations_per_nominator",
    "outputs": [
      {
        "internalType": "uint256",
        "name": "",
        "type": "uint256"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  },
  {
    "inputs": [],
    "name": "min_nomination",
    "outputs": [
      {
        "internalType": "uint256",
        "name": "",
        "type": "uint256"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "address",
        "name": "candidate",
        "type": "address"
      },
      {
        "internalType": "uint256",
        "name": "amount",
        "type": "uint256"
      },
      {
        "internalType": "uint256",
        "name": "candidateNominationCount",
        "type": "uint256"
      },
      {
        "internalType": "uint256",
        "name": "nominatorNominationCount",
        "type": "uint256"
      }
    ],
    "name": "nominate",
    "outputs": [],
    "stateMutability": "nonpayable",
    "type": "function"
  },
  {
    "inputs": [],
    "name": "nominator_bond_less_delay",
    "outputs": [
      {
        "internalType": "uint256",
        "name": "",
        "type": "uint256"
      },
      {
        "internalType": "uint256",
        "name": "",
        "type": "uint256"
      },
      {
        "internalType": "uint256",
        "name": "",
        "type": "uint256"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "address",
        "name": "candidate",
        "type": "address"
      },
      {
        "internalType": "uint256",
        "name": "more",
        "type": "uint256"
      }
    ],
    "name": "nominator_bond_more",
    "outputs": [],
    "stateMutability": "nonpayable",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "address",
        "name": "nominator",
        "type": "address"
      }
    ],
    "name": "nominator_nomination_count",
    "outputs": [
      {
        "internalType": "uint256",
        "name": "",
        "type": "uint256"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "address",
        "name": "nominator",
        "type": "address"
      }
    ],
    "name": "nominator_requests",
    "outputs": [
      {
        "components": [
          {
            "internalType": "address",
            "name": "nominator",
            "type": "address"
          },
          {
            "internalType": "uint256",
            "name": "revocations_count",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "less_total",
            "type": "uint256"
          },
          {
            "internalType": "address[]",
            "name": "candidates",
            "type": "address[]"
          },
          {
            "internalType": "uint256[]",
            "name": "amounts",
            "type": "uint256[]"
          },
          {
            "internalType": "uint256[]",
            "name": "when_executables",
            "type": "uint256[]"
          },
          {
            "internalType": "uint256[]",
            "name": "actions",
            "type": "uint256[]"
          }
        ],
        "internalType": "struct BfcStaking.nominator_requests_data",
        "name": "",
        "type": "tuple"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "address",
        "name": "nominator",
        "type": "address"
      }
    ],
    "name": "nominator_state",
    "outputs": [
      {
        "components": [
          {
            "internalType": "address",
            "name": "nominator",
            "type": "address"
          },
          {
            "internalType": "uint256",
            "name": "total",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "status",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "request_revocations_count",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "request_less_total",
            "type": "uint256"
          },
          {
            "internalType": "address[]",
            "name": "candidates",
            "type": "address[]"
          },
          {
            "internalType": "uint256[]",
            "name": "nominations",
            "type": "uint256[]"
          },
          {
            "internalType": "uint256[]",
            "name": "initial_nominations",
            "type": "uint256[]"
          },
          {
            "internalType": "uint256",
            "name": "reward_dst",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "awarded_tokens",
            "type": "uint256"
          },
          {
            "internalType": "uint256[]",
            "name": "awarded_tokens_per_candidate",
            "type": "uint256[]"
          }
        ],
        "internalType": "struct BfcStaking.nominator_meta_data",
        "name": "",
        "type": "tuple"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "uint256",
        "name": "round_index",
        "type": "uint256"
      }
    ],
    "name": "points",
    "outputs": [
      {
        "internalType": "uint256",
        "name": "",
        "type": "uint256"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "uint256",
        "name": "round_index",
        "type": "uint256"
      }
    ],
    "name": "previous_majority",
    "outputs": [
      {
        "internalType": "uint256",
        "name": "",
        "type": "uint256"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "uint256",
        "name": "round_index",
        "type": "uint256"
      }
    ],
    "name": "previous_selected_candidates",
    "outputs": [
      {
        "internalType": "address[]",
        "name": "",
        "type": "address[]"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  },
  {
    "inputs": [],
    "name": "rewards",
    "outputs": [
      {
        "internalType": "uint256",
        "name": "",
        "type": "uint256"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  },
  {
    "inputs": [],
    "name": "round_info",
    "outputs": [
      {
        "components": [
          {
            "internalType": "uint256",
            "name": "current_round_index",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "first_session_index",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "current_session_index",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "first_round_block",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "first_session_block",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "current_block",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "round_length",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "session_length",
            "type": "uint256"
          }
        ],
        "internalType": "struct BfcStaking.round_meta_data",
        "name": "",
        "type": "tuple"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "uint256",
        "name": "less",
        "type": "uint256"
      }
    ],
    "name": "schedule_candidate_bond_less",
    "outputs": [],
    "stateMutability": "nonpayable",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "uint256",
        "name": "candidateCount",
        "type": "uint256"
      }
    ],
    "name": "schedule_leave_candidates",
    "outputs": [],
    "stateMutability": "nonpayable",
    "type": "function"
  },
  {
    "inputs": [],
    "name": "schedule_leave_nominators",
    "outputs": [],
    "stateMutability": "nonpayable",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "address",
        "name": "candidate",
        "type": "address"
      },
      {
        "internalType": "uint256",
        "name": "less",
        "type": "uint256"
      }
    ],
    "name": "schedule_nominator_bond_less",
    "outputs": [],
    "stateMutability": "nonpayable",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "address",
        "name": "candidate",
        "type": "address"
      }
    ],
    "name": "schedule_revoke_nomination",
    "outputs": [],
    "stateMutability": "nonpayable",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "uint256",
        "name": "tier",
        "type": "uint256"
      }
    ],
    "name": "selected_candidates",
    "outputs": [
      {
        "internalType": "address[]",
        "name": "",
        "type": "address[]"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "uint256",
        "name": "new_reward_dst",
        "type": "uint256"
      }
    ],
    "name": "set_candidate_reward_dst",
    "outputs": [],
    "stateMutability": "nonpayable",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "address",
        "name": "new_controller",
        "type": "address"
      }
    ],
    "name": "set_controller",
    "outputs": [],
    "stateMutability": "nonpayable",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "uint256",
        "name": "new_reward_dst",
        "type": "uint256"
      }
    ],
    "name": "set_nominator_reward_dst",
    "outputs": [],
    "stateMutability": "nonpayable",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "uint256",
        "name": "commission",
        "type": "uint256"
      }
    ],
    "name": "set_validator_commission",
    "outputs": [],
    "stateMutability": "nonpayable",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "uint256",
        "name": "round_index",
        "type": "uint256"
      }
    ],
    "name": "total",
    "outputs": [
      {
        "components": [
          {
            "internalType": "uint256",
            "name": "total_self_bond",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "active_self_bond",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "total_nominations",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "total_top_nominations",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "total_bottom_nominations",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "active_nominations",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "active_top_nominations",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "active_bottom_nominations",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "total_nominators",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "total_top_nominators",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "total_bottom_nominators",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "active_nominators",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "active_top_nominators",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "active_bottom_nominators",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "total_stake",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "active_stake",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "total_voting_power",
            "type": "uint256"
          },
          {
            "internalType": "uint256",
            "name": "active_voting_power",
            "type": "uint256"
          }
        ],
        "internalType": "struct BfcStaking.total_stake",
        "name": "",
        "type": "tuple"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  },
  {
    "inputs": [
      {
        "internalType": "uint256",
        "name": "round_index",
        "type": "uint256"
      },
      {
        "internalType": "address",
        "name": "validator",
        "type": "address"
      }
    ],
    "name": "validator_points",
    "outputs": [
      {
        "internalType": "uint256",
        "name": "",
        "type": "uint256"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  }
]
