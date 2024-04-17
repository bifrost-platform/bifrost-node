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
const VALID_FINALIZE_REQUEST_SIG = '0xe0e6f6cf622c9ab93b5144de526482d4d2072d877f518dd95d6001bbea29c96222988301974919d4c4c95ade4a5f519b47e1f6653128bb7fc905962117277d021b';

// submit_unsigned_psbt()
const VALID_SYSTEM_VAULT_VOUT = 0;
const VALID_UNSIGNED_PSBT_HASH = '0xcd0c3f3caff0c74701e7d45aaaac84cab1ec6507c679a028d015e8dff3004958';
const VALID_UNSIGNED_PSBT_SUBMISSION_SIG = '0xe0e6f6cf622c9ab93b5144de526482d4d2072d877f518dd95d6001bbea29c96222988301974919d4c4c95ade4a5f519b47e1f6653128bb7fc905962117277d021b';
const VALID_UNSIGNED_PSBT_SUBMISSION_SIG_WITH_INVALID_BYTES = '0x7775f5d7af11b232d8398ce17c8d95fbf9fefc7764e62b3ca0bcfe2f07a50ab16343abf685e5996de75d1e91e1fd78d65b10a3b5aecb0e120c6c24ee9c3acc2e1b';
const VALID_UNSIGNED_PSBT_SUBMISSION_SIG_WITHOUT_REFUND = '0xbaf3a67ddb53533b451129eb50e4fbd1cb119cc71e9296fe02ea3037a977ce4a6faf7e36c3a8ccd91d3a01ec4748f5521d272d57d7e1df8a0caf93c459a565811c';
const VALID_UNSIGNED_PSBT_SUBMISSION_SIG_WITH_WRONG_OUTPUT_ORDER = '0xebba1b9f64a380f86a81f7e01f6726b157edf0c19c9c4e8d174130526fadc06c72ad7383e72bc8504737f6b9233eb198e49c7be103d0080ab1541bec6600ad2b1c';
const VALID_UNSIGNED_PSBT_SUBMISSION_SIG_WITH_WRONG_AMOUNT = '0xfcb4fa1efb447a0bd5d86d17b67ca0ab19e203aa35f1edddae1c128f46de6973789376aacd7bfad6b81ee09ae567678419045da74c63b7cf7ba22982521845f11c';
const VALID_UNSIGNED_PSBT = '0x70736274ff01007d020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff0200ca9a3b0000000022002040920e81464d4c6fba15563b0ff8e1de8e856df79edbf863cd2d9493cbcab4799584284800000000160014e0e55307ae2d25f1a8ff05fb3b25a0c67cbead16000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be0105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b00088000000';

// const INVALID_UNSIGNED_PSBT_HASH = '0x84d586783bf008ff4dccd5e7652222c64ca38e2b3cc808daddb55edfe285af41';
const INVALID_UNSIGNED_PSBT_SUBMISSION_SIG = '0x7775f5d7af11b232d8398ce17c8d95fbf9fefc7764e62b3ca0bcfe2f07a50ab16343abf685e5996de75d1e91e1fd78d65b10a3b5aecb0e120c6c24ee9c3acc2e1b';
const INVALID_UNSIGNED_PSBT_WITH_INVALID_BYTES = '0x00736274ff010089020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff0200ca9a3b0000000022002001f910a6a2d3f8d3562ea46b75c057387c6db6d49cb3b863c884f50da1a8450e95842848000000002251202d4b1f0e2a5e77e026e763f239006c8531d2ca14765db6963dec4976c0710e28000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be0105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b00088000000';
const INVALID_UNSIGNED_PSBT_WITHOUT_REFUND = '0x70736274ff01005e020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff0100ca9a3b0000000022002040920e81464d4c6fba15563b0ff8e1de8e856df79edbf863cd2d9493cbcab479000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be0105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b000880000';
const INVALID_UNSIGNED_PSBT_WITH_WRONG_OUTPUT_ORDER = '0x70736274ff01007d020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff029584284800000000160014e0e55307ae2d25f1a8ff05fb3b25a0c67cbead1600ca9a3b0000000022002040920e81464d4c6fba15563b0ff8e1de8e856df79edbf863cd2d9493cbcab479000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be0105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b00088000000';
const INVALID_UNSIGNED_PSBT_WITH_WRONG_AMOUNT = '0x70736274ff01007d020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff0200ca9a3b0000000022002040920e81464d4c6fba15563b0ff8e1de8e856df79edbf863cd2d9493cbcab4799284284800000000160014e0e55307ae2d25f1a8ff05fb3b25a0c67cbead16000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be0105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b00088000000';

