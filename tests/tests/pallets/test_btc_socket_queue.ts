import { expect } from 'chai';
import { TransactionReceiptAPI } from 'web3';

import { Keyring } from '@polkadot/api';

import {
  DEMO_SOCKET_ABI, INVALID_DEMO_SOCKET_BYTE_CODE_WITH_INVALID_MSG_HASH,
  INVALID_DEMO_SOCKET_BYTE_CODE_WITH_INVALID_STATUS, VALID_DEMO_SOCKET_BYTE_CODE
} from '../../constants/demo_contract';
import { TEST_CONTROLLERS, TEST_RELAYERS } from '../../constants/keys';
import { getExtrinsicResult } from '../extrinsics';
import { describeDevNode, INodeContext } from '../set_dev_node';

const SOCKET_MESSAGE_SEQ_ID = 4657;

// finalize_request()
// const VALID_FINALIZE_REQUEST_SIG = '';
// const INVALID_FINALIZE_REQUEST_SIG = '';

// submit_unsigned_psbt()
const VALID_SYSTEM_VAULT_VOUT = 0;
const VALID_UNSIGNED_PSBT_HASH = '0x02c1e853587f6695f9d16b12392a96dac9b5c476f850727401f2709f359d096f';
const VALID_UNSIGNED_PSBT_SUBMISSION_SIG = '0x1ab3bb3a2dd74fc03c1b790535b8b831af95aa6c84da68fed439de629235f3d816ce7b5ebb73beab4d606d91792d236fe439d953347197adc8cfdfde63e821c11c';
const VALID_UNSIGNED_PSBT_SUBMISSION_SIG_WITH_INVALID_BYTES = '0x7775f5d7af11b232d8398ce17c8d95fbf9fefc7764e62b3ca0bcfe2f07a50ab16343abf685e5996de75d1e91e1fd78d65b10a3b5aecb0e120c6c24ee9c3acc2e1b';
const VALID_UNSIGNED_PSBT_SUBMISSION_SIG_WITHOUT_REFUND = '0x7ebcbed3ba8d8e5f5ec8e1580f90af6c09026492f020e175fb7da42f122999817bb28e47f00042499c0a4107244aee53a9bced091e71ff14b199e752f8ef1fba1b';
const VALID_UNSIGNED_PSBT_SUBMISSION_SIG_WITH_WRONG_OUTPUT_ORDER = '0xf622827ad503a70c7cda6dc7bdba9e8c99b9c75cb9a46274645dbcb4c04d0afd71bcb5c243ba88e52ad02da62dd9ecfb477d627e13b8604156ade3e8e213b0c61b';
const VALID_UNSIGNED_PSBT_SUBMISSION_SIG_WITH_WRONG_AMOUNT = '0xa357f810e19f4d7838ac7eaffe057b8c63dedd0138202ec0321a2ffb79897bc842c1a33ba4ba5c2cbf77a37c0625f2abe516a1d598c4378083c20f7027d6547e1b';
const VALID_UNSIGNED_PSBT = '0x70736274ff010089020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff0200ca9a3b0000000022002001f910a6a2d3f8d3562ea46b75c057387c6db6d49cb3b863c884f50da1a8450e95842848000000002251202d4b1f0e2a5e77e026e763f239006c8531d2ca14765db6963dec4976c0710e28000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be0105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b00088000000';

