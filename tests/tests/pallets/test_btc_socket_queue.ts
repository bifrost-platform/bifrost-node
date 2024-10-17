import { expect } from 'chai';
import { TransactionReceiptAPI } from 'web3';

import { Keyring } from '@polkadot/api';

import {
  DEMO_BITCOIN_SOCKET_ABI, DEMO_BITCOIN_SOCKET_BYTE_CODE, DEMO_SOCKET_ABI,
  INVALID_DEMO_SOCKET_BYTE_CODE_WITH_INVALID_MSG_HASH,
  INVALID_DEMO_SOCKET_BYTE_CODE_WITH_INVALID_STATUS, VALID_DEMO_SOCKET_BYTE_CODE
} from '../../constants/demo_contract';
import { TEST_CONTROLLERS, TEST_RELAYERS } from '../../constants/keys';
import { getExtrinsicResult } from '../extrinsics';
import { describeDevNode, INodeContext } from '../set_dev_node';

const SOCKET_MESSAGE_SEQ_ID = 4657;

// submit_executed_request()
const VALID_EXECUTED_REQUEST_SUBMISSION_SIG = '0x98e46db597e3e3b0f2797ad7d8db39ad57b48b05556469423045e418365daedf63a07decf059965192db16fa7d54cc439a3f32851605ce33b7e3c171854eb3f61c';

// submit_unsigned_psbt()
const VALID_PSBT_TXID = '0x4356d8ce9259a22d2afc0ad7ba34bd349bf9d2bc7e28e676d22fc7cb3fa822a7';
const VALID_UNSIGNED_PSBT_SUBMISSION_SIG = '0xd4dbf66950ff096f2b09b7ca64741b90e218e8e0e1c16f6c925c5a1d771a6472774222d41a92d3096a999639f438d583468b656414bdb50561a9aeb33f2ce9871b';
const VALID_UNSIGNED_PSBT_SUBMISSION_SIG_WITHOUT_REFUND = '0x4044f7f11789062aafed5d38db630da3547ae48d807b5c484d11393ad9d5fcd57fc2ee1167a58624290442c8bfdccec084d60c672d94479359c438c6dc9ad5611b';
const VALID_UNSIGNED_PSBT_SUBMISSION_SIG_WITH_WRONG_AMOUNT = '0xa451add673191f611fd1659daeb086be69f89f36bb5bf5c564a71f352aa615cf32d364d93b0f825a9c3dc45ed0d0540faa686371fe8671d119b2e24ca647730c1b';
const VALID_UNSIGNED_PSBT = '0x70736274ff01007d020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff0200ca9a3b0000000022002001f910a6a2d3f8d3562ea46b75c057387c6db6d49cb3b863c884f50da1a8450e9584284800000000160014e0e55307ae2d25f1a8ff05fb3b25a0c67cbead16000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be0105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b00088000000';

const INVALID_UNSIGNED_PSBT_SUBMISSION_SIG = '0x64a7298c65a13542ea3b480f83822d8569398274e25aa920ba0f28cc7412750e23208877c20756ffaba0739356bc73cf09abdcd8be5045249be341785dff4fae1c';
const INVALID_UNSIGNED_PSBT_WITHOUT_REFUND = '0x70736274ff01005e020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff0100ca9a3b0000000022002001f910a6a2d3f8d3562ea46b75c057387c6db6d49cb3b863c884f50da1a8450e000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be0105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b000880000';
const INVALID_UNSIGNED_PSBT_WITH_WRONG_AMOUNT = '0x70736274ff01007d020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff0200ca9a3b0000000022002001f910a6a2d3f8d3562ea46b75c057387c6db6d49cb3b863c884f50da1a8450e9284284800000000160014e0e55307ae2d25f1a8ff05fb3b25a0c67cbead16000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be0105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b00088000000';

