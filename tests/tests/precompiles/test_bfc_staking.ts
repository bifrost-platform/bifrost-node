import BigNumber from 'bignumber.js';
import { expect } from 'chai';
import { numberToHex } from 'web3-utils';

import { MIN_FULL_CANDIDATE_STAKING_AMOUNT } from '../../constants/currency';
import {
  TEST_CONTROLLERS, TEST_RELAYERS, TEST_STASHES
} from '../../constants/keys';
import { BFC_STAKING_ABI } from '../abi/bfc_staking';
import { describeDevNode } from '../set_dev_node';
import { callPrecompile, sendPrecompileTx } from '../transactions';
import { jumpToRound } from '../utils';

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

describeDevNode('precompile_bfc_staking - precompile view functions', (context) => {
  const alith: { public: string, private: string } = TEST_CONTROLLERS[0];

  it('should successfully verify validator/candidate roles', async function () {
    const is_candidate = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'is_candidate',
      [alith.public, '0x0'],
    );
    const decoded_is_candidate = context.web3.eth.abi.decodeParameters(
      ['bool'],
      is_candidate.result,
    )[0];
    expect(decoded_is_candidate).equal(true);

    const is_selected_candidate = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'is_selected_candidate',
      [alith.public, '0x0'],
    );
    const decoded_is_selected_candidate = context.web3.eth.abi.decodeParameters(
      ['bool'],
      is_selected_candidate.result,
    )[0];
    expect(decoded_is_selected_candidate).equal(true);

    const is_previous_selected_candidate = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'is_previous_selected_candidate',
      ['0x1', alith.public],
    );
    const decoded_is_previous_selected_candidate = context.web3.eth.abi.decodeParameters(
      ['bool'],
      is_previous_selected_candidate.result,
    )[0];
    expect(decoded_is_previous_selected_candidate).equal(true);
  });

  it('should successfully verify common storage existance', async function () {
    const round_info = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'round_info',
      [],
    );
    const decoded_round_info = context.web3.eth.abi.decodeParameters(
      ['tuple(uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256)'],
      round_info.result,
    )[0];
    expect(Number(decoded_round_info[0])).equal(1);

    const latest_round = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'latest_round',
      [],
    );
    const decoded_latest_round = context.web3.eth.abi.decodeParameters(
      ['uint256'],
      latest_round.result,
    )[0];
    expect(Number(decoded_latest_round)).equal(1);

    const majority = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'majority',
      [],
    );
    const decoded_majority = context.web3.eth.abi.decodeParameters(
      ['uint256'],
      majority.result,
    )[0];
    expect(Number(decoded_majority)).equal(1);

    const previous_majority = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'previous_majority',
      ['0x1'],
    );
    const decoded_previous_majority = context.web3.eth.abi.decodeParameters(
      ['uint256'],
      previous_majority.result,
    )[0];
    expect(Number(decoded_previous_majority)).equal(1);

    await context.createBlock();
    const points = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'points',
      ['0x1'],
    );
    const decoded_points = context.web3.eth.abi.decodeParameters(
      ['uint256'],
      points.result,
    )[0];
    expect(Number(decoded_points)).greaterThan(1);

    const validator_points = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'validator_points',
      ['0x1', alith.public],
    );
    const decoded_validator_points = context.web3.eth.abi.decodeParameters(
      ['uint256'],
      validator_points.result,
    )[0];
    expect(Number(decoded_validator_points)).greaterThan(1);

    await jumpToRound(context, 2);
    const rewards = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'rewards',
      [],
    );
    const decoded_rewards = context.web3.eth.abi.decodeParameters(
      ['uint256'],
      rewards.result,
    )[0];
    expect(Number(decoded_rewards)).greaterThan(1);

    const total = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'total',
      ['0x2'],
    );
    const decoded_total = context.web3.eth.abi.decodeParameters(
      [
        'tuple(uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256)',
      ],
      total.result,
    )[0];
    expect(Number(decoded_total[0])).greaterThan(1);

    const inflation_config = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'inflation_config',
      [],
    );
    const decoded_inflation_config = context.web3.eth.abi.decodeParameters(
      ['uint256', 'uint256', 'uint256'],
      inflation_config.result,
    );
    expect(Number(decoded_inflation_config.__length__)).equal(3);
    expect(decoded_inflation_config[0]).equal(new BigNumber(7).multipliedBy(10 ** 7).toFixed());
    expect(decoded_inflation_config[1]).equal(new BigNumber(13).multipliedBy(10 ** 7).toFixed());
    expect(decoded_inflation_config[2]).equal(new BigNumber(15).multipliedBy(10 ** 7).toFixed());

    const inflation_rate = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'inflation_rate',
      [],
    );
    const decoded_inflation_rate = context.web3.eth.abi.decodeParameters(
      ['uint256'],
      inflation_rate.result,
    );
    expect(decoded_inflation_rate[0]).equal(new BigNumber(13).multipliedBy(10 ** 7).toFixed());

    const min_nomination = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'min_nomination',
      [],
    );
    const decoded_min_nomination = context.web3.eth.abi.decodeParameters(
      ['uint256'],
      min_nomination.result,
    );
    expect(decoded_min_nomination[0]).equal(new BigNumber(1).multipliedBy(10 ** 18).toFixed());

    const max_nominations_per_nominator = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'max_nominations_per_nominator',
      [],
    );
    const decoded_max_nominations_per_nominator = context.web3.eth.abi.decodeParameters(
      ['uint256'],
      max_nominations_per_nominator.result,
    );
    expect(Number(decoded_max_nominations_per_nominator[0])).equal(3);

    const max_nominations_per_candidate = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'max_nominations_per_candidate',
      [],
    );
    const decoded_max_nominations_per_candidate = context.web3.eth.abi.decodeParameters(
      ['uint256', 'uint256'],
      max_nominations_per_candidate.result,
    );
    expect(decoded_max_nominations_per_candidate.__length__).equal(2);
    expect(Number(decoded_max_nominations_per_candidate[0])).equal(10);
    expect(Number(decoded_max_nominations_per_candidate[1])).equal(2);

    const candidate_bond_less_delay = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'candidate_bond_less_delay',
      [],
    );
    const decoded_candidate_bond_less_delay = context.web3.eth.abi.decodeParameters(
      ['uint256', 'uint256'],
      candidate_bond_less_delay.result,
    );
    expect(decoded_candidate_bond_less_delay.__length__).equal(2);
    expect(Number(decoded_candidate_bond_less_delay[0])).equal(1);
    expect(Number(decoded_candidate_bond_less_delay[1])).equal(1);

    const nominator_bond_less_delay = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'nominator_bond_less_delay',
      [],
    );
    const decoded_nominator_bond_less_delay = context.web3.eth.abi.decodeParameters(
      ['uint256', 'uint256', 'uint256'],
      nominator_bond_less_delay.result,
    );
    expect(decoded_nominator_bond_less_delay.__length__).equal(3);
    expect(Number(decoded_nominator_bond_less_delay[0])).equal(1);
    expect(Number(decoded_nominator_bond_less_delay[1])).equal(1);
    expect(Number(decoded_nominator_bond_less_delay[2])).equal(1);
  });

  it('should successfully verify validator/candidate storage existance', async function () {
    const candidate_count = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'candidate_count',
      [],
    );
    const decoded_candidate_count = context.web3.eth.abi.decodeParameters(
      ['uint256'],
      candidate_count.result,
    );
    expect(Number(decoded_candidate_count[0])).equal(1);

    const selected_candidates = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'selected_candidates',
      ['0x0'],
    );
    const decoded_selected_candidates = context.web3.eth.abi.decodeParameters(
      ['address[]'],
      selected_candidates.result,
    )[0];
    expect(decoded_selected_candidates.length).equal(1);
    expect(decoded_selected_candidates[0]).equal(alith.public);

    const previous_selected_candidates = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'previous_selected_candidates',
      ['0x2'],
    );
    const decoded_previous_selected_candidates = context.web3.eth.abi.decodeParameters(
      ['address[]'],
      previous_selected_candidates.result,
    )[0];
    expect(decoded_previous_selected_candidates.length).equal(1);
    expect(decoded_previous_selected_candidates[0]).equal(alith.public);

    const candidate_pool = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'candidate_pool',
      [],
    );
    const decoded_candidate_pool = context.web3.eth.abi.decodeParameters(
      ['address[]', 'uint256[]'],
      candidate_pool.result,
    );
    expect(decoded_candidate_pool[0].length).equal(1);
    expect(decoded_candidate_pool[1].length).equal(1);
    expect(decoded_candidate_pool[0][0]).equal(alith.public);

    const candidate_state = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'candidate_state',
      [alith.public],
    );
    const decoded_candidate_state = context.web3.eth.abi.decodeParameters(
      [
        'tuple(address,address,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256,bool,uint256,uint256,uint256,uint256,uint256,uint256,uint256)',
      ],
      candidate_state.result,
    )[0];
    expect(decoded_candidate_state[0]).equal(alith.public);

    const candidate_states = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'candidate_states',
      ['0x0'],
    );
    const decoded_candidate_states = context.web3.eth.abi.decodeParameters(
      [
        'address[]',
        'address[]',
        'uint256[]',
        'uint256[]',
        'uint256[]',
        'uint256[]',
        'uint256[]',
        'uint256[]',
        'uint256[]',
        'uint256[]',
        'uint256[]',
        'uint256[]',
        'bool[]',
        'uint256[]',
        'uint256[]',
        'uint256[]',
        'uint256[]',
        'uint256[]',
        'uint256[]',
        'uint256[]',
      ],
      candidate_states.result,
    );
    expect(decoded_candidate_states[0][0]).equal(alith.public);

    const candidate_states_by_selection = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'candidate_states_by_selection',
      ['0x0', '0x1'],
    );
    const decoded_candidate_states_by_selection = context.web3.eth.abi.decodeParameters(
      [
        'address[]',
        'address[]',
        'uint256[]',
        'uint256[]',
        'uint256[]',
        'uint256[]',
        'uint256[]',
        'uint256[]',
        'uint256[]',
        'uint256[]',
        'uint256[]',
        'uint256[]',
        'bool[]',
        'uint256[]',
        'uint256[]',
        'uint256[]',
        'uint256[]',
        'uint256[]',
        'uint256[]',
        'uint256[]',
      ],
      candidate_states_by_selection.result,
    );
    expect(decoded_candidate_states_by_selection[0][0]).equal(alith.public);

    const candidate_request = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'candidate_request',
      [alith.public],
    );
    const decoded_candidate_request = context.web3.eth.abi.decodeParameters(
      [
        'tuple(address,uint256,uint256)',
      ],
      candidate_request.result,
    )[0];
    expect(decoded_candidate_request[0]).equal(alith.public);

    const candidate_top_nominations = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'candidate_top_nominations',
      [alith.public],
    );
    const decoded_candidate_top_nominations = context.web3.eth.abi.decodeParameters(
      [
        'address',
        'uint256',
        'address[]',
        'uint256[]',
      ],
      candidate_top_nominations.result,
    );
    expect(decoded_candidate_top_nominations[0]).equal(alith.public);

    const candidate_bottom_nominations = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'candidate_bottom_nominations',
      [alith.public],
    );
    const decoded_candidate_bottom_nominations = context.web3.eth.abi.decodeParameters(
      [
        'address',
        'uint256',
        'address[]',
        'uint256[]',
      ],
      candidate_bottom_nominations.result,
    );
    expect(decoded_candidate_bottom_nominations[0]).equal(alith.public);

    const candidate_nomination_count = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'candidate_nomination_count',
      [alith.public],
    );
    const decoded_candidate_nomination_count = context.web3.eth.abi.decodeParameters(
      [
        'uint256',
      ],
      candidate_nomination_count.result,
    )[0];
    expect(Number(decoded_candidate_nomination_count)).equal(0);
  });

  it('should successfully verify nominator storage existance', async function () {
    const nominator_state = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'nominator_state',
      [alith.public],
    );
    const decoded_nominator_state = context.web3.eth.abi.decodeParameters(
      [
        'address',
        'uint256',
        'uint256',
        'uint256',
        'uint256',
        'address[]',
        'uint256[]',
        'uint256[]',
        'uint256',
        'uint256',
        'uint256[]',
      ],
      nominator_state.result,
    );
    expect(decoded_nominator_state[0]).equal(alith.public);

    const nominator_requests = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'nominator_requests',
      [alith.public],
    );
    const decoded_nominator_requests = context.web3.eth.abi.decodeParameters(
      [
        'address',
        'uint256',
        'uint256',
        'address[]',
        'uint256[]',
        'uint256[]',
        'uint256[]',
      ],
      nominator_requests.result,
    );
    expect(decoded_nominator_requests[0]).equal(alith.public);

    const nominator_nomination_count = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'nominator_nomination_count',
      [alith.public],
    );
    const decoded_nominator_nomination_count = context.web3.eth.abi.decodeParameters(
      ['uint256'],
      nominator_nomination_count.result,
    )[0];
    expect(Number(decoded_nominator_nomination_count)).equal(0);
  });
});