// const INVALID_UNSIGNED_PSBT_HASH = '0x84d586783bf008ff4dccd5e7652222c64ca38e2b3cc808daddb55edfe285af41';
const INVALID_UNSIGNED_PSBT_SUBMISSION_SIG = '0x7775f5d7af11b232d8398ce17c8d95fbf9fefc7764e62b3ca0bcfe2f07a50ab16343abf685e5996de75d1e91e1fd78d65b10a3b5aecb0e120c6c24ee9c3acc2e1b';
const INVALID_UNSIGNED_PSBT_WITH_INVALID_BYTES = '0x00736274ff010089020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff0200ca9a3b0000000022002001f910a6a2d3f8d3562ea46b75c057387c6db6d49cb3b863c884f50da1a8450e95842848000000002251202d4b1f0e2a5e77e026e763f239006c8531d2ca14765db6963dec4976c0710e28000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be0105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b00088000000';
const INVALID_UNSIGNED_PSBT_WITHOUT_REFUND = '0x70736274ff01005e020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff0100ca9a3b0000000022002001f910a6a2d3f8d3562ea46b75c057387c6db6d49cb3b863c884f50da1a8450e000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be0105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b000880000';
const INVALID_UNSIGNED_PSBT_WITH_WRONG_OUTPUT_ORDER = '0x70736274ff010089020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff0295842848000000002251202d4b1f0e2a5e77e026e763f239006c8531d2ca14765db6963dec4976c0710e2800ca9a3b0000000022002001f910a6a2d3f8d3562ea46b75c057387c6db6d49cb3b863c884f50da1a8450e000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be0105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b00088000000';
const INVALID_UNSIGNED_PSBT_WITH_WRONG_AMOUNT = '0x70736274ff010089020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff0200ca9a3b0000000022002001f910a6a2d3f8d3562ea46b75c057387c6db6d49cb3b863c884f50da1a8450e0a000000000000002251202d4b1f0e2a5e77e026e763f239006c8531d2ca14765db6963dec4976c0710e28000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be0105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b00088000000';

const VALID_SOCKET_MESSAGE = '0x00000000000000000000000000000000000000000000000000000000000000200000bfc000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000036c000000000000000000000000000000000000000000000000000000000000123100000000000000000000000000000000000000000000000000000000000000050000271100000000000000000000000000000000000000000000000000000000040207030100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000e0000000080000000300000bfc7e3a761afcec9f3e2fb7e853ffc45a62319143fa00000000000000000000000000000000000000000000000000000000000000000000000000000000000000003cd0a705a2dc65e5b1e1205896baa2be8a07c6e00000000000000000000000003cd0a705a2dc65e5b1e1205896baa2be8a07c6e0000000000000000000000000000000000000000000000000000000004828849500000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000000000000';
const INVALID_SOCKET_MESSAGE_WITH_INVALID_BYTES = '0x100000000000000000000000000000000000000000000000000000000000002000000bfc00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000036c000000000000000000000000000000000000000000000000000000000000123100000000000000000000000000000000000000000000000000000000000000050000008900000000000000000000000000000000000000000000000000000000040207030100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000e0000000080000000300000bfc7e3a761afcec9f3e2fb7e853ffc45a62319143fa00000000000000000000000000000000000000000000000000000000000000000000000000000000000000003cd0a705a2dc65e5b1e1205896baa2be8a07c6e00000000000000000000000003cd0a705a2dc65e5b1e1205896baa2be8a07c6e0000000000000000000000000000000000000000000000000000000004828849500000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000000000000';
const INVALID_SOCKET_MESSAGE_WITH_INVALID_BRIDGE_CHAINS = '0x000000000000000000000000000000000000000000000000000000000000002000000bfc00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000036c000000000000000000000000000000000000000000000000000000000000123100000000000000000000000000000000000000000000000000000000000000050000008900000000000000000000000000000000000000000000000000000000040207030100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000e0000000080000000300000bfc7e3a761afcec9f3e2fb7e853ffc45a62319143fa00000000000000000000000000000000000000000000000000000000000000000000000000000000000000003cd0a705a2dc65e5b1e1205896baa2be8a07c6e00000000000000000000000003cd0a705a2dc65e5b1e1205896baa2be8a07c6e0000000000000000000000000000000000000000000000000000000004828849500000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000000000000'

// submit_signed_psbt()
const VALID_SIGNED_PSBT_SUBMISSION_SIG = '0xfef806390258ed49edf83eccaa7b84510a68279e0a8008123e58fa0091da31d745d83cb2ee051238feadca10a915c110e49bf26e6ef07a5f386a328bababee6f1c';
const VALID_SIGNED_PSBT = '0x70736274ff010089020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff0200ca9a3b0000000022002001f910a6a2d3f8d3562ea46b75c057387c6db6d49cb3b863c884f50da1a8450e95842848000000002251202d4b1f0e2a5e77e026e763f239006c8531d2ca14765db6963dec4976c0710e28000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be2202024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07664730440220209e607c752d4c5c8b03587408b872baa61743efa4d1c11cb53b381a585a00e2022059f9af0b5e35d33d43d26dccdf8e9d7fd93c5e01320b4a362151bfa886c5f59b010105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b00088000000';