const VALID_SOCKET_MESSAGE = '0x00000000000000000000000000000000000000000000000000000000000000200000bfc000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000036c000000000000000000000000000000000000000000000000000000000000123100000000000000000000000000000000000000000000000000000000000000050000271200000000000000000000000000000000000000000000000000000000040207030100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000e0000000080000000300000bfc7e3a761afcec9f3e2fb7e853ffc45a62319143fa00000000000000000000000000000000000000000000000000000000000000000000000000000000000000003cd0a705a2dc65e5b1e1205896baa2be8a07c6e00000000000000000000000003cd0a705a2dc65e5b1e1205896baa2be8a07c6e0000000000000000000000000000000000000000000000000000000004828849500000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000000000000';
const INVALID_SOCKET_MESSAGE_WITH_INVALID_BYTES = '0x100000000000000000000000000000000000000000000000000000000000002000000bfc00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000036c000000000000000000000000000000000000000000000000000000000000123100000000000000000000000000000000000000000000000000000000000000050000008900000000000000000000000000000000000000000000000000000000040207030100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000e0000000080000000300000bfc7e3a761afcec9f3e2fb7e853ffc45a62319143fa00000000000000000000000000000000000000000000000000000000000000000000000000000000000000003cd0a705a2dc65e5b1e1205896baa2be8a07c6e00000000000000000000000003cd0a705a2dc65e5b1e1205896baa2be8a07c6e0000000000000000000000000000000000000000000000000000000004828849500000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000000000000';
const INVALID_SOCKET_MESSAGE_WITH_INVALID_BRIDGE_CHAINS = '0x000000000000000000000000000000000000000000000000000000000000002000000bfc00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000036c000000000000000000000000000000000000000000000000000000000000123100000000000000000000000000000000000000000000000000000000000000050000008900000000000000000000000000000000000000000000000000000000040207030100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000e0000000080000000300000bfc7e3a761afcec9f3e2fb7e853ffc45a62319143fa00000000000000000000000000000000000000000000000000000000000000000000000000000000000000003cd0a705a2dc65e5b1e1205896baa2be8a07c6e00000000000000000000000003cd0a705a2dc65e5b1e1205896baa2be8a07c6e0000000000000000000000000000000000000000000000000000000004828849500000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000000000000'

// submit_signed_psbt()
const VALID_SIGNED_PSBT_SUBMISSION_SIG = '0x20ff86f4a74ad37663a7493d2d5e33bd494cea183e09e881e8af22fa5dc8d5a40cec6cb42a85cb6f2962e4f2271048a7ffb51dea824ad0a796d0fcc162d5df7d1c';
const VALID_SIGNED_PSBT = '0x70736274ff01007d020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff0200ca9a3b0000000022002001f910a6a2d3f8d3562ea46b75c057387c6db6d49cb3b863c884f50da1a8450e9584284800000000160014e0e55307ae2d25f1a8ff05fb3b25a0c67cbead16000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be2202024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076648304502210082620df266e4bbe7df2a4e9e29e402efd3858efd2a69a217092caef9d26f663f022030d281a151db5233e20d1e8e5762378edc0292360d0dd759cdf4ee7d06e6b86d01220202531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe3374830450221009077fc4b17587c6e699fa2b4ae9326bbec8ba5c6f3683a50137bb8a0e02d9ab602201adfa4a490f770dbdb3b5363628af1f8fc3a48dc748b7214195e72f842357a96012202031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f483045022100b8b01d9721ce1901a89103faee2441ee253dcc2fa0b7ff794cedd2476aea9ebe0220594e1ed55dfcd90cff18ac8400542b28559fc2d44ed02603d272a6a044dd2bf9010105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b00088000000';

