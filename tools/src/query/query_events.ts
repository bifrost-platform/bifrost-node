import Gauge from 'gauge';
import { Web3 } from 'web3';
import yargs from 'yargs';
import { hideBin } from 'yargs/helpers';

import { ApiPromise, WsProvider } from '@polkadot/api';

async function query_events() {
  const argv = await yargs(hideBin(process.argv))
    .usage('Usage: npm run query_events [args]')
    .version('1.0.0')
    .options({
      start: {
        type: 'number',
        describe: 'The starting block number where to start the query.'
      },
      end: {
        type: 'number',
        describe: 'The ending block number where to end the query. The default value will be the highest block of the connected provider.'
      },
      pallet: {
        type: 'string',
        describe: 'The name of the pallet where the event locates.'
      },
      event: {
        type: 'string',
        describe: 'The name of the event to query.'
      },
      provider: {
        type: 'string',
        describe: 'The provider endpoint. WebSocket provider is required.',
        default: 'ws://127.0.0.1:9944'
      },
    }).help().argv;

  if (!argv.start) {
    console.error('⚠️  Please enter a valid `start` block number');
    return;
  }
  if (!argv.pallet) {
    console.error('⚠️  Please enter a valid `pallet` name');
    return;
  }
  if (!argv.event) {
    console.error('⚠️  Please enter a valid `event` name');
    return;
  }
  if (!argv.provider || !argv.provider.startsWith('ws')) {
    console.error('⚠️  Please enter a valid provider. WebSocket provider is required.');
    return;
  }
  let pallet = argv.pallet.toLowerCase();
  let event = argv.event.toLowerCase();

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

  let endHeader;
  let endNumber;
  let startNumber = argv.start;
  if (argv.end) {
    endNumber = argv.end;
  } else {
    endHeader = await api.rpc.chain.getHeader();
    endNumber = endHeader.number.toNumber();
  }
  if (endNumber < startNumber) {
    console.error('⚠️  The requested `start` block number is higher than `end` block. Must be `start` < `end`.');
    process.exit(1);
  }

  const gauge = new Gauge();

  let results = [];
  let currentProcess = 0;
  let singleProcessRate = 100 / (endNumber - startNumber) / 100;

  while (startNumber <= endNumber) {
    const blockHash = await api.rpc.chain.getBlockHash(startNumber);
    const atSubstrate = await api.at(blockHash);
    const rawEvents: any = await atSubstrate.query.system.events();
    const events = rawEvents.toHuman();

    let matchedEvents = [];
    for (const e of events) {
      const method = e.event.method.toLowerCase();
      const section = e.event.section.toLowerCase();
      if (section === pallet && method === event) {
        const index = Number(e.phase.ApplyExtrinsic);
        const signedBlock = await api.rpc.chain.getBlock(blockHash);
        const xt = signedBlock.block.extrinsics[index];
        matchedEvents.push({ index, hash: xt.hash.toHex() });
      }
    }

    if (matchedEvents.length) {
      results.push({ block: startNumber, extrinsics: matchedEvents });
    }

    gauge.pulse(); gauge.show(`Querying events(${startNumber}…${endNumber})`, currentProcess);
    currentProcess += singleProcessRate;
    startNumber += 1;
  }
  gauge.hide();

  if (!results.length) {
    console.log('✨ Matching events not found.');
    process.exit(0);
  }

  for (const result of results) {
    console.log(`✨ Found events in block #${result.block}`);
    for (const xt of result.extrinsics) {
      console.log(`     🔖 Event emitted at extrinsic #${result.block}-${xt.index} hash(${xt.hash})`);
    }
    console.log();
  }
  process.exit(0);
}

query_events().catch((error) => {
  console.error(error);
  process.exit(1);
});
