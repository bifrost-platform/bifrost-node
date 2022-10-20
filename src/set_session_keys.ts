import Web3 from 'web3';

async function set_session_keys() {
  const yargs = require('yargs/yargs');
  const { hideBin } = require('yargs/helpers');

  const axios = require('axios').default;
  const { ApiPromise, HttpProvider } = require('@polkadot/api');
  const { Keyring } = require('@polkadot/keyring');

  const argv = yargs(hideBin(process.argv))
    .usage('Usage: npm run set_session_keys [args]')
    .version('1.0.0')
    .options({
      controllerPrivate: {
        type: 'string', describe: 'Controller\'s PrivateKey (with 0x prefix)'
      },
      provider: {
        type: 'string',
        describe: 'Provider endpoint',
        default: 'http://127.0.0.1'
      },
      rpcPort: {
        type: 'number', describe: 'Node RPC Port', default: 9933
      }
    }).help().argv;

  if (!argv.controllerPrivate) {
    console.error('Please enter a valid controller private key');
    return;
  }
  if (!argv.provider || !argv.provider.startsWith('http')) {
    console.error('Please enter a valid provider');
    return;
  }
  if (!argv.rpcPort || isNaN(Number(argv.rpcPort))) {
    console.error('Please enter a valid RPC port');
    return;
  }
  let Accounts = require('web3-eth-accounts');
  let accounts = new Accounts();
  try {
    accounts.privateKeyToAccount(argv.controllerPrivate);
  } catch (err) {
    console.error('Please enter a valid controller private key');
    return;
  }

  const LOCAL_NODE_ENDPOINT: string = `${argv.provider}:${argv.rpcPort}`;

  const web3 = new Web3(LOCAL_NODE_ENDPOINT);
  try {
    const isSyncing = await web3.eth.isSyncing();
    if (isSyncing !== false) {
      console.error('Node is not completely sync yet');
      process.exit(-1);
    }
  } catch (e) {
    console.error('Node endpoint not reachable');
    process.exit(-1);
  }

  const keyring = new Keyring({ type: 'ethereum' });
  const controller = keyring.addFromUri(argv.controllerPrivate);
  const provider = new HttpProvider(LOCAL_NODE_ENDPOINT);
  const api = await ApiPromise.create({ provider });

  try {
    const sessionKey = (await axios.post(
      LOCAL_NODE_ENDPOINT,
      {
        jsonrpc: '2.0',
        method: 'author_rotateKeys',
        id: 1
      }
    )).data.result.slice(2);

    const auraSessionKey = `0x${sessionKey.slice(0, 64)}`;
    const granSessionKey = `0x${sessionKey.slice(64, 128)}`;
    const imonSessionKey = `0x${sessionKey.slice(128)}`;

    await api.tx.session.setKeys({
      aura: auraSessionKey,
      grandpa: granSessionKey,
      imOnline: imonSessionKey
    }, '0x00').signAndSend(controller, { nonce: -1 });

    console.log('\n🔑 Session Keys');
    console.log(`    aura: ${auraSessionKey}`);
    console.log(`    gran: ${granSessionKey}`);
    console.log(`    imon: ${imonSessionKey}`);
  } catch (error) {
    if (error instanceof Error) {
      console.error(
        `Failed to set session keys due to the following error: ${error.message}`);
    }
  }
}

set_session_keys().catch((error) => {
  console.error(error);
  process.exit(0);
});
