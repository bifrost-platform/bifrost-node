import Web3 from 'web3';

async function join_validators() {
  const yargs = require('yargs/yargs');
  const { hideBin } = require('yargs/helpers');

  const { ApiPromise, HttpProvider } = require('@polkadot/api');
  const { Keyring } = require('@polkadot/keyring');
  const { BigNumber } = require('bignumber.js');

  const argv = yargs(hideBin(process.argv))
    .usage('Usage: npm run set_session_keys [args]')
    .version('1.0.0')
    .options({
      controllerPrivate: {
        type: 'string', describe: 'Controller\'s PrivateKey (with 0x prefix)',
      },
      stashPrivate: {
        type: 'string', describe: 'Stash\'s PrivateKey (with 0x prefix)',
      },
      relayerPrivate: {
        type: 'string',
        describe: 'Relayer\'s PrivateKey (with 0x prefix)',
        default: '',
      },
      provider: {
        type: 'string',
        describe: 'Provider endpoint',
        default: 'http://127.0.0.1',
      },
      rpcPort: {
        type: 'number', describe: 'Node RPC Port', default: 9933,
      },
      bond: {
        type: 'number',
        describe: 'Initial self-bond amount in decimal',
        default: 1000,
      },
    }).help().argv;

  if (!argv.controllerPrivate) {
    console.error('Please enter a valid controller private key');
    return;
  }
  if (!argv.stashPrivate) {
    console.error('Please enter a valid stash private key');
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
  try {
    accounts.privateKeyToAccount(argv.stashPrivate);
  } catch (err) {
    console.error('Please enter a valid stash private key');
    return;
  }
  if (argv.relayerPrivate) {
    try {
      accounts.privateKeyToAccount(argv.relayerPrivate);
    } catch (err) {
      console.error('Please enter a valid relayer private key');
      return;
    }
  }

  const LOCAL_NODE_ENDPOINT: string = `${argv.provider}:${argv.rpcPort}`;

  const web3 = new Web3(LOCAL_NODE_ENDPOINT);
  const isSyncing = await web3.eth.isSyncing();

  if (isSyncing !== false) {
    console.error('Node is not completely sync yet');
    process.exit(-1);
  }

  const keyring = new Keyring({ type: 'ethereum' });
  const controller = keyring.addFromUri(argv.controllerPrivate);
  const stash = keyring.addFromUri(argv.stashPrivate);
  const provider = new HttpProvider(LOCAL_NODE_ENDPOINT);
  const api = await ApiPromise.create({ provider });

  const AMOUNT_FACTOR = 10 ** 18;
  const selfBond = new BigNumber(argv.bond).multipliedBy(AMOUNT_FACTOR);

  try {
    let relayerAddress: string = '';
    if (argv.relayerPrivate) {
      const relayer = keyring.addFromUri(argv.relayerPrivate);
      relayerAddress = relayer.address;
      await api.tx.bfcStaking.joinCandidates(
        controller.address,
        relayerAddress,
        selfBond.toFixed(),
        1000,
      ).signAndSend(stash, { nonce: -1 });
    } else {
      await api.tx.bfcStaking.joinCandidates(
        controller.address,
        null,
        selfBond.toFixed(),
        1000,
      ).signAndSend(stash, { nonce: -1 });
    }

    console.log('\n👤 Joined Validator');
    console.log(`    controller: ${controller.address}`);
    console.log(`    stash: ${stash.address}`);
    if (relayerAddress) {
      console.log(`    relayer: ${relayerAddress}`);
    }
    console.log(`    self-bond: ${argv.bond}`);
  } catch (error) {
    if (error instanceof Error) {
      console.error(`Failed to join validators due to the following error: ${error.message}`);
    }
  }
}

join_validators().catch((error) => {
  console.error(error);
  process.exit(1);
});
