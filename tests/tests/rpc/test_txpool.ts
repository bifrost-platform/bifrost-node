import { expect } from 'chai';

import { customWeb3Request } from '../providers';
import { describeDevNode, INodeContext } from '../set_dev_node';
import { createTransaction } from '../transactions';

export async function getEmptyRawTx(context: INodeContext) {
  return createTransaction(context, {
    privateKey: '0x5fb92d6e98884f76de468fa3f6278f8807c48bebc13595d45af5bdc4da702133',
    to: '0x3Cd0A705a2DC65e5b1E1205896BaA2be8A07c6e0',
    gas: 21000
  });
}

describeDevNode('TxPool - Genesis', (context) => {
  it('should be empty', async function () {
    const inspect = await customWeb3Request(context.web3, 'txpool_inspect', []);
    expect(inspect.pending).to.be.empty;
    const content = await customWeb3Request(context.web3, 'txpool_content', []);
    expect(content.pending).to.be.empty;
  });
});

describeDevNode('Txpool - Pending Ethereum transaction', (context) => {
  let txHash: string;
  before('Setup: Create transaction', async () => {
    const rawTx = await getEmptyRawTx(context);
    txHash = (await customWeb3Request(context.web3, 'eth_sendRawTransaction', [rawTx]));
  });

  it('should appear in txpool inspection', async function () {
    const inspect = await customWeb3Request(context.web3, 'txpool_inspect', []);
    const data = inspect.pending['0xf24ff3a9cf04c71dbc94d0b566f7a27b94566cac'][context.web3.utils.toHex(0)];
    expect(data).to.not.be.undefined;
  });

  it('should be marked as pending', async function () {
    const pendingTransaction = (
      await customWeb3Request(context.web3, 'eth_getTransactionByHash', [txHash])
    );
    expect(pendingTransaction).to.include({
      blockNumber: null,
      hash: txHash
    });
  });

  it('should appear in txpool content', async function () {
    const content = await customWeb3Request(context.web3, 'txpool_content', []);
    const data = content.pending['0xf24ff3a9cf04c71dbc94d0b566f7a27b94566cac'][context.web3.utils.toHex(0)];
    expect(data).to.include({
      blockHash: null,
      blockNumber: null,
      from: '0xf24ff3a9cf04c71dbc94d0b566f7a27b94566cac',
      hash: txHash,
      nonce: context.web3.utils.toHex(0),
      to: '0x3cd0a705a2dc65e5b1e1205896baa2be8a07c6e0',
      value: '0x0',
    });
  });
});

describeDevNode('Txpool - New block', (context) => {
  before('Setup: Create transaction and empty block', async () => {
    const rawTx = await getEmptyRawTx(context);
    await context.createBlock({ transactions: [rawTx] });
    await context.createBlock();
  });

  it('should reset the txpool', async function () {
    const inspect = await customWeb3Request(context.web3, 'txpool_inspect', []);
    expect(inspect.pending).to.be.empty;
    let content = await customWeb3Request(context.web3, 'txpool_content', []);
    expect(content.pending).to.be.empty;
  });
});
