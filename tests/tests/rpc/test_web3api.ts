import { expect } from 'chai';

import { customWeb3Request } from '../providers';
import { describeDevNode } from '../set_dev_node';

describeDevNode('Web3Api Information', (context) => {
  it('should include client version', async function () {
    const version = await context.web3.eth.getNodeInfo();
    const regex = new RegExp('^bifrost-node\\/v\\d+\\.\\d+\\.\\d+-[0-9a-f]+\\/[^\\/]+\\/rustc\\d+\\.\\d+\\.\\d+$');
    expect(version).to.be.match(regex);
  });

  it('should provide sha3 hashing', async function () {
    const data = context.web3.utils.stringToHex('hello');
    const nodeHash = await customWeb3Request(context.web3, 'web3_sha3', [data]);
    const localHash = context.web3.utils.sha3('hello');
    expect(nodeHash).to.be.equal(localHash);
  });

  it('should report peer count in hex', async function () {
    const result = await customWeb3Request(context.web3, 'net_peerCount', []);
    expect(result).to.be.equal('0x0');
    expect(typeof result).to.be.equal('string');
  });
});
