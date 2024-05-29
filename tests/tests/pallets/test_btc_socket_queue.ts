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

// submit_executed_request()
const VALID_EXECUTED_REQUEST_SUBMISSION_SIG = '0x9a3be5965cadfe127e65e485fc18c39bc9f7157dc9bbaf643f108e7c1b0740dc53c82990ef286702363d60e05ffe1afeea11ef0b1efbc79f9546c08032f108e81c';

// submit_unsigned_psbt()
const VALID_SYSTEM_VAULT_VOUT = 0;
const VALID_PSBT_TXID = '0x4356d8ce9259a22d2afc0ad7ba34bd349bf9d2bc7e28e676d22fc7cb3fa822a7';
const VALID_UNSIGNED_PSBT_SUBMISSION_SIG = '0x431836a4322e811431403dcc4df011b62b96987a53b1053b36e0b7a07dbee1c208942ce79927ebd813b34097732e2ce9125250f25c49ca5ec1ddb60e558b060e1c';
const VALID_UNSIGNED_PSBT_SUBMISSION_SIG_WITHOUT_REFUND = '0x441d7065a00bdc6249e466bfa46d1b35fd08e8c40386fa4c4235f1d8a0894ca61ab7bb5d601082e2cc7f24a46b9f116be3dc46f2c67d7a123d1375ca3177dc321b';
const VALID_UNSIGNED_PSBT_SUBMISSION_SIG_WITH_WRONG_OUTPUT_ORDER = '0x7107b996671096138e7b04cae6894c9d381b64d2259ba08e27aaaa3643830bbf52d0fb70c1a7918c917ad4b41564885af5fe0444c1657df6023ab2cb49353af01c';
const VALID_UNSIGNED_PSBT_SUBMISSION_SIG_WITH_WRONG_AMOUNT = '0x6b619c39b6a577e6730b50414e29beaf53899b50e8f3c5adfe9ea2d2d4033978582857574ca95ced1db18c13c199f6ec64880ad7e872f179584f8c0c90835b1c1c';
const VALID_UNSIGNED_PSBT = '0x70736274ff01007d020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff0200ca9a3b0000000022002001f910a6a2d3f8d3562ea46b75c057387c6db6d49cb3b863c884f50da1a8450e9584284800000000160014e0e55307ae2d25f1a8ff05fb3b25a0c67cbead16000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be0105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b00088000000';

const INVALID_UNSIGNED_PSBT_SUBMISSION_SIG = '0x64a7298c65a13542ea3b480f83822d8569398274e25aa920ba0f28cc7412750e23208877c20756ffaba0739356bc73cf09abdcd8be5045249be341785dff4fae1c';
const INVALID_UNSIGNED_PSBT_WITHOUT_REFUND = '0x70736274ff01005e020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff0100ca9a3b0000000022002001f910a6a2d3f8d3562ea46b75c057387c6db6d49cb3b863c884f50da1a8450e000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be0105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b000880000';
const INVALID_UNSIGNED_PSBT_WITH_WRONG_OUTPUT_ORDER = '0x70736274ff01007d020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff029584284800000000160014e0e55307ae2d25f1a8ff05fb3b25a0c67cbead1600ca9a3b0000000022002001f910a6a2d3f8d3562ea46b75c057387c6db6d49cb3b863c884f50da1a8450e000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be0105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b00088000000';
const INVALID_UNSIGNED_PSBT_WITH_WRONG_AMOUNT = '0x70736274ff01007d020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff0200ca9a3b0000000022002001f910a6a2d3f8d3562ea46b75c057387c6db6d49cb3b863c884f50da1a8450e9284284800000000160014e0e55307ae2d25f1a8ff05fb3b25a0c67cbead16000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be0105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b00088000000';

const VALID_SOCKET_MESSAGE = '0x00000000000000000000000000000000000000000000000000000000000000200000bfc000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000036c000000000000000000000000000000000000000000000000000000000000123100000000000000000000000000000000000000000000000000000000000000050000271200000000000000000000000000000000000000000000000000000000040207030100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000e0000000080000000300000bfc7e3a761afcec9f3e2fb7e853ffc45a62319143fa00000000000000000000000000000000000000000000000000000000000000000000000000000000000000003cd0a705a2dc65e5b1e1205896baa2be8a07c6e00000000000000000000000003cd0a705a2dc65e5b1e1205896baa2be8a07c6e0000000000000000000000000000000000000000000000000000000004828849500000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000000000000';
const INVALID_SOCKET_MESSAGE_WITH_INVALID_BYTES = '0x100000000000000000000000000000000000000000000000000000000000002000000bfc00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000036c000000000000000000000000000000000000000000000000000000000000123100000000000000000000000000000000000000000000000000000000000000050000008900000000000000000000000000000000000000000000000000000000040207030100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000e0000000080000000300000bfc7e3a761afcec9f3e2fb7e853ffc45a62319143fa00000000000000000000000000000000000000000000000000000000000000000000000000000000000000003cd0a705a2dc65e5b1e1205896baa2be8a07c6e00000000000000000000000003cd0a705a2dc65e5b1e1205896baa2be8a07c6e0000000000000000000000000000000000000000000000000000000004828849500000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000000000000';
const INVALID_SOCKET_MESSAGE_WITH_INVALID_BRIDGE_CHAINS = '0x000000000000000000000000000000000000000000000000000000000000002000000bfc00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000036c000000000000000000000000000000000000000000000000000000000000123100000000000000000000000000000000000000000000000000000000000000050000008900000000000000000000000000000000000000000000000000000000040207030100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000e0000000080000000300000bfc7e3a761afcec9f3e2fb7e853ffc45a62319143fa00000000000000000000000000000000000000000000000000000000000000000000000000000000000000003cd0a705a2dc65e5b1e1205896baa2be8a07c6e00000000000000000000000003cd0a705a2dc65e5b1e1205896baa2be8a07c6e0000000000000000000000000000000000000000000000000000000004828849500000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000000000000'

