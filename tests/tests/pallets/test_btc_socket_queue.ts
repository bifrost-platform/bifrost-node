import { expect } from 'chai';
import { TransactionReceiptAPI } from 'web3';

import { Keyring } from '@polkadot/api';

import {
  DEMO_SOCKET_ABI, INVALID_DEMO_SOCKET_BYTE_CODE,
  INVALID_STATUS_DEMO_SOCKET_BYTE_CODE, VALID_DEMO_SOCKET_BYTE_CODE
} from '../../constants/demo_contract';
import { TEST_CONTROLLERS, TEST_RELAYERS } from '../../constants/keys';
import { getExtrinsicResult } from '../extrinsics';
import { describeDevNode, INodeContext } from '../set_dev_node';

const SOCKET_MESSAGE_SEQ_ID = 4657;

// finalize_request()
const VALID_FINALIZE_REQUEST_SIG = '0x2812412ba6d41de42adbdae1c9c193c59e3a8ca20085d38316a442b588f23bfd750f876762efd22ad82245efee5745c73f33501625b185ada4bf4f48b4b5743b1b';
const INVALID_FINALIZE_REQUEST_SIG = '0xf45facd8aabcd0db50daca4a001562b09d2c7adaf8d7d11f7286d94eef43697241ad047021a6617c13344372b7e3edd69d1b6a07b573bc5c9f69d41b103129cc1c';

// submit_unsigned_psbt()
const VALID_UNSIGNED_PSBT_HASH = '0xde0e3eaca967df97ad1c347b2d87d855bb813b0619d8a3beaf31f70212380227';
const INVALID_UNSIGNED_PSBT_HASH = '0x0e0e3eaca967df97ad1c347b2d87d855bb813b0619d8a3beaf31f70212380227';
const VALID_UNSIGNED_PSBT = '0x70736274ff010089020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff0200e1f5050000000022002001f910a6a2d3f8d3562ea46b75c057387c6db6d49cb3b863c884f50da1a8450e95842848000000002251202d4b1f0e2a5e77e026e763f239006c8531d2ca14765db6963dec4976c0710e28000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be0105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b00088000000';
const INVALID_UNSIGNED_PSBT = '0x00736274ff010089020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff02958428480000000022002001f910a6a2d3f8d3562ea46b75c057387c6db6d49cb3b863c884f50da1a8450e95842848000000002251202d4b1f0e2a5e77e026e763f239006c8531d2ca14765db6963dec4976c0710e28000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be0105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b00088000000';
const INVALID_UNSIGNED_PSBT_WITHOUT_REFUND = '0x70736274ff01005e020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff0100e1f505000000002251202d4b1f0e2a5e77e026e763f239006c8531d2ca14765db6963dec4976c0710e28000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be0105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b000880000';
const INVALID_UNSIGNED_PSBT_WITH_OUTPUT_WRONG_ORDER = '0x70736274ff010089020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff0200e1f505000000002251202d4b1f0e2a5e77e026e763f239006c8531d2ca14765db6963dec4976c0710e28008d380c0100000022002001f910a6a2d3f8d3562ea46b75c057387c6db6d49cb3b863c884f50da1a8450e000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be0105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b00088000000';
const INVALID_UNSIGNED_PSBT_WITH_WRONG_AMOUNT = '0x70736274ff010089020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff0200e1f5050000000022002001f910a6a2d3f8d3562ea46b75c057387c6db6d49cb3b863c884f50da1a8450e94842848000000002251202d4b1f0e2a5e77e026e763f239006c8531d2ca14765db6963dec4976c0710e28000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be0105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b00088000000';
const VALID_SOCKET_MESSAGE = '0x000000000000000000000000000000000000000000000000000000000000002000000bfc00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000036c000000000000000000000000000000000000000000000000000000000000123100000000000000000000000000000000000000000000000000000000000000050000008900000000000000000000000000000000000000000000000000000000040207030100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000e0000000080000000300000bfc7e3a761afcec9f3e2fb7e853ffc45a62319143fa00000000000000000000000000000000000000000000000000000000000000000000000000000000000000003cd0a705a2dc65e5b1e1205896baa2be8a07c6e00000000000000000000000003cd0a705a2dc65e5b1e1205896baa2be8a07c6e0000000000000000000000000000000000000000000000000000000004828849500000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000000000000';
const INVALID_SOCKET_MESSAGE = '0x100000000000000000000000000000000000000000000000000000000000002000000bfc00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000036c000000000000000000000000000000000000000000000000000000000000123100000000000000000000000000000000000000000000000000000000000000050000008900000000000000000000000000000000000000000000000000000000040207030100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000e0000000080000000300000bfc7e3a761afcec9f3e2fb7e853ffc45a62319143fa00000000000000000000000000000000000000000000000000000000000000000000000000000000000000003cd0a705a2dc65e5b1e1205896baa2be8a07c6e00000000000000000000000003cd0a705a2dc65e5b1e1205896baa2be8a07c6e0000000000000000000000000000000000000000000000000000000004828849500000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000000000000';
const VALID_UNSIGNED_PSBT_SUBMISSION_SIG = '0x2812412ba6d41de42adbdae1c9c193c59e3a8ca20085d38316a442b588f23bfd750f876762efd22ad82245efee5745c73f33501625b185ada4bf4f48b4b5743b1b';
const INVALID_UNSIGNED_PSBT_SUBMISSION_SIG = '0x0d40659d2fe9c6c4ebc4083afd6a46e9450387a06d3e64187dea72c5fb5595b921cf41c642587342077d592fa465d1ae0c0262b2250b43c031349a6773e1b7f91c';

