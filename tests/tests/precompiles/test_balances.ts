import { expect } from 'chai';

import { TEST_CONTROLLERS } from '../../constants/keys';
import { describeDevNode } from '../set_dev_node';
import { callPrecompile } from '../transactions';

const SELECTORS = {
  // Common storage getters
  total_issuance: '7f5097b7',
};

const PRECOMPILE_ADDRESS = '0x0000000000000000000000000000000000001000';

describeDevNode('precompile_balances - precompile view functions', (context) => {
  const alith: { public: string, private: string } = TEST_CONTROLLERS[0];

  it('should successfully verify balance storage existance', async function () {
    const total_issuance = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'total_issuance',
      [],
    );
    const decoded_total_issuance = context.web3.eth.abi.decodeParameters(
      ['uint256'],
      total_issuance.result,
    )[0];
    expect(Number(decoded_total_issuance)).greaterThan(0);
  });
});
