import { expect } from 'chai';

import { TEST_CONTROLLERS } from '../../constants/keys';
import { describeDevNode } from '../set_dev_node';
import { callPrecompile } from '../transactions';

const SELECTORS = {
  maximum_offence_count: '42caa150',
  validator_offence: 'c63c3f8a',
  validator_offences: '2962bb0b',
};

const PRECOMPILE_ADDRESS = '0x0000000000000000000000000000000000000500';

describeDevNode('precompile_bfc_offences - precompile view functions', (context) => {
  const alith: { public: string, private: string } = TEST_CONTROLLERS[0];

  it('should successfully verify offence storage existance', async function () {
    const maximum_offence_count = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'maximum_offence_count',
      ['0x0'],
    );
    const decoded_maximum_offence_count = context.web3.eth.abi.decodeParameters(
      ['uint256[]'],
      maximum_offence_count.result,
    )[0];
    expect(Number(decoded_maximum_offence_count[0])).equal(5);
    expect(Number(decoded_maximum_offence_count[1])).equal(3);

    const validator_offence = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'validator_offence',
      [alith.public],
    );
    const decoded_validator_offence = context.web3.eth.abi.decodeParameters(
      ['tuple(address,uint256,uint256,uint256)'],
      validator_offence.result,
    )[0];
    expect(decoded_validator_offence[0]).equal(alith.public);

    const validator_offences = await callPrecompile(
      context,
      alith.public,
      PRECOMPILE_ADDRESS,
      SELECTORS,
      'validator_offences',
      [
        context.web3.eth.abi.encodeParameter('address[]', [alith.public])
      ],
    );
    const decoded_validator_offences = context.web3.eth.abi.decodeParameters(
      ['tuple(address[],uint256[],uint256[],uint256[])'],
      validator_offences.result,
    )[0];
    expect(decoded_validator_offences[0][0]).equal(alith.public);
  });
});