const INVALID_SIGNED_PSBT_SUBMISSION_SIG = '0x1ddb5590467cc3b90495696689229bb7f5f552e7a4e4e37bc05b01bd83e22d50196958434b1010ddcd14ca4b5c6c3a398c3a0e1f61da147295ee63dbc07de66f1b';
const INVALID_SIGNED_PSBT_WITH_INVALID_BYTES = '0x00736274ff010089020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff0200ca9a3b0000000022002001f910a6a2d3f8d3562ea46b75c057387c6db6d49cb3b863c884f50da1a8450e95842848000000002251202d4b1f0e2a5e77e026e763f239006c8531d2ca14765db6963dec4976c0710e28000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be2202024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07664730440220209e607c752d4c5c8b03587408b872baa61743efa4d1c11cb53b381a585a00e2022059f9af0b5e35d33d43d26dccdf8e9d7fd93c5e01320b4a362151bfa886c5f59b010105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b00088000000';
const INVALID_SIGNED_PSBT_WITH_WRONG_ORIGIN = '0x70736274ff010089020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff0200ca9a3b0000000022002001f910a6a2d3f8d3562ea46b75c057387c6db6d49cb3b863c884f50da1a8450e0a000000000000002251202d4b1f0e2a5e77e026e763f239006c8531d2ca14765db6963dec4976c0710e28000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be2202024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d0766483045022100ff2960144477f33350347f6fbe9a666afa2eaf884a8025f215ff14748b4325d102205bf3292f6f0762f95fab96153ed083d2e1cbca1353278e325ba48ce8bbe4f8cd010105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b00088000000';

// submit_system_vault_key()
const SYSTEM_VAULT_PUBKEY = '0x02c56c0cf38df8708f2e5725102f87a1d91f9356b0b7ebc4f6cafb396684e143b4';
const SYSTEM_VAULT_PUBKEY_SUBMISSION_SIG = '0x912088929bce91c813eb42a393ed2e5b2a36250e8ba483192dc9a2e4663401df42767fbbce7b1faccd9364c516923964fbfb6a0e914cf3a929e454b0cd49560e1c';

// submit_vault_key()
const VAULT_PUBKEY = '0x03e4f6fb93d47f69aed9338553e3ef1871a2b963f287268ca23cbf6fd3fc7dc6d9';
const VAULT_PUBKEY_SUBMISSION_SIG = '0xcc1c09e81934cdcaeebf370fe793cf82bdf06b5fc3ef82ccc5be736dd5c2517f53af885c0cd2a62fc56a0fa86509c27ff1a3d17b102431fb5b0bf8956b68736b1b';

const ZERO_ADDRESS = '0x0000000000000000000000000000000000000000';
const REFUND_ADDRESS = 'tb1p94937r32tem7qfh8v0erjqrvs5ca9js5wewmd93aa3yhdsr3pc5qdtsy5h';

async function joinRegistrationPool(context: INodeContext) {
  const keyring = new Keyring({ type: 'ethereum' });
  const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);

  await context.polkadotApi.tx.btcRegistrationPool.requestVault(REFUND_ADDRESS).signAndSend(baltathar);
  await context.createBlock();
}

async function submitVaultKey(context: INodeContext) {
  const keyring = new Keyring({ type: 'ethereum' });
  const alithRelayer = keyring.addFromUri(TEST_RELAYERS[0].private);
  const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);

  const keySubmission = {
    authorityId: alithRelayer.address,
    who: baltathar.address,
    pubKey: VAULT_PUBKEY,
  };

  await context.polkadotApi.tx.btcRegistrationPool.submitVaultKey(keySubmission, VAULT_PUBKEY_SUBMISSION_SIG).send();
  await context.createBlock();
}

async function setSocket(context: INodeContext, address: string) {
  const keyring = new Keyring({ type: 'ethereum' });
  const sudo = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  await context.polkadotApi.tx.sudo.sudo(
    context.polkadotApi.tx.btcSocketQueue.setSocket(address)
  ).signAndSend(sudo);
  await context.createBlock();
}

