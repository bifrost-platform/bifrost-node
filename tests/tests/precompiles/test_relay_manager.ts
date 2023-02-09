import { expect } from 'chai';

import { TEST_CONTROLLERS, TEST_RELAYERS } from '../../constants/keys';
import { describeDevNode } from '../set_dev_node';
import { callPrecompile } from '../transactions';

const SELECTORS = {
  // Role verifiers
  is_relayer: '976a75f1',
  is_selected_relayer: 'b6f0d1d0',
  is_relayers: '1768adb0',
  is_selected_relayers: 'f6bdf202',
  is_complete_selected_relayers: '17cc85f8',
  is_previous_selected_relayer: 'f8448c30',
  is_previous_selected_relayers: '39bae210',
  is_heartbeat_pulsed: '1ace2613',
  // Relayer storage getters
  selected_relayers: 'dcc7e6e0',
  previous_selected_relayers: '6d709a20',
  relayer_pool: '6e93ba34',
  majority: 'd2ea63fb',
  previous_majority: 'ea6ce574',
  relayer_state: '3f4e4fae',
  relayer_states: 'a77293f0',
  // Common storage getters
  latest_round: '6f31dd98',
  // Relayer dispatchable methods
  heartbeat: '3defb962',
};

const PRECOMPILE_ADDRESS = '0x0000000000000000000000000000000000002000';

describeDevNode('precompile_relay_manager - precompile view functions', (context) => {
  const alith: { public: string, private: string } = TEST_CONTROLLERS[0];
  const alithRelayer: { public: string, private: string } = TEST_RELAYERS[0];

  it('should successfully verify relayer roles', async function () {
    const is_relayer = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'is_relayer',
      [alithRelayer.public],
    );
    const decoded_is_relayer = context.web3.eth.abi.decodeParameters(
      ['bool'],
      is_relayer.result,
    )[0];
    expect(decoded_is_relayer).equal(true);

    const is_selected_relayer = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'is_selected_relayer',
      [alithRelayer.public, '0x1'],
    );
    const decoded_is_selected_relayer = context.web3.eth.abi.decodeParameters(
      ['bool'],
      is_selected_relayer.result,
    )[0];
    expect(decoded_is_selected_relayer).equal(true);

    const is_relayers = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'is_relayers',
      [
        context.web3.eth.abi.encodeParameter('address[]', [alithRelayer.public]),
      ],
    );
    const decoded_is_relayers = context.web3.eth.abi.decodeParameters(
      ['bool'],
      is_relayers.result,
    )[0];
    expect(decoded_is_relayers).equal(true);

    const is_selected_relayers = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'is_selected_relayers',
      [
        context.web3.eth.abi.encodeParameter('address[]', [alithRelayer.public]),
        '0x1',
      ],
    );
    const decoded_is_selected_relayers = context.web3.eth.abi.decodeParameters(
      ['bool'],
      is_selected_relayers.result,
    )[0];
    expect(decoded_is_selected_relayers).equal(true);

    const is_complete_selected_relayers = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'is_complete_selected_relayers',
      [
        context.web3.eth.abi.encodeParameter('address[]', [alithRelayer.public]),
        '0x1',
      ],
    );
    const decoded_is_complete_selected_relayers = context.web3.eth.abi.decodeParameters(
      ['bool'],
      is_complete_selected_relayers.result,
    )[0];
    expect(decoded_is_complete_selected_relayers).equal(true);

    const is_previous_selected_relayer = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'is_previous_selected_relayer',
      [
        '0x1',
        alithRelayer.public,
        '0x1',
      ],
    );
    const decoded_is_previous_selected_relayer = context.web3.eth.abi.decodeParameters(
      ['bool'],
      is_previous_selected_relayer.result,
    )[0];
    expect(decoded_is_previous_selected_relayer).equal(true);
  });

  it('should successfully verify relayer storage existance', async function () {
    const selected_relayers = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'selected_relayers',
      ['0x1'],
    );
    const decoded_selected_relayers = context.web3.eth.abi.decodeParameters(
      ['address[]'],
      selected_relayers.result,
    )[0];
    expect(decoded_selected_relayers.length).equal(1);
    expect(decoded_selected_relayers[0]).equal(alithRelayer.public);

    const previous_selected_relayers = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'previous_selected_relayers',
      ['0x1', '0x1'],
    );
    const decoded_previous_selected_relayers = context.web3.eth.abi.decodeParameters(
      ['address[]'],
      previous_selected_relayers.result,
    )[0];
    expect(decoded_previous_selected_relayers.length).equal(1);
    expect(decoded_previous_selected_relayers[0]).equal(alithRelayer.public);

    const relayer_pool = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'relayer_pool',
      [],
    );
    const decoded_relayer_pool = context.web3.eth.abi.decodeParameters(
      ['address[]', 'address[]'],
      relayer_pool.result,
    );
    expect(decoded_relayer_pool[0][0]).equal(alithRelayer.public);

    const majority = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'majority',
      ['0x1'],
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
      ['0x1', '0x1'],
    );
    const decoded_previous_majority = context.web3.eth.abi.decodeParameters(
      ['uint256'],
      previous_majority.result,
    )[0];
    expect(Number(decoded_previous_majority)).equal(1);

    const relayer_state = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'relayer_state',
      [alithRelayer.public],
    );
    const decoded_relayer_state = context.web3.eth.abi.decodeParameters(
      ['tuple(address,address,uint256)'],
      relayer_state.result,
    )[0];
    expect(decoded_relayer_state[0]).equal(alithRelayer.public);

    const relayer_states = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'relayer_states',
      [],
    );
    const decoded_relayer_states = context.web3.eth.abi.decodeParameters(
      ['address[]', 'address[]', 'uint256[]'],
      relayer_states.result,
    );
    expect(decoded_relayer_states[0][0]).equal(alithRelayer.public);
  });
});