// submit_signed_psbt()
const VALID_SIGNED_PSBT = '0x70736274ff010089020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff0200e1f5050000000022002001f910a6a2d3f8d3562ea46b75c057387c6db6d49cb3b863c884f50da1a8450e95842848000000002251202d4b1f0e2a5e77e026e763f239006c8531d2ca14765db6963dec4976c0710e28000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be2202024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d0766483045022100c2cfd505b2f0fa47f5ebd61475f3255caf9feb11d58e6007181c7bd3a8a2a8930220059e7e76c6940f55b872485036578b6b7d0741a84e9f8a0039d0d8b453043e5f010105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b00088000000';
const INVALID_SIGNED_PSBT = '0x00736274ff010089020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff0200e1f5050000000022002001f910a6a2d3f8d3562ea46b75c057387c6db6d49cb3b863c884f50da1a8450e95842848000000002251202d4b1f0e2a5e77e026e763f239006c8531d2ca14765db6963dec4976c0710e28000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be2202024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d0766483045022100c2cfd505b2f0fa47f5ebd61475f3255caf9feb11d58e6007181c7bd3a8a2a8930220059e7e76c6940f55b872485036578b6b7d0741a84e9f8a0039d0d8b453043e5f010105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b00088000000';
const INVALID_SIGNED_PSBT_WITH_WRONG_ORIGIN = '0x70736274ff010089020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff0200ca9a3b0000000022002001f910a6a2d3f8d3562ea46b75c057387c6db6d49cb3b863c884f50da1a8450e95842848000000002251202d4b1f0e2a5e77e026e763f239006c8531d2ca14765db6963dec4976c0710e28000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be2202024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07664730440220209e607c752d4c5c8b03587408b872baa61743efa4d1c11cb53b381a585a00e2022059f9af0b5e35d33d43d26dccdf8e9d7fd93c5e01320b4a362151bfa886c5f59b010105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b00088000000';
const VALID_SIGNED_PSBT_SUBMISSION_SIG = '0x5080380ead598b545aad53ccffcdf02e3df7d7aa7e678f12bc7caefc5b3c56272b0c4e67c559c3a1553d3095f8b22abddc5627b100887ebd48b6b303da7cd4561b';
const INVALID_SIGNED_PSBT_SUBMISSION_SIG = '0x0080380ead598b545aad53ccffcdf02e3df7d7aa7e678f12bc7caefc5b3c56272b0c4e67c559c3a1553d3095f8b22abddc5627b100887ebd48b6b303da7cd4561b';

