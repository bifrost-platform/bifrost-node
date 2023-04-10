import Web3 from 'web3';

async function query_extrinsic() {
  const yargs = require('yargs/yargs');
  const { hideBin } = require('yargs/helpers');
  const { ApiPromise, HttpProvider } = require('@polkadot/api');

  const argv = yargs(hideBin(process.argv))
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
        describe: 'The provider URL.',
        default: 'http://127.0.0.1:9933'
      },
    }).help().argv;

  if (!argv.block) {
    console.error('Please enter a valid `block` number');
    return;
  }

  if (!argv.index) {
    console.error('Please enter a valid `index` number');
    return;
  }

  const web3 = new Web3(argv.provider);
  try {
    const isSyncing = await web3.eth.isSyncing();
    if (isSyncing !== false) {
      console.error('Node is not completely synced yet');
      process.exit(-1);
    }
  } catch (e) {
    console.error('Node endpoint is not reachable');
    process.exit(-1);
  }

  const provider = new HttpProvider(argv.provider);
  const api = await ApiPromise.create({ provider });

  const blockHash = await api.rpc.chain.getBlockHash(argv.block);
  const signedBlock = await api.rpc.chain.getBlock(blockHash);

  if (signedBlock.block.extrinsics.length - 1 < argv.index) {
    console.error(`The given extrinsic index does not exist in block #${argv.block}`);
    return;
  }

  const rawXt = signedBlock.block.extrinsics[argv.index];
  const xt = rawXt.toHuman();

  const rawEvents = await api.query.system.events.at(blockHash);
  const events = rawEvents.toHuman();

  let matchedEvents = [];
  for (const e of events) {
    if (Number(e.phase.ApplyExtrinsic) === argv.index) {
      matchedEvents.push(e);
    }
  }

  console.log(`🔖 Extrinsic #${argv.block}-${argv.index} hash(${rawXt.hash.toHex()})`);
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
    for (const data of event.event.data) {
      console.log(`               ${JSON.stringify(data)}`);
    }
  }
}

query_extrinsic().catch((error) => {
  console.error(error);
  process.exit(0);
});
