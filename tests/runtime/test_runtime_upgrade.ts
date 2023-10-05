import { expect } from 'chai';
import { describe } from 'mocha';
import Web3, { Contract, TransactionReceiptAPI } from 'web3';

import { ApiPromise, HttpProvider } from '@polkadot/api';

import { DEMO_ABI, DEMO_BYTE_CODE } from '../constants/demo_contract';
import { ERC20_ABI, ERC20_BYTE_CODE } from '../constants/ERC20';
import { TEST_CONTROLLERS } from '../constants/keys';
import { STAKING_ABI, STAKING_ADDRESS } from '../constants/staking_contract';
import { sleep } from '../tests/utils';

const web3 = new Web3(new Web3.providers.HttpProvider('http://localhost:9944'));

const alithPk = TEST_CONTROLLERS[0].private;
const alith = web3.eth.accounts.wallet.add(alithPk)[0].address;
const baltatharPk = TEST_CONTROLLERS[1].private;
const baltathar = web3.eth.accounts.wallet.add(baltatharPk)[1].address;

let erc20Address: string | undefined;

const deployDemo = async (deployTx: any): Promise<TransactionReceiptAPI | undefined> => {
  const signedTx = (await web3.eth.accounts.signTransaction({
    from: alith,
    data: deployTx.encodeABI(),
    gasPrice: web3.utils.toWei(1000, 'gwei'),
    gas: 3000000
  }, alithPk)).rawTransaction;

  // send transaction
  const txHash = await web3.requestManager.send({ method: 'eth_sendRawTransaction', params: [signedTx] });
  expect(txHash).is.ok;

  await sleep(3000);
  const receipt = await web3.requestManager.send({ method: 'eth_getTransactionReceipt', params: [txHash] });
  expect(receipt).is.ok;
  expect(receipt?.status).equal('0x1');
  expect(receipt?.contractAddress).is.ok;

  return receipt;
};

const sendTransaction = async (signedTx: string): Promise<string> => {
  const txHash = await web3.requestManager.send({ method: 'eth_sendRawTransaction', params: [signedTx] });
  expect(txHash).is.ok;

  // get transaction receipt
  await sleep(3000);
  const receipt = await web3.requestManager.send({ method: 'eth_getTransactionReceipt', params: [txHash] });
  expect(receipt).is.ok;
  expect(receipt!.status).equal('0x1');

  return txHash;
};

const createErc20Transfer = async (): Promise<string> => {
  const erc20: Contract<any> = new web3.eth.Contract(ERC20_ABI, erc20Address);
  const gas = await erc20.methods.transfer(baltathar, web3.utils.toWei(1, 'ether')).estimateGas({ from: alith });
  expect(gas).is.ok;

  return (await web3.eth.accounts.signTransaction({
    from: alith,
    to: erc20Address,
    gas,
    gasPrice: web3.utils.toWei(1000, 'gwei'),
    data: erc20.methods.transfer(baltathar, web3.utils.toWei(1, 'ether')).encodeABI()
  }, alithPk)).rawTransaction;
};

describe('runtime_upgrade - evm interactions', function () {
  this.timeout(20000);

  it('should successfully send transaction - legacy', async function () {
    const signedTx = (await web3.eth.accounts.signTransaction({
      from: alith,
      to: '0xf24FF3a9CF04c71Dbc94D0b566f7A27B94566cac',
      gasPrice: web3.utils.toWei(1000, 'gwei'),
      value: web3.utils.toWei(1, 'ether'),
      gas: 21000
    }, alithPk)).rawTransaction;

    // send transaction
    await sendTransaction(signedTx);
  });

  it('should successfully send transaction - eip1559', async function () {
    const signedTx = (await web3.eth.accounts.signTransaction({
      from: alith,
      to: '0xf24FF3a9CF04c71Dbc94D0b566f7A27B94566cac',
      maxFeePerGas: web3.utils.toWei(1200, 'gwei'),
      maxPriorityFeePerGas: web3.utils.toWei(1.5, 'gwei'),
      value: web3.utils.toWei(1, 'ether'),
      gas: 21000
    }, alithPk)).rawTransaction;

    // send transaction
    await sendTransaction(signedTx);
  });

  it('should successfully deploy a smart contract', async function () {
    const deployTx = ((new web3.eth.Contract(DEMO_ABI) as any).deploy({
      data: DEMO_BYTE_CODE
    }));
    const receipt = await deployDemo(deployTx);

    // estimate contract methods
    const contract: Contract<any> = new web3.eth.Contract(DEMO_ABI, receipt?.contractAddress);
    const gas = await contract.methods.store(1).estimateGas({ from: alith });
    expect(gas).is.ok;

    // send contract methods
    const signedTx_2 = (await web3.eth.accounts.signTransaction({
      from: alith,
      to: receipt?.contractAddress,
      gas,
      gasPrice: web3.utils.toWei(1000, 'gwei'),
      data: contract.methods.store(1).encodeABI()
    }, alithPk)).rawTransaction;

    await sendTransaction(signedTx_2);

    // call contract methods
    const response = await contract.methods.retrieve().call();
    expect(response).equal(BigInt(1));
  });

  it('should successfully interact with a precompiled contract', async function () {
    const staking: Contract<any> = new web3.eth.Contract(STAKING_ABI, STAKING_ADDRESS);
    const candidatePool = await staking.methods.candidate_pool().call();
    expect(candidatePool).is.ok;
    expect(candidatePool[0][0]).equal(alith);
    expect(candidatePool[1][0]).equal(BigInt(web3.utils.toWei(1000, 'ether')));

    const candidateState = await staking.methods.candidate_state(alith).call();
    expect(candidateState).is.ok;
    expect(candidateState.candidate).equal(alith);
    expect(candidateState.bond).equal(BigInt(web3.utils.toWei(1000, 'ether')));

    const gas = await staking.methods.nominate(alith, web3.utils.toWei(1000, 'ether'), 1000, 1000).estimateGas({ from: baltathar });
    expect(gas).is.ok;

    const signedTx = (await web3.eth.accounts.signTransaction({
      from: baltathar,
      to: STAKING_ADDRESS,
      gas,
      gasPrice: web3.utils.toWei(1000, 'gwei'),
      data: staking.methods.nominate(alith, web3.utils.toWei(1000, 'ether'), 1000, 1000).encodeABI()
    }, baltatharPk)).rawTransaction;

    await sendTransaction(signedTx);
  });
});