// submit_system_vault_key()
const SYSTEM_VAULT_PUBKEY = '0x02c56c0cf38df8708f2e5725102f87a1d91f9356b0b7ebc4f6cafb396684e143b4';
const SYSTEM_VAULT_PUBKEY_SUBMISSION_SIG = '0x912088929bce91c813eb42a393ed2e5b2a36250e8ba483192dc9a2e4663401df42767fbbce7b1faccd9364c516923964fbfb6a0e914cf3a929e454b0cd49560e1c';

// submit_vault_key()
const VAULT_PUBKEY = '0x02c56c0cf38df8708f2e5725102f87a1d91f9356b0b7ebc4f6cafb396684e143b4';
const VAULT_PUBKEY_SUBMISSION_SIG = '0x912088929bce91c813eb42a393ed2e5b2a36250e8ba483192dc9a2e4663401df42767fbbce7b1faccd9364c516923964fbfb6a0e914cf3a929e454b0cd49560e1c';

const ZERO_ADDRESS = '0x0000000000000000000000000000000000000000';

async function joinRegistrationPool(context: INodeContext) {
  const keyring = new Keyring({ type: 'ethereum' });
  const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const refund = 'tb1p94937r32tem7qfh8v0erjqrvs5ca9js5wewmd93aa3yhdsr3pc5qdtsy5h';

  await context.polkadotApi.tx.btcRegistrationPool.requestVault(refund).signAndSend(baltathar);
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
      socketMessages: [INVALID_SOCKET_MESSAGE],
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
      socketMessages: [VALID_SOCKET_MESSAGE],
      psbt: VALID_UNSIGNED_PSBT
    };

    await context.polkadotApi.tx.btcSocketQueue.submitUnsignedPsbt(msg, VALID_UNSIGNED_PSBT_SUBMISSION_SIG).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcSocketQueue', 'submitUnsignedPsbt');
    expect(extrinsicResult).eq('InvalidRequestInfo');
  });

  it('should fail to submit unsigned psbt - socket message hash does not match', async function () {
    const socket = await deployDemoSocket(context, INVALID_DEMO_SOCKET_BYTE_CODE);
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

    const extrinsicResult = await getExtrinsicResult(context, 'btcSocketQueue', 'submitUnsignedPsbt');
    expect(extrinsicResult).eq('InvalidSocketMessage');
  });

  it('should fail to submit unsigned psbt - message status is not accepted', async function () {
    const socket = await deployDemoSocket(context, INVALID_STATUS_DEMO_SOCKET_BYTE_CODE);
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

    const extrinsicResult = await getExtrinsicResult(context, 'btcSocketQueue', 'submitUnsignedPsbt');
    expect(extrinsicResult).eq('InvalidSocketMessage');
  });

  it('should fail to submit unsigned psbt - user is not registered', async function () {
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
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcSocketQueue', 'submitUnsignedPsbt');
    expect(extrinsicResult).eq('UserDNE');
  });

  it('should fail to submit unsigned psbt - invalid psbt bytes', async function () {
    await joinRegistrationPool(context);
    await submitVaultKey(context);

    const msg = {
      authorityId: alith.address,
      socketMessages: [VALID_SOCKET_MESSAGE],
      psbt: INVALID_UNSIGNED_PSBT
    };

    const signature = '0xb705f5b89d60394b4cadd2c92cd278d0542666a5a4db4f3df8945c086dc3be90001db99698e9b46144be7656fc0fb0f5ae4a1e5cf5e812ab25bcb1dc1cbf467f1b';
    await context.polkadotApi.tx.btcSocketQueue.submitUnsignedPsbt(msg, signature).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcSocketQueue', 'submitUnsignedPsbt');
    expect(extrinsicResult).eq('InvalidPsbt');
  });

  it('should fail to submit unsigned psbt - socket message duplication', async function () {
    const msg = {
      authorityId: alith.address,
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
      socketMessages: [VALID_SOCKET_MESSAGE],
      psbt: INVALID_UNSIGNED_PSBT_WITHOUT_REFUND
    };

    const signature = '0x957ca7141ee7dd273ef3726048da2bd08291e02038e426fedeba652e6ec200ba2320f230df12abd8f32649a4f4651904a754d968960c760fc19ccd9304d980e81c';
    await context.polkadotApi.tx.btcSocketQueue.submitUnsignedPsbt(msg, signature).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcSocketQueue', 'submitUnsignedPsbt');
    expect(extrinsicResult).eq('InvalidPsbt');
  });

  it('should fail to submit unsigned psbt - first tx output is not refund', async function () {
    const msg = {
      authorityId: alith.address,
      socketMessages: [VALID_SOCKET_MESSAGE],
      psbt: INVALID_UNSIGNED_PSBT_WITH_OUTPUT_WRONG_ORDER
    };

    const signature = '0x4cb32a71f925dfa14e18fce9b789f8e3fc2e71be231fc6062f2d53ad49e1abc56fa127f0e82179829e0b4fb6bea592a1f7c81312bc8e0c468b7ee9ea8c0fcd841c';
    await context.polkadotApi.tx.btcSocketQueue.submitUnsignedPsbt(msg, signature).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcSocketQueue', 'submitUnsignedPsbt');
    expect(extrinsicResult).eq('InvalidPsbt');
  });

  it('should fail to submit unsigned psbt - tx output with wrong amount', async function () {
    const msg = {
      authorityId: alith.address,
      socketMessages: [VALID_SOCKET_MESSAGE],
      psbt: INVALID_UNSIGNED_PSBT_WITH_WRONG_AMOUNT
    };

    const signature = '0x2941ca5cc2e72c07996f1c9da60d1c598c9a5efff013ffd926dfce1adc68d2a76e78e2b5f15a0f89c84786fb0a3bb99d9732263caa9cebd9d78721f59c0b135b1c';
    await context.polkadotApi.tx.btcSocketQueue.submitUnsignedPsbt(msg, signature).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcSocketQueue', 'submitUnsignedPsbt');
    expect(extrinsicResult).eq('InvalidPsbt');
  });

  it('should successfully submit an unsigned psbt', async function () {
    const msg = {
      authorityId: alith.address,
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
      unsignedPsbt: INVALID_UNSIGNED_PSBT,
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

    const signature = '0xbf3327d8d3dd0b64b876b481f74f39ee74a84e5c983720c5b7824e03e020c8f14df4001e4745097fe9c288fa0b311019111e0a7eaa5a08ec122b64b1a606b7671b';
    await context.polkadotApi.tx.btcSocketQueue.submitSignedPsbt(msg, signature).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcSocketQueue', 'submitSignedPsbt');
    expect(extrinsicResult).eq('InvalidPsbt');
  });

  it('should fail to submit signed psbt - invalid signed psbt format', async function () {
    const msg = {
      authorityId: alithRelayer.address,
      unsignedPsbt: VALID_UNSIGNED_PSBT,
      signedPsbt: INVALID_SIGNED_PSBT,
    };

    const signature = '0xef4f340fa0c5f382a81842a4b365abc06093efb1bae70650dda4f451aa6217d778a624c72c4b3b1032bdc2f040c6295cc8c340e9c781d4b320e5df4645ce07111c';
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

    const signature = '0xfef806390258ed49edf83eccaa7b84510a68279e0a8008123e58fa0091da31d745d83cb2ee051238feadca10a915c110e49bf26e6ef07a5f386a328bababee6f1c';
    await context.polkadotApi.tx.btcSocketQueue.submitSignedPsbt(msg, signature).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcSocketQueue', 'submitSignedPsbt');
    expect(extrinsicResult).eq('InvalidPsbt');
  });

  it('should successfully submit a signed psbt', async function () {
    const msg = {
      authorityId: alithRelayer.address,
      unsignedPsbt: VALID_UNSIGNED_PSBT,
      signedPsbt: VALID_SIGNED_PSBT,
    };

    await context.polkadotApi.tx.btcSocketQueue.submitSignedPsbt(msg, VALID_SIGNED_PSBT_SUBMISSION_SIG).send();
    await context.createBlock();

    const rawPendingRequest: any = await context.polkadotApi.query.btcSocketQueue.pendingRequests(VALID_UNSIGNED_PSBT_HASH);
    const pendingRequest = rawPendingRequest.toHuman();

    expect(pendingRequest).is.null;

    const rawAcceptedRequest: any = await context.polkadotApi.query.btcSocketQueue.acceptedRequests(VALID_UNSIGNED_PSBT_HASH);
    const acceptedRequest = rawAcceptedRequest.toHuman();

    expect(acceptedRequest).is.ok;
    expect(acceptedRequest.unsignedPsbt).is.eq(VALID_UNSIGNED_PSBT);
    expect(acceptedRequest.signedPsbts[alithRelayer.address]).is.eq(VALID_SIGNED_PSBT);
    expect(acceptedRequest.socketMessages).contains(VALID_SOCKET_MESSAGE);
  });
});