const VALID_SOCKET_MESSAGE = '0x00000000000000000000000000000000000000000000000000000000000000200000bfc000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000036c000000000000000000000000000000000000000000000000000000000000123100000000000000000000000000000000000000000000000000000000000000050000271100000000000000000000000000000000000000000000000000000000040207030100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000e0000000080000000300000bfc7e3a761afcec9f3e2fb7e853ffc45a62319143fa00000000000000000000000000000000000000000000000000000000000000000000000000000000000000003cd0a705a2dc65e5b1e1205896baa2be8a07c6e00000000000000000000000003cd0a705a2dc65e5b1e1205896baa2be8a07c6e0000000000000000000000000000000000000000000000000000000004828849500000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000000000000';
const INVALID_SOCKET_MESSAGE_WITH_INVALID_BYTES = '0x100000000000000000000000000000000000000000000000000000000000002000000bfc00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000036c000000000000000000000000000000000000000000000000000000000000123100000000000000000000000000000000000000000000000000000000000000050000008900000000000000000000000000000000000000000000000000000000040207030100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000e0000000080000000300000bfc7e3a761afcec9f3e2fb7e853ffc45a62319143fa00000000000000000000000000000000000000000000000000000000000000000000000000000000000000003cd0a705a2dc65e5b1e1205896baa2be8a07c6e00000000000000000000000003cd0a705a2dc65e5b1e1205896baa2be8a07c6e0000000000000000000000000000000000000000000000000000000004828849500000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000000000000';
const INVALID_SOCKET_MESSAGE_WITH_INVALID_BRIDGE_CHAINS = '0x000000000000000000000000000000000000000000000000000000000000002000000bfc00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000036c000000000000000000000000000000000000000000000000000000000000123100000000000000000000000000000000000000000000000000000000000000050000008900000000000000000000000000000000000000000000000000000000040207030100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000e0000000080000000300000bfc7e3a761afcec9f3e2fb7e853ffc45a62319143fa00000000000000000000000000000000000000000000000000000000000000000000000000000000000000003cd0a705a2dc65e5b1e1205896baa2be8a07c6e00000000000000000000000003cd0a705a2dc65e5b1e1205896baa2be8a07c6e0000000000000000000000000000000000000000000000000000000004828849500000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000000000000'

// submit_signed_psbt()
const VALID_SIGNED_PSBT_SUBMISSION_SIG = '0x6399c38f32510e6f191fce66fdb46fb98286a429aca918cc56133bdc1081a41574946f0f1baecb5e0877604de866bcafc2ac0f89b9d4f1442cd50470f80f27fe1c';
const VALID_SIGNED_PSBT = '0x70736274ff01007d020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff0200ca9a3b0000000022002040920e81464d4c6fba15563b0ff8e1de8e856df79edbf863cd2d9493cbcab4799584284800000000160014e0e55307ae2d25f1a8ff05fb3b25a0c67cbead16000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be2202024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d0766483045022100eedf6cbfd0ebaf44b855da3940a97b0e4971629a23c3c422aa8443ce0bad5e0f02205b807f66f8ca999b28f2f965396d7821968b156e752873855fe64b161362a70401220202531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe337483045022100916010189cac425f21ca53a30e7ea94a17716d72f6c0ca9849fb708e2402d6b802204f7359ff47e2077b83fb1923f734593699c8d504f259b9786dc475bc63452e11012202031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f473044022025bee1adc4b5d5652b3c7e0fa1808d3434af11d99f7fa45649a05d27a0db95d302207fc4ef7647f36133578930dad898eab02c1718fb304dbe24b4f4399ae1cd170d010105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b00088000000';

