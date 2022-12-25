/**
 * setup node prerequisites
 * 1. key generation
 * 2. key insertion
 * 3. join validator pool
 */
async function setNode() {
  const axios = require('axios').default;
  const { ApiPromise, HttpProvider } = require('@polkadot/api');
  const { Keyring } = require('@polkadot/keyring');
  const { BigNumber } = require('bignumber.js');
  const { TEST_CONTROLLERS, TEST_STASHES, TEST_RELAYERS } = require('../constants/keys');

  const yargs = require('yargs/yargs');
  const { hideBin } = require('yargs/helpers');

  const argv = yargs(hideBin(process.argv))
    .usage('Usage: npm run set_node [args]')
    .version('1.0.0')
    .options({
      index: {
        type: 'number', describe: 'Node index to execute (The genesis node is 1)', default: 1,
      },
      provider: {
        type: 'string', describe: 'Rpc endpoint of the executed node ', default: 'http://127.0.0.1',
      },
      'single-account': {
        type: 'boolean', describe: 'The condition flag if the validator accounts identical', default: false,
      },
      full: {
        type: 'boolean', describe: 'The condition flag if the executed node will be participated as a full node', default: false,
      },
    }).help().argv;

  let rpcPort: number = 9933;

  if (isNaN(Number(argv.index))) {
    console.error('please pass a numeric node index');
    return;
  }
  if (Number(argv.index) < 1) {
    console.error('please pass a positive numeric node index');
    return;
  }
  if (Number(argv.index) === 1) {
    console.error('cannot set main node');
    return;
  }
  console.log(`[*] passed node index = ${argv.index}`);
  console.log(`[*] node host = ${argv.provider}`);

  rpcPort += argv.index;

  const LOCAL_NODE_ENDPOINT: string = `${argv.provider}:${rpcPort}`;
  const CONTROLLER = TEST_CONTROLLERS[argv.index - 1];
  const STASH = argv.singleAccount ? TEST_CONTROLLERS[argv.index - 1] : TEST_STASHES[argv.index - 1];
  const RELAYER = argv.singleAccount ? TEST_CONTROLLERS[argv.index - 1] : TEST_RELAYERS[argv.index - 1];

  // setup polkadot api
  const keyring = new Keyring({ type: 'ethereum' });
  const controller = keyring.addFromUri(CONTROLLER.private);
  const stash = keyring.addFromUri(STASH.private);
  const provider = new HttpProvider(LOCAL_NODE_ENDPOINT);
  const api = await ApiPromise.create({ provider });

  const AMOUNT_FACTOR = 10 ** 18;

  // generate session key
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

  // insert session key
  const tx01 = await api.tx.session.setKeys({
    aura: auraSessionKey,
    grandpa: granSessionKey,
    imOnline: imonlineSessionKey,
  }, '0x00').signAndSend(controller, { nonce: -1 });
  console.log(`[*] insert session key = ${tx01}`);

  // join validator candidate pool
  const stake = new BigNumber(1000).multipliedBy(AMOUNT_FACTOR);
  if (argv.full) {
    const tx02 = await api.tx.bfcStaking.joinCandidates(
      CONTROLLER.public,
      RELAYER.public,
      stake.toFixed(),
      100,
    ).signAndSend(stash, { nonce: -1 });
    console.log(`[*] join full candidates = ${tx02}`);
  } else {
    const tx02 = await api.tx.bfcStaking.joinCandidates(
      CONTROLLER.public,
      null,
      stake.toFixed(),
      100,
    ).signAndSend(stash, { nonce: -1 });
    console.log(`[*] join basic candidates = ${tx02}`);
  }
}
setNode();
