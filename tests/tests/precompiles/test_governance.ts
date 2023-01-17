import { expect } from 'chai';

import { TEST_CONTROLLERS } from '../../constants/keys';
import { describeDevNode } from '../set_dev_node';
import { callPrecompile } from '../transactions';

const SELECTORS = {
  // Common storage getters
  public_prop_count: '56fdf547',
  public_props: 'b089f202',
  deposit_of: 'a30305e9',
  voting_of: '09daa4d8',
  account_votes: '198a1bd9',
  lowest_unbaked: '0388f282',
  ongoing_referendum_info: '8b93d11a',
  finished_referendum_info: 'b1fd383f',
  // Dispatchable methods
  propose: '7824e7d1',
  second: 'c7a76601',
  vote: 'f56cb3b3',
  remove_vote: '2042f50b',
  delegate: '0185921e',
  undelegate: 'cb37b8ea',
  unlock: '2f6c493c',
  note_preimage: '200881f5',
  note_imminent_preimage: 'cf205f96',
};

const PRECOMPILE_ADDRESS = '0x0000000000000000000000000000000000000800';

describeDevNode('precompile_governance - precompile actions', (context) => {
  const alith: { public: string, private: string } = TEST_CONTROLLERS[0];

  it('should successfully verify governance storage existance', async function () {
    const public_prop_count = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'public_prop_count',
      [],
    );
    const decoded_public_prop_count = context.web3.eth.abi.decodeParameters(
      ['uint256'],
      public_prop_count.result,
    )[0];
    expect(Number(decoded_public_prop_count)).equal(0);
  });
});