const INVALID_SIGNED_PSBT_SUBMISSION_SIG = '0x1ddb5590467cc3b90495696689229bb7f5f552e7a4e4e37bc05b01bd83e22d50196958434b1010ddcd14ca4b5c6c3a398c3a0e1f61da147295ee63dbc07de66f1b';
const INVALID_SIGNED_PSBT_WITH_INVALID_BYTES = '0x00736274ff010089020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff0200ca9a3b0000000022002001f910a6a2d3f8d3562ea46b75c057387c6db6d49cb3b863c884f50da1a8450e95842848000000002251202d4b1f0e2a5e77e026e763f239006c8531d2ca14765db6963dec4976c0710e28000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be2202024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07664730440220209e607c752d4c5c8b03587408b872baa61743efa4d1c11cb53b381a585a00e2022059f9af0b5e35d33d43d26dccdf8e9d7fd93c5e01320b4a362151bfa886c5f59b010105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b00088000000';
const INVALID_SIGNED_PSBT_WITH_WRONG_ORIGIN = '0x70736274ff010052020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff019584284800000000160014e0e55307ae2d25f1a8ff05fb3b25a0c67cbead16000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be2202024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d0766483045022100ab5428f86b67df28de0938d0b037fc46ad0061929224849c5b88f265c449b0a7022001bd4e0d1786f05f56a08516edb54bb7022f94ee1f0d0d7eab8c92d233d85e8501220202531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe337473044022027c6f158dd5a3d440f212d722bdad2af6bdd98d98e0122221cd3a38a3fc6ee13022023fb2d78a0d1054d40e076b5bb38da4b3e1ce543614d9f4f3e423693cf42b567012202031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f483045022100faaf70fe5b55d6cd246900394172cb900b38ec7fab56075e8eeb0e268fb0809802202fb11d1ede4dd7104f55bf8dc2bbcd0cc57087df3708af0264f5d1f1793ee704010105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b000880000';
const INVALID_SIGNED_PSBT_WITH_UNFINALIZED = '0x70736274ff01007d020000000150cefd4f6b4e3bf316808aa126d8d89ce812d04d1c0b072aa30cf8f86347804b0000000000ffffffff0200ca9a3b0000000022002040920e81464d4c6fba15563b0ff8e1de8e856df79edbf863cd2d9493cbcab4799584284800000000160014e0e55307ae2d25f1a8ff05fb3b25a0c67cbead16000000000001012b00f2052a01000000220020a3379884c9919e8ae37a568e76b4af9d72b0928bf52f5ea8e5f53032691d17be2202024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d0766483045022100eedf6cbfd0ebaf44b855da3940a97b0e4971629a23c3c422aa8443ce0bad5e0f02205b807f66f8ca999b28f2f965396d7821968b156e752873855fe64b161362a704010105695221024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d07662102531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33721031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f53ae2206024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d076604ebc0ee0b220602531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe33704417d4be92206031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f0479b00088000000';

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

    const signature = '0x87134fddc1e36abffe3e25bbc68b3d7b63b0f8eb1e00532b0cf78ae090cf23895fb75eebcf094e5faf88fd22ed256143f323f7aaebf42fbc90f8b85f88047b5a1b';
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

    const signature = '0x977ba775105f331dc14d052f452fd8854daa750b094f06faa1b326c660c87d91431e0b229ec60da906dcca4500b1a446cad35f3565458001c53e000dcf6aec181c';
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

    const signature = '0x8570e335712f0b33cfe4469b944753097203caabe2282051c69d310c804f357a5d6781ff006d80b4bc1fd56e512439bcbf2de2522540e811ddda0e7ead11b5231c';
    await context.polkadotApi.tx.btcSocketQueue.submitSignedPsbt(msg, signature).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcSocketQueue', 'submitSignedPsbt');
    expect(extrinsicResult).eq('CannotFinalizePsbt');
  });

  it('should successfully submit signed psbt', async function () {
    const msg = {
      authorityId: alithRelayer.address,
      unsignedPsbt: VALID_UNSIGNED_PSBT,
      signedPsbt: VALID_SIGNED_PSBT,
    };

    await context.polkadotApi.tx.btcSocketQueue.submitSignedPsbt(msg, VALID_SIGNED_PSBT_SUBMISSION_SIG).send();
    await context.createBlock();

    const rawBondedOutboundTx: any = await context.polkadotApi.query.btcSocketQueue.bondedOutboundTx('0xa9c014fe043fd1aaa14e6d1f2dd90b148193b275ee35dfbd9c11a8f61dbd38b5');
    const bondedOutboundTx = rawBondedOutboundTx.toHuman();
    expect(bondedOutboundTx[0]).is.eq(VALID_SOCKET_MESSAGE);

    const rawAcceptedRequest: any = await context.polkadotApi.query.btcSocketQueue.acceptedRequests(VALID_UNSIGNED_PSBT_HASH);
    const acceptedRequest = rawAcceptedRequest.toHuman();
    expect(acceptedRequest).is.ok;

    const rawPendingRequest: any = await context.polkadotApi.query.btcSocketQueue.pendingRequests(VALID_UNSIGNED_PSBT_HASH);
    const pendingRequest = rawPendingRequest.toHuman();
    expect(pendingRequest).is.null;
  });
});

describeDevNode('pallet_btc_socket_queue - finalize request', (context) => {
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

  it('should successfully finalize request', async function () {
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
  });
});