// submit_signed_psbt()
const VALID_SIGNED_PSBT_SUBMISSION_SIG = '0xd24abef978a9263c648c1e1087ebc05d5c8df02c5bb1478a5ada4fa463d53a6d3923f07f62012cb4878ae74514eb04425abbdc4efc7dc8d947e6013290863f871b';
const VALID_SIGNED_PSBT = '0x70736274ff01007d020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff0200ca9a3b0000000022002001f910a6a2d3f8d3562ea46b75c057387c6db6d49cb3b863c884f50da1a8450e9584284800000000160014e0e55307ae2d25f1a8ff05fb3b25a0c67cbead16000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be2202024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076648304502210082620df266e4bbe7df2a4e9e29e402efd3858efd2a69a217092caef9d26f663f022030d281a151db5233e20d1e8e5762378edc0292360d0dd759cdf4ee7d06e6b86d01220202531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe3374830450221009077fc4b17587c6e699fa2b4ae9326bbec8ba5c6f3683a50137bb8a0e02d9ab602201adfa4a490f770dbdb3b5363628af1f8fc3a48dc748b7214195e72f842357a96012202031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f483045022100b8b01d9721ce1901a89103faee2441ee253dcc2fa0b7ff794cedd2476aea9ebe0220594e1ed55dfcd90cff18ac8400542b28559fc2d44ed02603d272a6a044dd2bf9010105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b00088000000';

const INVALID_SIGNED_PSBT_SUBMISSION_SIG = '0x1ddb5590467cc3b90495696689229bb7f5f552e7a4e4e37bc05b01bd83e22d50196958434b1010ddcd14ca4b5c6c3a398c3a0e1f61da147295ee63dbc07de66f1b';
const INVALID_SIGNED_PSBT_WITH_WRONG_ORIGIN = '0x70736274ff01007d020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff0200ca9a3b0000000022002001f910a6a2d3f8d3562ea46b75c057387c6db6d49cb3b863c884f50da1a8450e9284284800000000160014e0e55307ae2d25f1a8ff05fb3b25a0c67cbead16000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be0108fc040047304402203a1dd209d8a6c163759c2ec37561cb48d7f466cfd650813348b63e85583277ce022060fdccbb6b0f863b6933c1623a946d1c3cbbf121f4982747d4fc18b3195ef2b50147304402201f839262864d169e564644dcbc00cc9226b7dd42621568367b654aa89455cbb2022020a3dfc14590342badbe9ee4810c2203c3b88bd5887dc8b8073ae84cc574495401695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae000000';
const INVALID_SIGNED_PSBT_WITH_UNFINALIZED = '0x70736274ff01007d020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff0200ca9a3b0000000022002001f910a6a2d3f8d3562ea46b75c057387c6db6d49cb3b863c884f50da1a8450e9584284800000000160014e0e55307ae2d25f1a8ff05fb3b25a0c67cbead16000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be2202024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076648304502210082620df266e4bbe7df2a4e9e29e402efd3858efd2a69a217092caef9d26f663f022030d281a151db5233e20d1e8e5762378edc0292360d0dd759cdf4ee7d06e6b86d010105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b00088000000';

// submit_system_vault_key()
const SYSTEM_VAULT_PUBKEY = '0x02c56c0cf38df8708f2e5725102f87a1d91f9356b0b7ebc4f6cafb396684e143b4';
const SYSTEM_VAULT_PUBKEY_SUBMISSION_SIG = '0x912088929bce91c813eb42a393ed2e5b2a36250e8ba483192dc9a2e4663401df42767fbbce7b1faccd9364c516923964fbfb6a0e914cf3a929e454b0cd49560e1c';

// submit_vault_key()
const VAULT_PUBKEY = '0x03e4f6fb93d47f69aed9338553e3ef1871a2b963f287268ca23cbf6fd3fc7dc6d9';
const VAULT_PUBKEY_SUBMISSION_SIG = '0xcc1c09e81934cdcaeebf370fe793cf82bdf06b5fc3ef82ccc5be736dd5c2517f53af885c0cd2a62fc56a0fa86509c27ff1a3d17b102431fb5b0bf8956b68736b1b';