async function deployDemoSocket(context: INodeContext, bytecode: string) {
  const deployTx = ((new context.web3.eth.Contract(DEMO_SOCKET_ABI) as any).deploy({
    data: bytecode
  }));
  const receipt = await deployDemo(context, deployTx);
  return receipt?.contractAddress;
}

const deployDemo = async (context: INodeContext, deployTx: any): Promise<TransactionReceiptAPI | undefined> => {
  const signedTx = (await context.web3.eth.accounts.signTransaction({
    from: TEST_CONTROLLERS[3].public,
    data: deployTx.encodeABI(),
    gasPrice: context.web3.utils.toWei(1000, 'gwei'),
    gas: 3000000
  }, TEST_CONTROLLERS[3].private)).rawTransaction;

  // send transaction
  const txHash = await context.web3.requestManager.send({ method: 'eth_sendRawTransaction', params: [signedTx] });
  expect(txHash).is.ok;

  await context.createBlock();
  await context.createBlock();

  const receipt = await context.web3.requestManager.send({ method: 'eth_getTransactionReceipt', params: [txHash] });
  expect(receipt).is.ok;
  expect(receipt?.status).equal('0x1');
  expect(receipt?.contractAddress).is.ok;

  return receipt;
};

async function requestSystemVault(context: INodeContext) {
  const keyring = new Keyring({ type: 'ethereum' });
  const sudo = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  await context.polkadotApi.tx.sudo.sudo(
    context.polkadotApi.tx.btcRegistrationPool.requestSystemVault()
  ).signAndSend(sudo);
  await context.createBlock();
}

async function submitSystemVaultKey(context: INodeContext) {
  const keyring = new Keyring({ type: 'ethereum' });
  const relayer = keyring.addFromUri(TEST_RELAYERS[0].private);
  const submit = {
    authorityId: relayer.address,
    pubKey: SYSTEM_VAULT_PUBKEY
  };

  await context.polkadotApi.tx.btcRegistrationPool.submitSystemVaultKey(submit, SYSTEM_VAULT_PUBKEY_SUBMISSION_SIG).send();
  await context.createBlock();
}