describeDevNode('precompile_bfc_staking - precompile dispatch functions', (context) => {
  const alith: { public: string, private: string } = TEST_CONTROLLERS[0];

  const baltathar: { public: string, private: string } = TEST_CONTROLLERS[1];
  const baltatharStash: { public: string, private: string } = TEST_STASHES[1];
  const baltatharRelayer: { public: string, private: string } = TEST_RELAYERS[1];

  it('should successfully execute `join_candidates()`', async function () {
    const stake = new BigNumber(MIN_FULL_CANDIDATE_STAKING_AMOUNT);

    const block = await sendPrecompileTx(
      context,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      baltatharStash.public,
      baltatharStash.private,
      'join_candidates',
      [baltathar.public, baltatharRelayer.public, numberToHex(stake.toFixed()), numberToHex(1)],
    );

    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0].result);
    expect(receipt.status).equal(true);

    const candidate_pool = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'candidate_pool',
      [],
    );
    const decoded_candidate_pool = context.web3.eth.abi.decodeParameters(
      ['address[]', 'uint256[]'],
      candidate_pool.result,
    );
    expect(decoded_candidate_pool[0].length).equal(2);
    expect(decoded_candidate_pool[1].length).equal(2);
    expect(decoded_candidate_pool[0]).includes(baltathar.public);
  });

  it('should successfully execute `candidate_bond_more()`', async function () {
    const more = new BigNumber(MIN_FULL_CANDIDATE_STAKING_AMOUNT);

    const block = await sendPrecompileTx(
      context,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      baltatharStash.public,
      baltatharStash.private,
      'candidate_bond_more',
      [numberToHex(more.toFixed())],
    );

    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0].result);
    expect(receipt.status).equal(true);

    const candidate_state = await callPrecompile(
      context,
      baltathar.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'candidate_state',
      [baltathar.public],
    );
    const decoded_candidate_state = context.web3.eth.abi.decodeParameters(
      [
        'tuple(address,address,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256,bool,uint256,uint256,uint256,uint256,uint256,uint256,uint256)',
      ],
      candidate_state.result,
    )[0];
    expect(new BigNumber(decoded_candidate_state[2]).toFixed()).equal(new BigNumber(more).multipliedBy(2).toFixed());
  });
});

describeDevNode('precompile_bfc_staking - precompile gas estimation', (context) => {
  const alithStash: { public: string, private: string } = TEST_STASHES[0];

  it('should successfully estimate dispatch functions', async function () {
    const bfc_staking = new context.web3.eth.Contract(BFC_STAKING_ABI, PRECOMPILE_ADDRESS);
    const gas = await bfc_staking.methods.candidate_bond_more(new BigNumber(100).multipliedBy(10 ** 18).toFixed()).estimateGas({ from: alithStash.public });
    expect(gas).greaterThan(0);
  });
});