const ZERO_ADDRESS = '0x0000000000000000000000000000000000000000';
const REFUND_ADDRESS = 'bcrt1qurj4xpaw95jlr28lqhankfdqce7tatgkeqrk9q';

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
    context.polkadotApi.tx.btcSocketQueue.setSocket(address, false)
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
    who: '0x0000000000000000000000000000000000000100',
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

  it('should fail to submit unsigned psbt - socket message duplication', async function () {
    await joinRegistrationPool(context);
    await submitVaultKey(context);

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

    const rawPendingRequest: any = await context.polkadotApi.query.btcSocketQueue.pendingRequests(VALID_PSBT_TXID);
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

    const signature = '0x9032907a744718c9314cbceed5655aa0e66b87610bf1529398b5f1d8efbd254035e7f9e8933a57800b35910483fee796f3b9828d121c9e34f147da5de426a8c21b';
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

    const signature = '0x3d6109639aff02ae12919512349bcec43ea98fe77ed85f49d0954aa1cb823563364fb684486e97c5c012bae9e0e423e6bb9a4955b21258e1ec480f1068cf14c41c';
    await context.polkadotApi.tx.btcSocketQueue.submitSignedPsbt(msg, signature).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcSocketQueue', 'submitSignedPsbt');
    expect(extrinsicResult).eq('InvalidPsbt');
  });

  it('should fail to submit signed psbt - cannot finalize psbt', async function () {
    const msg = {
      authorityId: alithRelayer.address,
      unsignedPsbt: VALID_UNSIGNED_PSBT,
      signedPsbt: INVALID_SIGNED_PSBT_WITH_UNFINALIZED,
    };

    const signature = '0x8cec6e3e22124edb601284c2fa3db549f5a21b937d24f13cfc4ad40c44fa01d819be9cfd9394fef0ab5a937eb71a644a03153fdae06cbc9e708b39aa23f6b1871c';
    await context.polkadotApi.tx.btcSocketQueue.submitSignedPsbt(msg, signature).send();
    await context.createBlock();

    const rawPendingRequest: any = await context.polkadotApi.query.btcSocketQueue.pendingRequests(VALID_PSBT_TXID);
    const pendingRequest = rawPendingRequest.toHuman();
    expect(pendingRequest).is.ok;
    expect(pendingRequest.signedPsbts[alithRelayer.address]).is.ok;
  });

  it('should successfully submit signed psbt', async function () {
    const msg = {
      authorityId: alithRelayer.address,
      unsignedPsbt: VALID_UNSIGNED_PSBT,
      signedPsbt: VALID_SIGNED_PSBT,
    };

    await context.polkadotApi.tx.btcSocketQueue.submitSignedPsbt(msg, VALID_SIGNED_PSBT_SUBMISSION_SIG).send();
    await context.createBlock();

    const rawPendingRequest: any = await context.polkadotApi.query.btcSocketQueue.pendingRequests(VALID_PSBT_TXID);
    const pendingRequest = rawPendingRequest.toHuman();
    expect(pendingRequest).is.null;

    const rawBondedOutboundTx: any = await context.polkadotApi.query.btcSocketQueue.bondedOutboundTx(VALID_PSBT_TXID);
    const bondedOutboundTx = rawBondedOutboundTx.toHuman();
    expect(bondedOutboundTx[0]).is.eq(VALID_SOCKET_MESSAGE);

    const rawFinalizedRequest: any = await context.polkadotApi.query.btcSocketQueue.finalizedRequests(VALID_PSBT_TXID);
    const finalizedRequest = rawFinalizedRequest.toHuman();
    expect(finalizedRequest).is.ok;
  });
});

describeDevNode('pallet_btc_socket_queue - accept request', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const alithRelayer = keyring.addFromUri(TEST_RELAYERS[0].private);

  before('init', async function () {
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

    const msg2 = {
      authorityId: alithRelayer.address,
      unsignedPsbt: VALID_UNSIGNED_PSBT,
      signedPsbt: VALID_SIGNED_PSBT,
    };

    await context.polkadotApi.tx.btcSocketQueue.submitSignedPsbt(msg2, VALID_SIGNED_PSBT_SUBMISSION_SIG).send();
    await context.createBlock();
  });

  it('should successfully submit executed request', async function () {
    const msg = {
      authorityId: alith.address,
      txid: VALID_PSBT_TXID,
    };

    await context.polkadotApi.tx.btcSocketQueue.submitExecutedRequest(msg, VALID_EXECUTED_REQUEST_SUBMISSION_SIG).send();
    await context.createBlock();

    const rawFinalizedRequest: any = await context.polkadotApi.query.btcSocketQueue.finalizedRequests(VALID_PSBT_TXID);
    const finalizedRequest = rawFinalizedRequest.toHuman();
    expect(finalizedRequest).is.null;

    const rawExecutedRequest: any = await context.polkadotApi.query.btcSocketQueue.executedRequests(VALID_PSBT_TXID);
    const executedRequest = rawExecutedRequest.toHuman();
    expect(executedRequest).is.ok;
  });
});