describeDevNode('pallet_btc_socket_queue - submit unsigned pbst', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);

  it('should fail to submit unsigned psbt - invalid authority', async function () {
    const msg = {
      authorityId: baltathar.address,
      systemVout: VALID_SYSTEM_VAULT_VOUT,
      socketMessages: [VALID_SOCKET_MESSAGE],
      psbt: VALID_UNSIGNED_PSBT
    };

    let errorMsg = '';
    await context.polkadotApi.tx.btcSocketQueue.submitUnsignedPsbt(msg, VALID_UNSIGNED_PSBT_SUBMISSION_SIG).send().catch(err => {
      if (err instanceof Error) {
        errorMsg = err.message;
      }
    });
    await context.createBlock();

    expect(errorMsg).eq('1010: Invalid Transaction: Invalid signing address');
  });

  it('should fail to submit unsigned psbt - invalid signature', async function () {
    const msg = {
      authorityId: alith.address,
      systemVout: VALID_SYSTEM_VAULT_VOUT,
      socketMessages: [VALID_SOCKET_MESSAGE],
      psbt: VALID_UNSIGNED_PSBT
    };

    let errorMsg = '';
    await context.polkadotApi.tx.btcSocketQueue.submitUnsignedPsbt(msg, INVALID_UNSIGNED_PSBT_SUBMISSION_SIG).send().catch(err => {
      if (err instanceof Error) {
        errorMsg = err.message;
      }
    });
    await context.createBlock();

    expect(errorMsg).eq('1010: Invalid Transaction: Transaction has a bad signature');
  });

  it('should fail to submit unsigned psbt - system vault is not requested', async function () {
    const msg = {
      authorityId: alith.address,
      systemVout: VALID_SYSTEM_VAULT_VOUT,
      socketMessages: [VALID_SOCKET_MESSAGE],
      psbt: VALID_UNSIGNED_PSBT
    };

    await context.polkadotApi.tx.btcSocketQueue.submitUnsignedPsbt(msg, VALID_UNSIGNED_PSBT_SUBMISSION_SIG).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcSocketQueue', 'submitUnsignedPsbt');
    expect(extrinsicResult).eq('SystemVaultDNE');
  });

  it('should fail to submit unsigned psbt - system vault is not generated', async function () {
    await requestSystemVault(context);

    const msg = {
      authorityId: alith.address,
      systemVout: VALID_SYSTEM_VAULT_VOUT,
      socketMessages: [VALID_SOCKET_MESSAGE],
      psbt: VALID_UNSIGNED_PSBT
    };

    await context.polkadotApi.tx.btcSocketQueue.submitUnsignedPsbt(msg, VALID_UNSIGNED_PSBT_SUBMISSION_SIG).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcSocketQueue', 'submitUnsignedPsbt');
    expect(extrinsicResult).eq('SystemVaultDNE');
  });

  it('should fail to submit unsigned psbt - empty socket message submitted', async function () {
    await submitSystemVaultKey(context);

    const msg = {
      authorityId: alith.address,
      systemVout: VALID_SYSTEM_VAULT_VOUT,
      socketMessages: [],
      psbt: VALID_UNSIGNED_PSBT
    };

    await context.polkadotApi.tx.btcSocketQueue.submitUnsignedPsbt(msg, VALID_UNSIGNED_PSBT_SUBMISSION_SIG).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcSocketQueue', 'submitUnsignedPsbt');
    expect(extrinsicResult).eq('InvalidSocketMessage');
  });

  it('should fail to submit unsigned psbt - invalid socket message bytes', async function () {
    const msg = {
      authorityId: alith.address,
      systemVout: VALID_SYSTEM_VAULT_VOUT,
      socketMessages: [INVALID_SOCKET_MESSAGE_WITH_INVALID_BYTES],
      psbt: VALID_UNSIGNED_PSBT
    };

    await context.polkadotApi.tx.btcSocketQueue.submitUnsignedPsbt(msg, VALID_UNSIGNED_PSBT_SUBMISSION_SIG).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcSocketQueue', 'submitUnsignedPsbt');
    expect(extrinsicResult).eq('InvalidSocketMessage');
  });

  it('should fail to submit unsigned psbt - socket contract is not set', async function () {
    const msg = {
      authorityId: alith.address,
      systemVout: VALID_SYSTEM_VAULT_VOUT,
      socketMessages: [VALID_SOCKET_MESSAGE],
      psbt: VALID_UNSIGNED_PSBT
    };

    await context.polkadotApi.tx.btcSocketQueue.submitUnsignedPsbt(msg, VALID_UNSIGNED_PSBT_SUBMISSION_SIG).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcSocketQueue', 'submitUnsignedPsbt');
    expect(extrinsicResult).eq('SocketDNE');
  });

  it('should fail to submit unsigned psbt - invalid request info response', async function () {
    await setSocket(context, ZERO_ADDRESS); // set socket to wrong address

    const msg = {
      authorityId: alith.address,
      systemVout: VALID_SYSTEM_VAULT_VOUT,
      socketMessages: [VALID_SOCKET_MESSAGE],
      psbt: VALID_UNSIGNED_PSBT
    };

    await context.polkadotApi.tx.btcSocketQueue.submitUnsignedPsbt(msg, VALID_UNSIGNED_PSBT_SUBMISSION_SIG).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcSocketQueue', 'submitUnsignedPsbt');
    expect(extrinsicResult).eq('InvalidRequestInfo');
  });

  it('should fail to submit unsigned psbt - socket message hash does not match', async function () {
    const socket = await deployDemoSocket(context, INVALID_DEMO_SOCKET_BYTE_CODE_WITH_INVALID_MSG_HASH);
    if (socket) {
      await setSocket(context, socket);
    }

    const msg = {
      authorityId: alith.address,
      systemVout: VALID_SYSTEM_VAULT_VOUT,
      socketMessages: [VALID_SOCKET_MESSAGE],
      psbt: VALID_UNSIGNED_PSBT
    };

    await context.polkadotApi.tx.btcSocketQueue.submitUnsignedPsbt(msg, VALID_UNSIGNED_PSBT_SUBMISSION_SIG).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcSocketQueue', 'submitUnsignedPsbt');
    expect(extrinsicResult).eq('InvalidSocketMessage');
  });

  it('should fail to submit unsigned psbt - message status is not accepted', async function () {
    const socket = await deployDemoSocket(context, INVALID_DEMO_SOCKET_BYTE_CODE_WITH_INVALID_STATUS);
    if (socket) {
      await setSocket(context, socket);
    }

    const msg = {
      authorityId: alith.address,
      systemVout: VALID_SYSTEM_VAULT_VOUT,
      socketMessages: [VALID_SOCKET_MESSAGE],
      psbt: VALID_UNSIGNED_PSBT
    };

    await context.polkadotApi.tx.btcSocketQueue.submitUnsignedPsbt(msg, VALID_UNSIGNED_PSBT_SUBMISSION_SIG).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcSocketQueue', 'submitUnsignedPsbt');
    expect(extrinsicResult).eq('InvalidSocketMessage');
  });

  it('should fail to submit unsigned psbt - invalid bridge relay chains', async function () {
    const socket = await deployDemoSocket(context, VALID_DEMO_SOCKET_BYTE_CODE);
    if (socket) {
      await setSocket(context, socket);
    }

    const msg = {
      authorityId: alith.address,
      systemVout: VALID_SYSTEM_VAULT_VOUT,
      socketMessages: [INVALID_SOCKET_MESSAGE_WITH_INVALID_BRIDGE_CHAINS],
      psbt: VALID_UNSIGNED_PSBT
    };

    await context.polkadotApi.tx.btcSocketQueue.submitUnsignedPsbt(msg, VALID_UNSIGNED_PSBT_SUBMISSION_SIG).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcSocketQueue', 'submitUnsignedPsbt');
    expect(extrinsicResult).eq('InvalidSocketMessage');
  });

  it('should fail to submit unsigned psbt - user is not registered', async function () {
    const msg = {
      authorityId: alith.address,
      systemVout: VALID_SYSTEM_VAULT_VOUT,
      socketMessages: [VALID_SOCKET_MESSAGE],
      psbt: VALID_UNSIGNED_PSBT
    };

    await context.polkadotApi.tx.btcSocketQueue.submitUnsignedPsbt(msg, VALID_UNSIGNED_PSBT_SUBMISSION_SIG).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcSocketQueue', 'submitUnsignedPsbt');
    expect(extrinsicResult).eq('UserDNE');
  });

  it('should fail to submit unsigned psbt - invalid psbt bytes', async function () {
    await joinRegistrationPool(context);
    await submitVaultKey(context);

    const msg = {
      authorityId: alith.address,
      systemVout: VALID_SYSTEM_VAULT_VOUT,
      socketMessages: [VALID_SOCKET_MESSAGE],
      psbt: INVALID_UNSIGNED_PSBT_WITH_INVALID_BYTES
    };

    await context.polkadotApi.tx.btcSocketQueue.submitUnsignedPsbt(msg, VALID_UNSIGNED_PSBT_SUBMISSION_SIG_WITH_INVALID_BYTES).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcSocketQueue', 'submitUnsignedPsbt');
    expect(extrinsicResult).eq('InvalidPsbt');
  });

  it('should fail to submit unsigned psbt - socket message duplication', async function () {
    const msg = {
      authorityId: alith.address,
      systemVout: VALID_SYSTEM_VAULT_VOUT,
      socketMessages: [VALID_SOCKET_MESSAGE, VALID_SOCKET_MESSAGE],
      psbt: VALID_UNSIGNED_PSBT
    };

    await context.polkadotApi.tx.btcSocketQueue.submitUnsignedPsbt(msg, VALID_UNSIGNED_PSBT_SUBMISSION_SIG).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcSocketQueue', 'submitUnsignedPsbt');
    expect(extrinsicResult).eq('InvalidSocketMessage');
  });

  it('should fail to submit unsigned psbt - missing refund tx output', async function () {
    const msg = {
      authorityId: alith.address,
      systemVout: VALID_SYSTEM_VAULT_VOUT,
      socketMessages: [VALID_SOCKET_MESSAGE],
      psbt: INVALID_UNSIGNED_PSBT_WITHOUT_REFUND
    };

    await context.polkadotApi.tx.btcSocketQueue.submitUnsignedPsbt(msg, VALID_UNSIGNED_PSBT_SUBMISSION_SIG_WITHOUT_REFUND).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcSocketQueue', 'submitUnsignedPsbt');
    expect(extrinsicResult).eq('InvalidPsbt');
  });

  it('should fail to submit unsigned psbt - first tx output is not system vault change refund', async function () {
    const msg = {
      authorityId: alith.address,
      systemVout: VALID_SYSTEM_VAULT_VOUT,
      socketMessages: [VALID_SOCKET_MESSAGE],
      psbt: INVALID_UNSIGNED_PSBT_WITH_WRONG_OUTPUT_ORDER
    };

    await context.polkadotApi.tx.btcSocketQueue.submitUnsignedPsbt(msg, VALID_UNSIGNED_PSBT_SUBMISSION_SIG_WITH_WRONG_OUTPUT_ORDER).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcSocketQueue', 'submitUnsignedPsbt');
    expect(extrinsicResult).eq('InvalidPsbt');
  });

  it('should fail to submit unsigned psbt - tx output with wrong amount', async function () {
    const msg = {
      authorityId: alith.address,
      systemVout: VALID_SYSTEM_VAULT_VOUT,
      socketMessages: [VALID_SOCKET_MESSAGE],
      psbt: INVALID_UNSIGNED_PSBT_WITH_WRONG_AMOUNT
    };

    await context.polkadotApi.tx.btcSocketQueue.submitUnsignedPsbt(msg, VALID_UNSIGNED_PSBT_SUBMISSION_SIG_WITH_WRONG_AMOUNT).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcSocketQueue', 'submitUnsignedPsbt');
    expect(extrinsicResult).eq('InvalidPsbt');
  });

  it('should successfully submit an unsigned psbt', async function () {
    const msg = {
      authorityId: alith.address,
      systemVout: VALID_SYSTEM_VAULT_VOUT,
      socketMessages: [VALID_SOCKET_MESSAGE],
      psbt: VALID_UNSIGNED_PSBT
    };

    await context.polkadotApi.tx.btcSocketQueue.submitUnsignedPsbt(msg, VALID_UNSIGNED_PSBT_SUBMISSION_SIG).send();
    await context.createBlock();

    const rawPendingRequest: any = await context.polkadotApi.query.btcSocketQueue.pendingRequests(VALID_UNSIGNED_PSBT_HASH);
    const pendingRequest = rawPendingRequest.toHuman();

    expect(pendingRequest).is.ok;
    expect(pendingRequest.unsignedPsbt).is.eq(VALID_UNSIGNED_PSBT);
    expect(pendingRequest.signedPsbts).is.empty;
    expect(pendingRequest.socketMessages).contains(VALID_SOCKET_MESSAGE);

    const rawSocketMessage: any = await context.polkadotApi.query.btcSocketQueue.socketMessages(SOCKET_MESSAGE_SEQ_ID);
    const socketMessage = rawSocketMessage.toHuman();
    expect(socketMessage).is.ok;
  });
});