const INVALID_SIGNED_PSBT_SUBMISSION_SIG = '0x1ddb5590467cc3b90495696689229bb7f5f552e7a4e4e37bc05b01bd83e22d50196958434b1010ddcd14ca4b5c6c3a398c3a0e1f61da147295ee63dbc07de66f1b';
const INVALID_SIGNED_PSBT_WITH_WRONG_ORIGIN = '0x70736274ff01007d020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff0200ca9a3b0000000022002001f910a6a2d3f8d3562ea46b75c057387c6db6d49cb3b863c884f50da1a8450e9284284800000000160014e0e55307ae2d25f1a8ff05fb3b25a0c67cbead16000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be0108fc040047304402203a1dd209d8a6c163759c2ec37561cb48d7f466cfd650813348b63e85583277ce022060fdccbb6b0f863b6933c1623a946d1c3cbbf121f4982747d4fc18b3195ef2b50147304402201f839262864d169e564644dcbc00cc9226b7dd42621568367b654aa89455cbb2022020a3dfc14590342badbe9ee4810c2203c3b88bd5887dc8b8073ae84cc574495401695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae000000';
const INVALID_SIGNED_PSBT_WITH_UNFINALIZED = '0x70736274ff01007d020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff0200ca9a3b0000000022002001f910a6a2d3f8d3562ea46b75c057387c6db6d49cb3b863c884f50da1a8450e9584284800000000160014e0e55307ae2d25f1a8ff05fb3b25a0c67cbead16000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be2202024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076648304502210082620df266e4bbe7df2a4e9e29e402efd3858efd2a69a217092caef9d26f663f022030d281a151db5233e20d1e8e5762378edc0292360d0dd759cdf4ee7d06e6b86d010105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b00088000000';

// submit_rollback_request()
const VALID_ROLLBACK_PSBT = '0x70736274ff010052020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff019284284800000000160014e0e55307ae2d25f1a8ff05fb3b25a0c67cbead16000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be0105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b000880000';
const INVALID_ROLLBACK_PSBT_WITH_WRONG_OUTPUT_TO = '0x70736274ff010052020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff01928428480000000016001455768e86925d0d680ff3ee5a3338875b01c1869f000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be0105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b000880000';
const INVALID_ROLLBACK_PSBT_WITH_WRONG_OUTPUT_AMOUNT = '0x70736274ff010052020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff019684284800000000160014e0e55307ae2d25f1a8ff05fb3b25a0c67cbead16000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be0105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b000880000';
const VALID_ROLLBACK_PSBT_TXID = '0x7e4b764c82bad01dae9e279c35d74f2b03a115e7f9dd7040b3f32d63520bbe28';

// submit_system_vault_key()
const SYSTEM_VAULT_PUBKEY = '0x02c56c0cf38df8708f2e5725102f87a1d91f9356b0b7ebc4f6cafb396684e143b4';
const SYSTEM_VAULT_PUBKEY_SUBMISSION_SIG = '0xd19701003fb3b0ad88cad82c85da2bf01b1e6855c0636384fd23ba061ec0fbc077c386a05f013f3f0f53faa5fe59f977cc557a7176ba00acfc7655a6767a121d1b';

// submit_vault_key()
const VAULT_PUBKEY = '0x03e4f6fb93d47f69aed9338553e3ef1871a2b963f287268ca23cbf6fd3fc7dc6d9';
const VAULT_PUBKEY_SUBMISSION_SIG = '0x82f8ae4150a2ac96ea0026a641a1d8000b26280dc5496723da078ed50a5ef3ae546fadf4646b65214c60ba38f7b7f1b227b514d2529cc623b970365b9c7ef04f1b';

const ZERO_ADDRESS = '0x0000000000000000000000000000000000000000';
const REFUND_ADDRESS = 'bcrt1qurj4xpaw95jlr28lqhankfdqce7tatgkeqrk9q';
const REFUND_ADDRESS_2 = 'bcrt1q24mgap5jt5xksrlnaedrxwy8tvqurp5l0600ag';
const SYSTEM_VAULT = 'bcrt1qq8u3pf4z60udx43w534htszh8p7xmdk5njemsc7gsn6smgdgg58qvavm86';

async function joinRegistrationPool(context: INodeContext, refund: string, pk: string) {
  const keyring = new Keyring({ type: 'ethereum' });
  const who = keyring.addFromUri(pk);

  await context.polkadotApi.tx.btcRegistrationPool.requestVault(refund).signAndSend(who);
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
    poolRound: 1,
  };

  await context.polkadotApi.tx.btcRegistrationPool.submitVaultKey(keySubmission, VAULT_PUBKEY_SUBMISSION_SIG).send();
  await context.createBlock();
}