describe('runtime_upgrade - ethapi', function () {
  this.timeout(20000);

  it('should successfully request eth namespace methods', async function () {
    const gasPrice = await web3.requestManager.send({ method: 'eth_gasPrice', params: [] });
    expect(gasPrice).is.ok;
    expect(web3.utils.hexToNumberString(gasPrice)).equal(web3.utils.toWei(1000, 'gwei'));

    const balance = await web3.requestManager.send({ method: 'eth_getBalance', params: [alith, null] });
    expect(balance).is.ok;

    const deployTx = (new web3.eth.Contract(ERC20_ABI) as any).deploy({
      data: ERC20_BYTE_CODE
    });
    const receipt = await deployDemo(deployTx);

    erc20Address = receipt?.contractAddress;
    const signedTx_2 = await createErc20Transfer();

    await sendTransaction(signedTx_2);

    const logs = await web3.requestManager.send({
      method: 'eth_getLogs', params: [
        {
          address: receipt?.contractAddress,
          topics: ['0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef'],
          fromBlock: 1,
          toBlock: 100
        }
      ]
    });
    expect(logs).is.ok;
  });

  it('should successfully request txpool namespace methods', async function () {
    // verify txpool_status
    const status = await web3.requestManager.send({ method: 'txpool_status', params: [] });
    expect(status).is.ok;
    expect(status.pending).is.ok;
    expect(status.queued).is.ok;

    // verify txpool_inspect
    const inspect = await web3.requestManager.send({ method: 'txpool_inspect', params: [] });
    expect(inspect).is.ok;
    expect(inspect.pending).is.ok;
    expect(inspect.queued).is.ok;

    // verify txpool_content
    const content = await web3.requestManager.send({ method: 'txpool_content', params: [] });
    expect(content).is.ok;
    expect(content.pending).is.ok;
    expect(content.queued).is.ok;
  });

  it('should successfully request debug namespace methods', async function () {
    const signedTx = await createErc20Transfer();

    const txHash = await sendTransaction(signedTx);

    const debug = await web3.requestManager.send({
      method: 'debug_traceTransaction',
      params: [txHash, { tracer: 'callTracer' }]
    });
    expect(debug).is.ok;
    expect(debug.type).equal('CALL');
    const debug_2 = await web3.requestManager.send({ method: 'debug_traceTransaction', params: [txHash] });
    expect(debug_2).is.ok;
    expect(debug_2.gas).is.ok;
    expect(debug_2.returnValue).is.ok;
    expect(debug_2.structLogs).is.ok;
    expect(debug_2.structLogs[0].depth).is.ok;
    expect(debug_2.structLogs[0].gas).is.ok;
    expect(debug_2.structLogs[0].gasCost).is.ok;
    expect(debug_2.structLogs[0].memory).is.ok;
    expect(debug_2.structLogs[0].op).is.ok;
    expect(debug_2.structLogs[0].pc).exist;
    expect(debug_2.structLogs[0].stack).is.ok;
    expect(debug_2.structLogs[0].storage).is.ok;
  });
});

describe('runtime_upgrade - pallet interactions', function () {
  this.timeout(20000);

  let api: ApiPromise;

  before('should initialize api', async function () {
    api = await ApiPromise.create({ provider: new HttpProvider('http://localhost:9944'), noInitWarn: true });
  });

  it('should have correct validator information', async function () {
    const rawCandidateState: any = await api.query.bfcStaking.candidateInfo(alith);
    const candidateState = rawCandidateState.unwrap().toJSON();
    expect(candidateState).is.ok;

    const candidatePool = await api.query.bfcStaking.candidatePool();
    expect(candidatePool).is.not.empty;

    const rawSelectedCandidates: any = await api.query.bfcStaking.selectedCandidates();
    const selectedCandidates = rawSelectedCandidates.toJSON();
    expect(selectedCandidates).is.not.empty;
  });

  it('should have correct relayer information', async function () {
    const rawRelayerPool: any = await api.query.relayManager.relayerPool();
    const relayerPool = rawRelayerPool.toJSON();
    expect(relayerPool).is.not.empty;

    const rawBondedController: any = await api.query.relayManager.bondedController(alith);
    const bondedController = rawBondedController.toJSON();
    const rawRelayerState: any = await api.query.relayManager.relayerState(bondedController);
    const relayerState = rawRelayerState.unwrap().toJSON();
    expect(relayerState).is.ok;
    expect(relayerState.controller).equal(alith);
  });
});
