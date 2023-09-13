import { Web3 } from 'web3';
import yargs from 'yargs';
import { hideBin } from 'yargs/helpers';

import { ApiPromise, WsProvider } from '@polkadot/api';

async function query_extrinsic() {
  const argv = await yargs(hideBin(process.argv))
    .usage('Usage: npm run query_extrinsic [args]')
    .version('1.0.0')
    .options({
      block: {
        type: 'number',
        describe: 'The block number where the extrinsic is included.'
      },
      index: {
        type: 'number',
        describe: 'The index of the extrinsic.'
      },
      provider: {
        type: 'string',
        describe: 'The provider endpoint. WebSocket provider is required.',
        default: 'ws://127.0.0.1:9944'
      },
    }).help().argv;

  if (!argv.block) {
    console.error('⚠️  Please enter a valid `block` number');
    return;
  }
  if (!argv.index) {
    console.error('⚠️  Please enter a valid `index` number');
    return;
  }
  if (!argv.provider || !argv.provider.startsWith('ws')) {
    console.error('⚠️  Please enter a valid provider. WebSocket provider is required.');
    return;
  }

  const web3 = new Web3(new Web3.providers.WebsocketProvider(argv.provider));
  try {
    const isSyncing = await web3.eth.isSyncing();
    if (isSyncing !== false) {
      console.error('⚠️  Node is not completely synced yet');
      process.exit(1);
    }
  } catch (e) {
    console.error('⚠️  Node endpoint is not reachable');
    process.exit(1);
  }

  const provider = new WsProvider(argv.provider);
  const api = await ApiPromise.create({ provider, noInitWarn: true });

  const blockHash = await api.rpc.chain.getBlockHash(argv.block);
  const signedBlock = await api.rpc.chain.getBlock(blockHash);

  if (signedBlock.block.extrinsics.length - 1 < argv.index) {
    console.error(`⚠️  The given extrinsic index does not exist in block #${argv.block}`);
    process.exit(1);
  }

  const xt = signedBlock.block.extrinsics[argv.index];

  const atSubstrate = await api.at(blockHash);
  const rawEvents: any = await atSubstrate.query.system.events();
  const events = rawEvents.toHuman();

  let matchedEvents = [];
  for (const e of events) {
    if (Number(e.phase.ApplyExtrinsic) === argv.index) {
      matchedEvents.push(e);
    }
  }

  console.log(`🔖 Extrinsic #${argv.block}-${argv.index} hash(${xt.hash.toHex()})`);
  console.log(`     Pallet: ${xt.method.section}`);
  console.log(`     Extrinsic: ${xt.method.method}`);

  if (xt.signer) {
    console.log(`     Signer: ${xt.signer}`);
  }

  console.log(`     Arguments:`);
  const keys: string[] = Object.keys(xt.method.args);
  for (const key of keys) {
    console.log(`         ${key}: ${xt.method.args[key]}`);
  }

  console.log(`     Events:`);
  for (const [index, event] of matchedEvents.entries()) {
    console.log(`       #${index}`);
    console.log(`           Pallet: ${event.event.section}`);
    console.log(`           Event: ${event.event.method}`);
    console.log(`           Data:`);
    console.log(`               ${JSON.stringify(event.event.data)}`);
  }
  process.exit(0);
}

query_extrinsic().catch((error) => {
  console.error(error);
  process.exit(1);
});