describeDevNode('pallet_btc_socket_queue - submit signed pbst', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const alithRelayer = keyring.addFromUri(TEST_RELAYERS[0].private);

  before('it should successfully initialize', async function () {
    await requestSystemVault(context);
    await submitSystemVaultKey(context);

    await joinRegistrationPool(context);
    await submitVaultKey(context);

    const socket = await deployDemoSocket(context, VALID_DEMO_SOCKET_BYTE_CODE);
    if (socket) {
      await setSocket(context, socket);
    }

    const msg = {
      authorityId: alith.address,
      socketMessages: [VALID_SOCKET_MESSAGE],
      psbt: VALID_UNSIGNED_PSBT
    };

    await context.polkadotApi.tx.btcSocketQueue.submitUnsignedPsbt(msg, VALID_UNSIGNED_PSBT_SUBMISSION_SIG).send();
    await context.createBlock();
  });

  it('should fail to submit signed psbt - invalid authority', async function () {
    const msg = {
      authorityId: alith.address,
      unsignedPsbt: VALID_UNSIGNED_PSBT,
      signedPsbt: VALID_SIGNED_PSBT,
    };

    let errorMsg = '';
    await context.polkadotApi.tx.btcSocketQueue.submitSignedPsbt(msg, VALID_SIGNED_PSBT_SUBMISSION_SIG).send().catch(err => {
      if (err instanceof Error) {
        errorMsg = err.message;
      }
    });
    await context.createBlock();

    expect(errorMsg).eq('1010: Invalid Transaction: Invalid signing address');
  });

  it('should fail to submit signed psbt - invalid signature', async function () {
    const msg = {
      authorityId: alithRelayer.address,
      unsignedPsbt: VALID_UNSIGNED_PSBT,
      signedPsbt: VALID_SIGNED_PSBT,
    };

    let errorMsg = '';
    await context.polkadotApi.tx.btcSocketQueue.submitSignedPsbt(msg, INVALID_SIGNED_PSBT_SUBMISSION_SIG).send().catch(err => {
      if (err instanceof Error) {
        errorMsg = err.message;
      }
    });
    await context.createBlock();

    expect(errorMsg).eq('1010: Invalid Transaction: Transaction has a bad signature');
  });

  it('should fail to submit signed psbt - unknown unsigned psbt', async function () {
    const msg = {
      authorityId: alithRelayer.address,
      unsignedPsbt: INVALID_UNSIGNED_PSBT_WITHOUT_REFUND,
      signedPsbt: VALID_SIGNED_PSBT,
    };

    await context.polkadotApi.tx.btcSocketQueue.submitSignedPsbt(msg, VALID_SIGNED_PSBT_SUBMISSION_SIG).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcSocketQueue', 'submitSignedPsbt');
    expect(extrinsicResult).eq('RequestDNE');
  });

  it('should fail to submit signed psbt - did not sign psbt', async function () {
    const msg = {
      authorityId: alithRelayer.address,
      unsignedPsbt: VALID_UNSIGNED_PSBT,
      signedPsbt: VALID_UNSIGNED_PSBT,
    };

    const signature = '0x9729a4d43357fa4a2554779cad1a65cf2149a7c7a00f1cef97d74d979758836f7cfd9840455286640118fbe02e2352c55dd35aa4f01e887ac2b5bee829ff39a21c';
    await context.polkadotApi.tx.btcSocketQueue.submitSignedPsbt(msg, signature).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcSocketQueue', 'submitSignedPsbt');
    expect(extrinsicResult).eq('InvalidPsbt');
  });

  it('should fail to submit signed psbt - invalid signed psbt format', async function () {
    const msg = {
      authorityId: alithRelayer.address,
      unsignedPsbt: VALID_UNSIGNED_PSBT,
      signedPsbt: INVALID_SIGNED_PSBT_WITH_INVALID_BYTES,
    };

    const signature = '0x1ddb5590467cc3b90495696689229bb7f5f552e7a4e4e37bc05b01bd83e22d50196958434b1010ddcd14ca4b5c6c3a398c3a0e1f61da147295ee63dbc07de66f1b';
    await context.polkadotApi.tx.btcSocketQueue.submitSignedPsbt(msg, signature).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcSocketQueue', 'submitSignedPsbt');
    expect(extrinsicResult).eq('InvalidPsbt');
  });

  it('should fail to submit signed psbt - signed wrong psbt', async function () {
    const msg = {
      authorityId: alithRelayer.address,
      unsignedPsbt: VALID_UNSIGNED_PSBT,
      signedPsbt: INVALID_SIGNED_PSBT_WITH_WRONG_ORIGIN,
    };

    const signature = '0xf3c562df4e196dce7119d21364c89a46a1171d936d16d3d89643ed4903e0449b29618305d5797a945c4cb6ba19ea2d382c47a0a7bac93a4bf0cea16630eaeaca1b';
    await context.polkadotApi.tx.btcSocketQueue.submitSignedPsbt(msg, signature).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcSocketQueue', 'submitSignedPsbt');
    expect(extrinsicResult).eq('InvalidPsbt');
  });

  it('should fail to submit signed psbt - cannot finalize psbt', async function () {
    const msg = {
      authorityId: alithRelayer.address,
      unsignedPsbt: VALID_UNSIGNED_PSBT,
      signedPsbt: VALID_SIGNED_PSBT,
    };

    await context.polkadotApi.tx.btcSocketQueue.submitSignedPsbt(msg, VALID_SIGNED_PSBT_SUBMISSION_SIG).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcSocketQueue', 'submitSignedPsbt');
    expect(extrinsicResult).eq('CannotFinalizePsbt');
  });
});
