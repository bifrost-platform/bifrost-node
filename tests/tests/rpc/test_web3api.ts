import { expect } from 'chai';
import { customWeb3Request } from '../providers';
import { describeDevNode } from '../set_dev_node';

describeDevNode('Web3Api Information', (context) => {
  it('should include client version', async function () {
    const version = await context.web3.eth.getNodeInfo();
    const specName: string = context.polkadotApi.runtimeVersion.specName.toString();
    const specVersion: string = context.polkadotApi.runtimeVersion.specVersion.toString();
    const implVersion: string = context.polkadotApi.runtimeVersion.implVersion.toString();
    const regex = new RegExp(specName + '/v' + specVersion + '.' + implVersion + '/fc-rpc-2.0.0');
    expect(version).to.be.match(regex);
  });

  it('should provide sha3 hashing', async function () {
    const data = context.web3.utils.stringToHex('hello');
    const nodeHash = await customWeb3Request(context.web3, 'web3_sha3', [data]);
    const localHash = context.web3.utils.sha3('hello');
    expect(nodeHash.result).to.be.equal(localHash);
  });

  it('should report peer count in hex', async function () {
    const result = await customWeb3Request(context.web3, 'net_peerCount', []);
    expect(result.result).to.be.equal('0x0');
    expect(typeof result.result).to.be.equal('string');
  });
});
