import { BigNumber } from 'bignumber.js';
import yargs from 'yargs';
import { hideBin } from 'yargs/helpers';

import { ApiPromise, WsProvider } from '@polkadot/api';
import { Keyring } from '@polkadot/keyring';

import {
  TEST_CONTROLLERS, TEST_RELAYERS, TEST_STASHES
} from '../constants/keys';

/**
 * setup node prerequisites
 * 1. key generation
 * 2. key insertion
 * 3. join validator pool
 */
async function setNode() {
  const argv = await yargs(hideBin(process.argv))
    .usage('Usage: npm run set_node [args]')
    .version('1.0.0')
    .options({
      index: {
        type: 'number',
        describe: 'Node index to execute (The genesis node is 0)',
        default: 0,
      },
      provider: {
        type: 'string',
        describe: 'Provider endpoint. WebSocket provider is required.',
        default: 'ws://127.0.0.1:9944'
      },
      'single-account': {
        type: 'boolean',
        describe: 'The condition flag if the validator accounts identical',
        default: false,
      },
      full: {
        type: 'boolean',
        describe: 'The condition flag if the executed node will be participated as a full node',
        default: false,
      },
    }).help().argv;

  if (isNaN(Number(argv.index))) {
    console.error('⚠️  Please pass a numeric node index.');
    return;
  }
  if (Number(argv.index) < 0) {
    console.error('⚠️  Please pass a positive numeric node index.');
    return;
  }
  if (Number(argv.index) === 0) {
    console.error('⚠️  Cannot set main node.');
    return;
  }
  console.log(`[*] passed node index = ${argv.index}`);
  console.log(`[*] node endpoint = ${argv.provider}`);

  const CONTROLLER = TEST_CONTROLLERS[argv.index];
  const STASH = argv.singleAccount ? TEST_CONTROLLERS[argv.index] : TEST_STASHES[argv.index];
  const RELAYER = argv.singleAccount ? TEST_CONTROLLERS[argv.index] : TEST_RELAYERS[argv.index];

  // setup polkadot api
  const keyring = new Keyring({ type: 'ethereum' });
  const controller = keyring.addFromUri(CONTROLLER.private);
  const stash = keyring.addFromUri(STASH.private);
  const provider = new WsProvider(argv.provider);
  const api = await ApiPromise.create({ provider, noInitWarn: true });

  const AMOUNT_FACTOR = 10 ** 18;

  // rotate session keys. this will generate a new session key pair inside your nodes chain data directory
  const sessionKeys = (await api.rpc.author.rotateKeys()).toHex().slice(2);
  const auraSessionKey = `0x${sessionKeys.slice(0, 64)}`;
  const granSessionKey = `0x${sessionKeys.slice(64, 128)}`;
  const imonSessionKey = `0x${sessionKeys.slice(128)}`;

  console.log(`session_key: ${sessionKeys}`);
  console.log(`aura session_key: ${auraSessionKey}`);
  console.log(`gran session_key: ${granSessionKey}`);
  console.log(`imonline session_key: ${imonSessionKey}`);

  // insert session key
  const tx01 = await api.tx.session.setKeys(sessionKeys, '0x00').signAndSend(controller, { nonce: -1 });

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

setNode().catch(error => {
  console.error(error);
  process.exit(1);
});