async function setSocket(context: INodeContext, address: string, is_bitcoin: boolean) {
  const keyring = new Keyring({ type: 'ethereum' });
  const sudo = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  await context.polkadotApi.tx.sudo.sudo(
    context.polkadotApi.tx.btcSocketQueue.setSocket(address, is_bitcoin)
  ).signAndSend(sudo);
  await context.createBlock();
}

async function deployDemoSocket(context: INodeContext, bytecode: string) {
  const deployTx = ((new context.web3.eth.Contract(DEMO_SOCKET_ABI) as any).deploy({
    data: bytecode
  }));
  const receipt = await sendTx(context, deployTx, null);
  return receipt?.contractAddress;
}

async function deployDemoBitcoinSocket(context: INodeContext, bytecode: string) {
  const deployTx = ((new context.web3.eth.Contract(DEMO_BITCOIN_SOCKET_ABI) as any).deploy({
    data: bytecode
  }));
  const receipt = await sendTx(context, deployTx, null);
  return receipt?.contractAddress;
}

async function insertDummyTxInfo(context: INodeContext, address: string) {
  const tx = ((new context.web3.eth.Contract(DEMO_BITCOIN_SOCKET_ABI, address) as any).methods.insert_dummy());
  await sendTx(context, tx, address);
}

async function clearDummyTxInfo(context: INodeContext, address: string) {
  const tx = ((new context.web3.eth.Contract(DEMO_BITCOIN_SOCKET_ABI, address) as any).methods.clear_dummy());
  await sendTx(context, tx, address);
}

