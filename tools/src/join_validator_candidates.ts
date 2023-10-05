import { BigNumber } from 'bignumber.js';
import { Web3 } from 'web3';
import { privateKeyToAccount } from 'web3-eth-accounts';
import yargs from 'yargs';
import { hideBin } from 'yargs/helpers';

import { ApiPromise, WsProvider } from '@polkadot/api';
import { Keyring } from '@polkadot/keyring';

async function join_validator_candidates() {
  const argv = await yargs(hideBin(process.argv))
    .usage('Usage: npm run join_validator_candidates [args]')
    .version('1.0.0')
    .options({
      controllerPrivate: {
        type: 'string', describe: 'Controller\'s PrivateKey (with 0x prefix)'
      },
      stashPrivate: {
        type: 'string', describe: 'Stash\'s PrivateKey (with 0x prefix)'
      },
      relayerPrivate: {
        type: 'string',
        describe: 'Relayer\'s PrivateKey (with 0x prefix)',
        default: ''
      },
      provider: {
        type: 'string',
        describe: 'Provider endpoint. WebSocket provider is required.',
        default: 'ws://127.0.0.1:9944'
      },
      bond: {
        type: 'number',
        describe: 'Initial self-bond amount in decimal',
        default: 1000
      }
    }).help().argv;

  if (!argv.controllerPrivate) {
    console.error('⚠️  Please enter a valid controller private key.');
    return;
  }
  if (!argv.stashPrivate) {
    console.error('⚠️  Please enter a valid stash private key.');
    return;
  }
  if (!argv.provider || !argv.provider.startsWith('ws')) {
    console.error('⚠️  Please enter a valid provider. WebSocket provider is required.');
    return;
  }
  try {
    privateKeyToAccount(argv.controllerPrivate);
  } catch (err) {
    console.error('⚠️  Please enter a valid controller private key.');
    return;
  }
  try {
    privateKeyToAccount(argv.stashPrivate);
  } catch (err) {
    console.error('⚠️  Please enter a valid stash private key.');
    return;
  }
  if (argv.relayerPrivate) {
    try {
      privateKeyToAccount(argv.relayerPrivate);
    } catch (err) {
      console.error('⚠️  Please enter a valid relayer private key.');
      return;
    }
  }

  const web3 = new Web3(new Web3.providers.WebsocketProvider(argv.provider));
  try {
    const isSyncing = await web3.eth.isSyncing();
    if (isSyncing !== false) {
      console.error('⚠️  Node is not completely synced yet.');
      process.exit(1);
    }
  } catch (error) {
    console.error('⚠️  Node endpoint is not reachable.');
    process.exit(1);
  }

  const keyring = new Keyring({ type: 'ethereum' });
  const controller = keyring.addFromUri(argv.controllerPrivate);
  const stash = keyring.addFromUri(argv.stashPrivate);
  const provider = new WsProvider(argv.provider);
  const api = await ApiPromise.create({ provider, noInitWarn: true });

  const AMOUNT_FACTOR = 10 ** 18;
  const selfBond = new BigNumber(argv.bond).multipliedBy(AMOUNT_FACTOR);

  try {
    let relayerAddress: string | null = null;
    if (argv.relayerPrivate) {
      const relayer = keyring.addFromUri(argv.relayerPrivate);
      relayerAddress = relayer.address;
    }
    const unsub = await api.tx.bfcStaking.joinCandidates(
      controller.address,
      relayerAddress,
      selfBond.toFixed(),
      1000
    ).signAndSend(stash, { nonce: -1 }, ({ events = [], status, txHash, dispatchError }) => {
      console.log(`💤 The requested extrinsic status is "${status.type}"`);

      if (status.isFinalized) {
        console.log(`🎁 Extrinsic included at blockHash ${status.asFinalized}`);
        console.log(`🔖 Extrinsic hash ${txHash.toHex()}`);

        // Loop through Vec<EventRecord> to display all events
        events.forEach(({ event: { data: [error, info], method, section } }) => {
          if (section === 'system' && method === 'ExtrinsicSuccess') {
            console.log('\n👤 Successfully joined as a validator candidate');
            console.log(`    controller: ${controller.address}`);
            console.log(`    stash: ${stash.address}`);
            if (relayerAddress) {
              console.log(`    relayer: ${relayerAddress}`);
            }
            console.log(`    self-bond: ${argv.bond}`);
            process.exit(0);
          } else if (section === 'system' && method === 'ExtrinsicFailed') {
            if (dispatchError) {
              if (dispatchError.isModule) {
                // for module errors, we have the section indexed, lookup
                const decoded = api.registry.findMetaError(dispatchError.asModule);
                const { docs, name, section } = decoded;

                console.error(`⚠️  Failed to join as a validator candidate due to an unknown error: ${section}.${name}: ${docs.join(' ')}`);
              } else {
                // Other, CannotLookup, BadOrigin, no extra info
                console.error(`⚠️  Failed to join as a validator candidate due to an unknown error: ${dispatchError.toString()}`);
              }
            }
            process.exit(1);
          }
        });
        unsub();
      }
    });
  } catch (error) {
    if (error instanceof Error) {
      console.error(
        `⚠️  Failed to join validators due to the following error: ${error.message}`);
    }
  }
}

join_validator_candidates().catch((error) => {
  console.error(error);
  process.exit(1);
});
