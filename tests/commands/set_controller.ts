import { sleep } from '../tests/utils';

async function setController() {
  const axios = require('axios').default;
  const { ApiPromise, HttpProvider } = require('@polkadot/api');
  const { Keyring } = require('@polkadot/keyring');
  const { BigNumber } = require('bignumber.js');
  const { INITIAL_ACCOUNTS, INITIAL_STASH_ACCOUNTS } = require('../constants/keys');

  let index: number = -1;
  let rpcPort: number = 9933;

  // node index (ex: 2)
  // new_controller private key
  // node host (ex: http://127.0.0.1)
  if (process.argv.length < 2 || process.argv[2] === null || process.argv[2] === undefined) {
    console.error('please pass your node index');
    return;
  }
  if (isNaN(Number(process.argv[2]))) {
    console.error('please pass a numeric node index');
    return;
  }
  if (Number(process.argv[2]) < 1) {
    console.error('please pass a positive numeric node index');
    return;
  }

  index = Number(process.argv[2]);
  console.log(`[*] passed node index = ${index}`);

  let newControllerPrivateKey = process.argv[3];
  let Accounts = require('web3-eth-accounts');
  let accounts = new Accounts();
  if (!newControllerPrivateKey.startsWith('0x')) {
    console.error('private key does not start with 0x');
    return;
  }
  try {
    let account = accounts.privateKeyToAccount(newControllerPrivateKey);
    console.log(`[*] new controller public key = ${account.address}`);
  } catch (err) {
    console.error('invalid private key received');
    return;
  }

  let host: string = `http://127.0.0.1`;
  if (process.argv[4]) {
    host = process.argv[4];
  }
  console.log(`[*] node host = ${host}`);

  rpcPort += index;

  const LOCAL_NODE_ENDPOINT: string = `${host}:${rpcPort}`;
  const OLD_CONTROLLER = INITIAL_ACCOUNTS[index - 1];
  const STASH = INITIAL_STASH_ACCOUNTS[index - 1];

  // setup polkadot api
  const keyring = new Keyring({ type: 'ethereum' });
  const old_controller = keyring.addFromUri(OLD_CONTROLLER.private);
  const new_controller = keyring.addFromUri(newControllerPrivateKey);
  const stash = keyring.addFromUri(STASH.private);
  const provider = new HttpProvider(LOCAL_NODE_ENDPOINT);
  const api = await ApiPromise.create({ provider });

  // purge session keys
  const tx01 = await api.tx.session.purgeKeys().signAndSend(old_controller, { nonce: -1 });
  console.log(`[*] purged session keys = ${tx01}`);

  // generate new session keys
  const response = await axios.post(
    LOCAL_NODE_ENDPOINT,
    {
      'jsonrpc': '2.0',
      'method': 'author_rotateKeys',
      'id': 1,
    },
  );
  const sessionKey = response.data.result.slice(2);
  const auraSessionKey = `0x${sessionKey.slice(0, 64)}`;
  const granSessionKey = `0x${sessionKey.slice(64, 128)}`;
  const imonlineSessionKey = `0x${sessionKey.slice(128)}`;
  console.log(`session_key: ${sessionKey}`);
  console.log(`aura session_key: ${auraSessionKey}`);
  console.log(`gran session_key: ${granSessionKey}`);
  console.log(`imonline session_key: ${imonlineSessionKey}`);

  // transfer funds to new controller
  const AMOUNT_FACTOR = 10 ** 18;
  const value = new BigNumber(10000).multipliedBy(AMOUNT_FACTOR);
  const tx02 = await api.tx.balances.transfer(new_controller.address, value.toFixed()).signAndSend(old_controller, { nonce: -1 });
  console.log(`[*] transfer funds to new controller = ${tx02}`);

  await sleep(3000);

  // insert new session keys
  const tx03 = await api.tx.session.setKeys({
    aura: auraSessionKey,
    grandpa: granSessionKey,
    imOnline: imonlineSessionKey,
  }, '0x00').signAndSend(new_controller, { nonce: -1 });
  console.log(`[*] insert session keys = ${tx03}`);

  // set new controller
  const tx04 = await api.tx.bfcStaking.setController(new_controller.address).signAndSend(stash, { nonce: -1 });
  console.log(`[*] set controller = ${tx04}`);
}
setController();