const sendTx = async (context: INodeContext, tx: any, to: string | null): Promise<TransactionReceiptAPI | undefined> => {
  const signedTx = (await context.web3.eth.accounts.signTransaction({
    to,
    from: TEST_CONTROLLERS[3].public,
    data: tx.encodeABI(),
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

  return receipt;
};

async function requestSystemVault(context: INodeContext) {
  const keyring = new Keyring({ type: 'ethereum' });
  const sudo = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  await context.polkadotApi.tx.sudo.sudo(
    context.polkadotApi.tx.btcRegistrationPool.requestSystemVault(false)
  ).signAndSend(sudo);
  await context.createBlock();
}

async function submitSystemVaultKey(context: INodeContext) {
  const keyring = new Keyring({ type: 'ethereum' });
  const relayer = keyring.addFromUri(TEST_RELAYERS[0].private);
  const submit = {
    authorityId: relayer.address,
    who: '0x0000000000000000000000000000000000000100',
    pubKey: SYSTEM_VAULT_PUBKEY,
    poolRound: 1,
  };

  await context.polkadotApi.tx.btcRegistrationPool.submitSystemVaultKey(submit, SYSTEM_VAULT_PUBKEY_SUBMISSION_SIG).send();
  await context.createBlock();
}

async function setMaxFeeRate(context: INodeContext) {
  const keyring = new Keyring({ type: 'ethereum' });
  const sudo = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  await context.polkadotApi.tx.sudo.sudo(
    context.polkadotApi.tx.btcSocketQueue.setMaxFeeRate(1000000000)
  ).signAndSend(sudo);
  await context.createBlock();
}

describeDevNode('pallet_btc_socket_queue - submit unsigned pbst', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);

  before(async function () {
    await setMaxFeeRate(context);
  });

  it('should fail to submit unsigned psbt - invalid authority', async function () {
    const msg = {
      authorityId: baltathar.address,
      outputs: [[REFUND_ADDRESS, [VALID_SOCKET_MESSAGE]], [SYSTEM_VAULT, []]],
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
      outputs: [[REFUND_ADDRESS, [VALID_SOCKET_MESSAGE]], [SYSTEM_VAULT, []]],
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
      outputs: [[REFUND_ADDRESS, [VALID_SOCKET_MESSAGE]], [SYSTEM_VAULT, []]],
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
      outputs: [[REFUND_ADDRESS, [VALID_SOCKET_MESSAGE]], [SYSTEM_VAULT, []]],
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
      outputs: [[REFUND_ADDRESS, []], [SYSTEM_VAULT, []]],
      psbt: VALID_UNSIGNED_PSBT
    };

    await context.polkadotApi.tx.btcSocketQueue.submitUnsignedPsbt(msg, VALID_UNSIGNED_PSBT_SUBMISSION_SIG).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcSocketQueue', 'submitUnsignedPsbt');
    expect(extrinsicResult).eq('InvalidPsbt');
  });

  it('should fail to submit unsigned psbt - invalid socket message bytes', async function () {
    const msg = {
      authorityId: alith.address,
      outputs: [[REFUND_ADDRESS, [INVALID_SOCKET_MESSAGE_WITH_INVALID_BYTES]], [SYSTEM_VAULT, []]],
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
      outputs: [[REFUND_ADDRESS, [VALID_SOCKET_MESSAGE]], [SYSTEM_VAULT, []]],
      psbt: VALID_UNSIGNED_PSBT
    };

    await context.polkadotApi.tx.btcSocketQueue.submitUnsignedPsbt(msg, VALID_UNSIGNED_PSBT_SUBMISSION_SIG).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcSocketQueue', 'submitUnsignedPsbt');
    expect(extrinsicResult).eq('SocketDNE');
  });

  it('should fail to submit unsigned psbt - invalid request info response', async function () {
    await setSocket(context, ZERO_ADDRESS, false); // set socket to wrong address

    const msg = {
      authorityId: alith.address,
      outputs: [[REFUND_ADDRESS, [VALID_SOCKET_MESSAGE]], [SYSTEM_VAULT, []]],
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
      await setSocket(context, socket, false);
    }

    const msg = {
      authorityId: alith.address,
      outputs: [[REFUND_ADDRESS, [VALID_SOCKET_MESSAGE]], [SYSTEM_VAULT, []]],
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
      await setSocket(context, socket, false);
    }

    const msg = {
      authorityId: alith.address,
      outputs: [[REFUND_ADDRESS, [VALID_SOCKET_MESSAGE]], [SYSTEM_VAULT, []]],
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
      await setSocket(context, socket, false);
    }

    const msg = {
      authorityId: alith.address,
      outputs: [[REFUND_ADDRESS, [INVALID_SOCKET_MESSAGE_WITH_INVALID_BRIDGE_CHAINS]], [SYSTEM_VAULT, []]],
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
      outputs: [[REFUND_ADDRESS, [VALID_SOCKET_MESSAGE]], [SYSTEM_VAULT, []]],
      psbt: VALID_UNSIGNED_PSBT
    };

    await context.polkadotApi.tx.btcSocketQueue.submitUnsignedPsbt(msg, VALID_UNSIGNED_PSBT_SUBMISSION_SIG).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcSocketQueue', 'submitUnsignedPsbt');
    expect(extrinsicResult).eq('UserDNE');
  });

  it('should fail to submit unsigned psbt - socket message duplication', async function () {
    await joinRegistrationPool(context, REFUND_ADDRESS, TEST_CONTROLLERS[1].private);
    await submitVaultKey(context);

    const msg = {
      authorityId: alith.address,
      outputs: [[REFUND_ADDRESS, [VALID_SOCKET_MESSAGE, VALID_SOCKET_MESSAGE]], [SYSTEM_VAULT, []]],
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
      outputs: [[REFUND_ADDRESS, [VALID_SOCKET_MESSAGE]], [SYSTEM_VAULT, []]],
      psbt: INVALID_UNSIGNED_PSBT_WITHOUT_REFUND
    };

    await context.polkadotApi.tx.btcSocketQueue.submitUnsignedPsbt(msg, VALID_UNSIGNED_PSBT_SUBMISSION_SIG_WITHOUT_REFUND).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcSocketQueue', 'submitUnsignedPsbt');
    expect(extrinsicResult).eq('InvalidPsbt');
  });

  it('should fail to submit unsigned psbt - tx output with wrong amount', async function () {
    const msg = {
      authorityId: alith.address,
      outputs: [[REFUND_ADDRESS, [VALID_SOCKET_MESSAGE]], [SYSTEM_VAULT, []]],
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
      outputs: [[REFUND_ADDRESS, [VALID_SOCKET_MESSAGE]], [SYSTEM_VAULT, []]],
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
    await setMaxFeeRate(context);

    await requestSystemVault(context);
    await submitSystemVaultKey(context);

    await joinRegistrationPool(context, REFUND_ADDRESS, TEST_CONTROLLERS[1].private);
    await submitVaultKey(context);

    const socket = await deployDemoSocket(context, VALID_DEMO_SOCKET_BYTE_CODE);
    if (socket) {
      await setSocket(context, socket, false);
    }

    const msg = {
      authorityId: alith.address,
      outputs: [[REFUND_ADDRESS, [VALID_SOCKET_MESSAGE]], [SYSTEM_VAULT, []]],
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

    const signature = '0x63c2b1750cff52718aa691bda208e8c16ed75b6cac016ff4c8606b40a35b39e875e00fdcb9be4ee629cfc55df3e4193bdb3aecf43397243adea5f735adc70eea1b';
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

    const signature = '0xd2bbb7600c18ebbe84f05fc5ac18d6099481c016b422a4f01f70d81db6695286010f3d1feb22075a4581a5057e56d948a909f6c59eba0f9989298f4848fd78121b';
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

    const signature = '0x7bbae705d8be964d9acb4a3d65b011f1dc6640be381fe724a24a984b758f71fd1b57618d9c191d559d537b1d3eaf98c5f0c524d16c77e8f350a573c06f04b1e01b';
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
    await setMaxFeeRate(context);

    await requestSystemVault(context);
    await submitSystemVaultKey(context);

    await joinRegistrationPool(context, REFUND_ADDRESS, TEST_CONTROLLERS[1].private);
    await submitVaultKey(context);

    const socket = await deployDemoSocket(context, VALID_DEMO_SOCKET_BYTE_CODE);
    if (socket) {
      await setSocket(context, socket, false);
    }

    const msg = {
      authorityId: alith.address,
      outputs: [[REFUND_ADDRESS, [VALID_SOCKET_MESSAGE]], [SYSTEM_VAULT, []]],
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

describeDevNode('pallet_btc_socket_queue - rollback request', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const charleth = keyring.addFromUri(TEST_CONTROLLERS[2].private);

  before('should successfully deploy bitcoin socket contract', async function () {
    const socket = await deployDemoBitcoinSocket(context, DEMO_BITCOIN_SOCKET_BYTE_CODE);
    if (socket) {
      await setSocket(context, socket, true);
    }
  });

  before('should join registration pool', async function () {
    await joinRegistrationPool(context, REFUND_ADDRESS, TEST_CONTROLLERS[1].private);
    await submitVaultKey(context);
  });

  before('should generate system vault', async function () {
    await setMaxFeeRate(context);

    await requestSystemVault(context);
    await submitSystemVaultKey(context);
  });

  it('should fail to submit rollback request - unknown user', async function () {
    const msg = {
      who: charleth.address,
      txid: VALID_PSBT_TXID,
      vout: 0,
      amount: 1210614933,
      unsignedPsbt: VALID_ROLLBACK_PSBT,
    };

    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.btcSocketQueue.submitRollbackRequest(msg)
    ).signAndSend(alith);
    await context.createBlock();

    const rawRollbackRequest: any = await context.polkadotApi.query.btcSocketQueue.rollbackRequests(VALID_ROLLBACK_PSBT_TXID);
    const rollbackRequest = rawRollbackRequest.toHuman();
    expect(rollbackRequest).is.null;
  });

  it('should fail to submit rollback request - pending vault', async function () {
    await joinRegistrationPool(context, REFUND_ADDRESS_2, TEST_CONTROLLERS[2].private);

    const msg = {
      who: charleth.address,
      txid: VALID_PSBT_TXID,
      vout: 0,
      amount: 1210614933,
      unsignedPsbt: VALID_ROLLBACK_PSBT,
    };

    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.btcSocketQueue.submitRollbackRequest(msg)
    ).signAndSend(alith);
    await context.createBlock();

    const rawRollbackRequest: any = await context.polkadotApi.query.btcSocketQueue.rollbackRequests(VALID_ROLLBACK_PSBT_TXID);
    const rollbackRequest = rawRollbackRequest.toHuman();
    expect(rollbackRequest).is.null;
  });

  it('should fail to submit rollback request - already known txinfo', async function () {
    const rawBitcoinSocket: any = await context.polkadotApi.query.btcSocketQueue.bitcoinSocket();
    const bitcoinSocket = rawBitcoinSocket.toHuman();

    await insertDummyTxInfo(context, bitcoinSocket);
    await context.createBlock();

    const msg = {
      who: baltathar.address,
      txid: VALID_PSBT_TXID,
      vout: 0,
      amount: 1210614933,
      unsignedPsbt: VALID_ROLLBACK_PSBT,
    };

    await context.createBlock();
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.btcSocketQueue.submitRollbackRequest(msg)
    ).signAndSend(alith);
    await context.createBlock();

    const rawRollbackRequest: any = await context.polkadotApi.query.btcSocketQueue.rollbackRequests(VALID_ROLLBACK_PSBT_TXID);
    const rollbackRequest = rawRollbackRequest.toHuman();
    expect(rollbackRequest).is.null;

    await clearDummyTxInfo(context, bitcoinSocket);
    await context.createBlock();
  });

  it('should fail to submit rollback request - invalid output to address', async function () {
    const msg = {
      who: baltathar.address,
      txid: VALID_PSBT_TXID,
      vout: 0,
      amount: 1210614933,
      unsignedPsbt: INVALID_ROLLBACK_PSBT_WITH_WRONG_OUTPUT_TO,
    };

    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.btcSocketQueue.submitRollbackRequest(msg)
    ).signAndSend(alith);
    await context.createBlock();

    const rawRollbackRequest: any = await context.polkadotApi.query.btcSocketQueue.rollbackRequests(VALID_ROLLBACK_PSBT_TXID);
    const rollbackRequest = rawRollbackRequest.toHuman();
    expect(rollbackRequest).is.null;
  });

  it('should fail to submit rollback request - invalid output amount', async function () {
    const msg = {
      who: baltathar.address,
      txid: VALID_PSBT_TXID,
      vout: 0,
      amount: 1210614933,
      unsignedPsbt: INVALID_ROLLBACK_PSBT_WITH_WRONG_OUTPUT_AMOUNT,
    };

    await context.createBlock();
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.btcSocketQueue.submitRollbackRequest(msg)
    ).signAndSend(alith);
    await context.createBlock();

    const rawRollbackRequest: any = await context.polkadotApi.query.btcSocketQueue.rollbackRequests(VALID_ROLLBACK_PSBT_TXID);
    const rollbackRequest = rawRollbackRequest.toHuman();
    expect(rollbackRequest).is.null;
  });

  it('should successfully submit rollback request', async function () {
    const msg = {
      who: baltathar.address,
      txid: VALID_PSBT_TXID,
      vout: 0,
      amount: 1210614933,
      unsignedPsbt: VALID_ROLLBACK_PSBT,
    };

    await context.createBlock();
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.btcSocketQueue.submitRollbackRequest(msg)
    ).signAndSend(alith);
    await context.createBlock();

    const rawRollbackRequest: any = await context.polkadotApi.query.btcSocketQueue.rollbackRequests(VALID_ROLLBACK_PSBT_TXID);
    const rollbackRequest = rawRollbackRequest.toHuman();
    expect(rollbackRequest).is.ok;
    expect(rollbackRequest.unsignedPsbt).equal(VALID_ROLLBACK_PSBT);

    const rawBondedRollbackOutput: any = await context.polkadotApi.query.btcSocketQueue.bondedRollbackOutputs(VALID_PSBT_TXID, 0);
    const bondedRollbackOutput = rawBondedRollbackOutput.toHuman();
    expect(bondedRollbackOutput).equal(VALID_ROLLBACK_PSBT_TXID);
  });
});
