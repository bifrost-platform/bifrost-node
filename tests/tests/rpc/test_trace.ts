import { expect } from 'chai';

import { customWeb3Request } from '../providers';
import { describeDevNode } from '../set_dev_node';
import { getEmptyRawTx } from './test_txpool';

describeDevNode('Trace - Simple Ethereum transfer transaction', (context) => {
  let txHash: string;
  before('Setup: Create transaction with block', async () => {
    const rawTx = await getEmptyRawTx(context);
    txHash = (await customWeb3Request(context.web3, 'eth_sendRawTransaction', [rawTx]));
    await context.createBlock();
  });

  it('should be able to trace ethereum transaction', async function () {
    const traceResult = (await customWeb3Request(context.web3, 'debug_traceTransaction', [txHash]));
    expect(traceResult).to.not.be.undefined;
    // expect(traceResult).to.include({
    //   gas: '0x0',
    //   returnValue: ''
    // });
  });
});