describeDevNode('pallet_btc_socket_queue - finalize request', (context) => {
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

    const msg2 = {
      authorityId: alithRelayer.address,
      unsignedPsbt: VALID_UNSIGNED_PSBT,
      signedPsbt: VALID_SIGNED_PSBT,
    };

    await context.polkadotApi.tx.btcSocketQueue.submitSignedPsbt(msg2, VALID_SIGNED_PSBT_SUBMISSION_SIG).send();
    await context.createBlock();
  });

  it('should fail to finalize request - invalid authority', async function () {
    const msg = {
      authorityId: alithRelayer.address,
      psbtHash: VALID_UNSIGNED_PSBT_HASH,
    };

    let errorMsg = '';
    await context.polkadotApi.tx.btcSocketQueue.finalizeRequest(msg, INVALID_FINALIZE_REQUEST_SIG).send().catch(err => {
      if (err instanceof Error) {
        errorMsg = err.message;
      }
    });
    await context.createBlock();

    expect(errorMsg).eq('1010: Invalid Transaction: Invalid signing address');
  });

  it('should fail to finalize request - invalid authority', async function () {
    const msg = {
      authorityId: alith.address,
      psbtHash: VALID_UNSIGNED_PSBT_HASH,
    };

    let errorMsg = '';
    await context.polkadotApi.tx.btcSocketQueue.finalizeRequest(msg, INVALID_FINALIZE_REQUEST_SIG).send().catch(err => {
      if (err instanceof Error) {
        errorMsg = err.message;
      }
    });
    await context.createBlock();

    expect(errorMsg).eq('1010: Invalid Transaction: Transaction has a bad signature');
  });

  it('should fail to finalize request - unknown request', async function () {
    const msg = {
      authorityId: alith.address,
      psbtHash: INVALID_UNSIGNED_PSBT_HASH,
    };

    await context.polkadotApi.tx.btcSocketQueue.finalizeRequest(msg, INVALID_FINALIZE_REQUEST_SIG).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcSocketQueue', 'finalizeRequest');
    expect(extrinsicResult).eq('RequestDNE');
  });

  it('should successfully finalize accepted request', async function () {
    const msg = {
      authorityId: alith.address,
      psbtHash: VALID_UNSIGNED_PSBT_HASH,
    };

    await context.polkadotApi.tx.btcSocketQueue.finalizeRequest(msg, VALID_FINALIZE_REQUEST_SIG).send();
    await context.createBlock();

    const rawAcceptedRequest: any = await context.polkadotApi.query.btcSocketQueue.acceptedRequests(VALID_UNSIGNED_PSBT_HASH);
    const acceptedRequest = rawAcceptedRequest.toHuman();

    expect(acceptedRequest).is.null;

    const rawFinalizedRequest: any = await context.polkadotApi.query.btcSocketQueue.finalizedRequests(VALID_UNSIGNED_PSBT_HASH);
    const finalizedRequest = rawFinalizedRequest.toHuman();

    expect(finalizedRequest).is.ok;
    expect(finalizedRequest.unsignedPsbt).is.eq(VALID_UNSIGNED_PSBT);
    expect(finalizedRequest.signedPsbts[alithRelayer.address]).is.eq(VALID_SIGNED_PSBT);
    expect(finalizedRequest.socketMessages).contains(VALID_SOCKET_MESSAGE);
  });
});
