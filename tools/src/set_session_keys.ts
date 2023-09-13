import { Web3 } from 'web3';
import { privateKeyToAccount } from 'web3-eth-accounts';
import yargs from 'yargs';
import { hideBin } from 'yargs/helpers';

import { ApiPromise, WsProvider } from '@polkadot/api';
import { Keyring } from '@polkadot/keyring';

async function set_session_keys() {
  const argv = await yargs(hideBin(process.argv))
    .usage('Usage: npm run set_session_keys [args]')
    .version('1.0.0')
    .options({
      controllerPrivate: {
        type: 'string', describe: 'Controller\'s PrivateKey (with 0x prefix)'
      },
      provider: {
        type: 'string',
        describe: 'Provider endpoint. WebSocket provider is required.',
        default: 'ws://127.0.0.1:9944'
      },
    }).help().argv;

  if (!argv.controllerPrivate) {
    console.error('⚠️  Please enter a valid controller private key.');
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

  const web3 = new Web3(new Web3.providers.WebsocketProvider(argv.provider));
  try {
    const isSyncing = await web3.eth.isSyncing();
    if (isSyncing !== false) {
      console.error('⚠️  Node is not completely synced yet.');
      process.exit(1);
    }
  } catch (e) {
    console.error('⚠️  Node endpoint is not reachable.');
    process.exit(1);
  }

  const keyring = new Keyring({ type: 'ethereum' });
  const controller = keyring.addFromUri(argv.controllerPrivate);
  const provider = new WsProvider(argv.provider);
  const api = await ApiPromise.create({ provider, noInitWarn: true });

  try {
    // rotate session keys. this will generate a new session key pair inside your nodes chain data directory
    const sessionKeys = (await api.rpc.author.rotateKeys()).toHex().slice(2);

    const auraSessionKey = `0x${sessionKeys.slice(0, 64)}`;
    const granSessionKey = `0x${sessionKeys.slice(64, 128)}`;
    const imonSessionKey = `0x${sessionKeys.slice(128)}`;

    const unsub = await api.tx.session.setKeys({
      aura: auraSessionKey,
      grandpa: granSessionKey,
      imOnline: imonSessionKey
    }, '0x00').signAndSend(controller, { nonce: -1 }, ({ events = [], status, txHash, dispatchError }) => {
      console.log(`💤 The requested extrinsic status is "${status.type}"`);

      if (status.isFinalized) {
        console.log(`🎁 Extrinsic included at blockHash ${status.asFinalized}`);
        console.log(`🔖 Extrinsic hash ${txHash.toHex()}`);

        // Loop through Vec<EventRecord> to display all events
        events.forEach(({ event: { data: [error, info], method, section } }) => {
          if (section === 'system' && method === 'ExtrinsicSuccess') {
            console.log('\n🔑 Successfully setted your session keys');
            console.log(`    aura: ${auraSessionKey}`);
            console.log(`    gran: ${granSessionKey}`);
            console.log(`    imon: ${imonSessionKey}`);
            process.exit(0);
          } else if (section === 'system' && method === 'ExtrinsicFailed') {
            if (dispatchError) {
              if (dispatchError.isModule) {
                // for module errors, we have the section indexed, lookup
                const decoded = api.registry.findMetaError(dispatchError.asModule);
                const { docs, name, section } = decoded;

                console.error(`⚠️  Failed to set your session keys due to an unknown error: ${section}.${name}: ${docs.join(' ')}`);
              } else {
                // Other, CannotLookup, BadOrigin, no extra info
                console.error(`⚠️  Failed to set your session keys due to an unknown error: ${dispatchError.toString()}`);
              }
            }
          }
        });
        unsub();
      }
    });
  } catch (error) {
    if (error instanceof Error) {
      console.error(
        `⚠️  Failed to set your session keys due to the following error: ${error.message}`);
    }
  }
}

set_session_keys().catch((error) => {
  console.error(error);
  process.exit(1);
});
