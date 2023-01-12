import BigNumber from 'bignumber.js';
import { expect } from 'chai';
import { numberToHex } from 'web3-utils';

import {
  AMOUNT_FACTOR, DEFAULT_STAKING_AMOUNT, MIN_BASIC_CANDIDATE_STAKING_AMOUNT,
  MIN_FULL_CANDIDATE_STAKING_AMOUNT, MIN_NOMINATOR_STAKING_AMOUNT
} from '../../constants/currency';
import {
  TEST_CONTROLLERS, TEST_RELAYERS, TEST_STASHES
} from '../../constants/keys';
import { describeDevNode } from '../set_dev_node';
import { callPrecompile, sendPrecompileTx } from '../transactions';

const SELECTORS = {
  // Role verifiers
  is_nominator: '8e5080e7',
  is_candidate: '5245c1e1',
  is_selected_candidate: '4a079cfd',
  is_selected_candidates: '044527bd',
  is_complete_selected_candidates: '2e8c2a6a',
  is_previous_selected_candidate: '0b32e591',
  is_previous_selected_candidates: 'e200c8e3',
  // Common storage getters
  round_info: 'f8aa8ddd',
  latest_round: '6f31dd98',
  majority: 'b6e54bdf',
  previous_majority: 'e0f9ab40',
  points: '9799b4e7',
  validator_points: '59a595fb',
  rewards: '9ec5a894',
  total: 'b119ebfe',
  inflation_config: '10db2de9',
  inflation_rate: '180692d0',
  estimated_yearly_return: 'fd0c6dc1',
  min_nomination: 'c9f593b2',
  max_nominations_per_nominator: '8b88f0e1',
  max_nominations_per_candidate: '547eaba9',
  candidate_bond_less_delay: '7abd4bbb',
  nominator_bond_less_delay: '804d185e',
  // Validator storage getters
  candidate_count: '4b1c4c29',
  selected_candidates: 'a5542eea',
  previous_selected_candidates: 'd9c62dc8',
  candidate_pool: '96b41b5b',
  candidate_state: '36f3b497',
  candidate_states: '3b368c8c',
  candidate_states_by_selection: 'd631e15c',
  candidate_request: '2e388768',
  candidate_top_nominations: '2a9cdf2b',
  candidate_bottom_nominations: '9be794c0',
  candidate_nomination_count: '1c8ad6fe',
  // Nominator storage getters
  nominator_state: '3f97be51',
  nominator_requests: '24f81326',
  nominator_nomination_count: 'dae5659b',
  // Common dispatchable methods
  go_offline: '767e0450',
  go_online: 'd2f73ceb',
  // Validator dispatchable methods
  join_candidates: 'f98e1021',
  candidate_bond_more: 'c57bd3a8',
  schedule_leave_candidates: '60afbac6',
  schedule_candidate_bond_less: '034c47bc',
  execute_leave_candidates: 'e33a8f25',
  execute_candidate_bond_less: '6c76b502',
  cancel_leave_candidates: '0880b3e2',
  cancel_candidate_bond_less: '583d0fdc',
  set_validator_commission: '6492d2e0',
  // Nominator dispatchable methods
  nominate: '49df6eb3',
  nominator_bond_more: '971d44c8',
  schedule_leave_nominators: '13153b19',
  schedule_revoke_nomination: '5b84b7c7',
  schedule_nominator_bond_less: '774bef4d',
  execute_leave_nominators: '4480de22',
  execute_nomination_request: 'bfb13332',
  cancel_leave_nominators: 'e48105f0',
  cancel_nomination_request: 'bdb20cae',
};

const PRECOMPILE_ADDRESS = '0x0000000000000000000000000000000000000400';

describeDevNode('precompile_bfc_staking - genesis', (context) => {
  const alith: { public: string, private: string } = TEST_CONTROLLERS[0];

  it('should include candidate to pool', async function () {
    const raw_candidate_pool = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'candidate_pool',
      [],
    );
    const candidate_pool = context.web3.eth.abi.decodeParameters(
      ['address[]', 'uint256[]'],
      raw_candidate_pool.result,
    );
    expect(candidate_pool[0].length).equal(1);
    expect(candidate_pool[0][0].toLowerCase()).equal(alith.public.toLowerCase());
    expect(candidate_pool[1][0]).equal(DEFAULT_STAKING_AMOUNT);
  });

  it('should include alith as a selected candidate', async function () {
    const raw_is_selected_candidate = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'is_selected_candidate',
      [alith.public, '2'],
    );
    expect(Number(raw_is_selected_candidate.result)).equal(1);

    const raw_selected_candidates = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'selected_candidates',
      ['2'],
    );
    const selected_candidates = context.web3.eth.abi.decodeParameters(
      ['address[]'],
      raw_selected_candidates.result,
    )[0];
    expect(selected_candidates.length).equal(1);
    expect(selected_candidates[0].toLowerCase()).equal(alith.public.toLowerCase());
  });
});

describeDevNode('precompile_bfc_staking - join candidates', (context) => {
  const baltathar: { public: string, private: string } = TEST_CONTROLLERS[1];
  const baltatharStash: { public: string, private: string } = TEST_STASHES[1];
  const baltatharRelayer: { public: string, private: string } = TEST_RELAYERS[1];

  it('should fail due to minimum amount constraint', async function () {
    const stakeBelowMin = new BigNumber(MIN_BASIC_CANDIDATE_STAKING_AMOUNT).minus(AMOUNT_FACTOR);

    const block = await sendPrecompileTx(
      context,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      baltatharStash.public,
      baltatharStash.private,
      'join_candidates',
      [baltathar.public, baltatharRelayer.public, numberToHex(stakeBelowMin.toFixed()), numberToHex(1)],
    );

    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0].result);
    expect(receipt.status).equal(false);
  });

  it('should fail due to invalid candidate amount', async function () {
    const stakeBelowMin = new BigNumber(MIN_FULL_CANDIDATE_STAKING_AMOUNT);

    const block = await sendPrecompileTx(
      context,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      baltatharStash.public,
      baltatharStash.private,
      'join_candidates',
      [baltathar.public, baltatharRelayer.public, numberToHex(stakeBelowMin.toFixed()), numberToHex(0)],
    );

    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0].result);
    expect(receipt.status).equal(false);
  });

  it('should successfully join candidate pool', async function () {
    const stakeBelowMin = new BigNumber(MIN_FULL_CANDIDATE_STAKING_AMOUNT);

    const block = await sendPrecompileTx(
      context,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      baltatharStash.public,
      baltatharStash.private,
      'join_candidates',
      [baltathar.public, baltatharRelayer.public, numberToHex(stakeBelowMin.toFixed()), numberToHex(1)],
    );

    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0].result);
    expect(receipt.status).equal(true);

    const raw_candidate_pool = await callPrecompile(
      context,
      baltathar.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'candidate_pool',
      [],
    );
    const candidate_pool = context.web3.eth.abi.decodeParameters(
      ['address[]', 'uint256[]'],
      raw_candidate_pool.result,
    );
    expect(candidate_pool[0].length).equal(2);
    expect(candidate_pool[0]).includes(baltathar.public);
  });
});

describeDevNode('precompile_bfc_staking - candidate stake management', (context) => {
  const alith: { public: string, private: string } = TEST_CONTROLLERS[0];
  const alithStash: { public: string, private: string } = TEST_STASHES[0];

  it('should fail due to non-stash origin', async function () {
    const stake = new BigNumber(MIN_FULL_CANDIDATE_STAKING_AMOUNT);

    const block = await sendPrecompileTx(
      context,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      alith.public,
      alith.private,
      'candidate_bond_more',
      [
        stake.toFixed(),
      ],
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0].result);
    expect(receipt.status).equal(false);
  });

  it('should successfully self bond more stake', async function () {
    const stake = new BigNumber(MIN_FULL_CANDIDATE_STAKING_AMOUNT);

    const block = await sendPrecompileTx(
      context,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      alithStash.public,
      alithStash.private,
      'candidate_bond_more',
      [
        stake.toFixed(),
      ],
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0].result);
    expect(receipt.status).equal(true);
  });
});

describeDevNode('precompile_bfc_staking - join nominators', (context) => {
  const alith: { public: string, private: string } = TEST_CONTROLLERS[0];
  const baltathar: { public: string, private: string } = TEST_CONTROLLERS[1];
  const charleth: { public: string, private: string } = TEST_CONTROLLERS[2];

  it('should fail due to minimum amount constraint', async function () {
    const stakeBelowMin = new BigNumber(MIN_NOMINATOR_STAKING_AMOUNT).minus(AMOUNT_FACTOR);

    const block = await sendPrecompileTx(
      context,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      baltathar.public,
      baltathar.private,
      'nominate',
      [
        alith.public,
        numberToHex(stakeBelowMin.toFixed()),
        numberToHex(0),
        numberToHex(0),
      ],
    );

    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0].result);
    expect(receipt.status).equal(false);
  });

  it('should fail due to wrong candidate', async function () {
    const stake = new BigNumber(MIN_NOMINATOR_STAKING_AMOUNT);

    const block = await sendPrecompileTx(
      context,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      baltathar.public,
      baltathar.private,
      'nominate',
      [
        charleth.public,
        numberToHex(stake.toFixed()),
        numberToHex(0),
        numberToHex(0),
      ],
    );

    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0].result);
    expect(receipt.status).equal(false);
  });

  it('should successfully nominate to alith', async function () {
    const stake = new BigNumber(MIN_NOMINATOR_STAKING_AMOUNT);

    const block = await sendPrecompileTx(
      context,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      baltathar.public,
      baltathar.private,
      'nominate',
      [
        alith.public,
        numberToHex(stake.toFixed()),
        numberToHex(0),
        numberToHex(0),
      ],
    );

    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0].result);
    expect(receipt.status).equal(true);
  });
});

describeDevNode('precompile_bfc_staking - common storage getters', (context) => {

});
