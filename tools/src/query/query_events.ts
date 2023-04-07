import Web3 from 'web3';

async function query_events() {
  const Gauge = require('gauge');
  const yargs = require('yargs/yargs');
  const { hideBin } = require('yargs/helpers');
  const { ApiPromise, HttpProvider } = require('@polkadot/api');

  const argv = yargs(hideBin(process.argv))
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
        describe: 'The provider URL.',
        default: 'http://127.0.0.1:9933'
      },
    }).help().argv;

  if (!argv.start) {
    console.error('Please enter a valid `start` block number');
    return;
  }

  if (!argv.pallet) {
    console.error('Please enter a valid `pallet` name');
    return;
  }
  if (!argv.event) {
    console.error('Please enter a valid `event` name');
    return;
  }
  let pallet = argv.pallet.toLowerCase();
  let event = argv.event.toLowerCase();

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

  let endHeader;
  let endNumber;
  let startNumber = argv.start;
  if (argv.end) {
    endNumber = argv.end;
  } else {
    endHeader = await api.rpc.chain.getHeader();
    endNumber = endHeader.number;
  }
  if (endNumber < startNumber) {
    console.error('The requested `start` block number is higher than `end` block. Must be `start` < `end`.');
    return;
  }

  const gauge = new Gauge();

  let results = [];
  let currentProcess = 0;
  let singleProcessRate = 100 / (endNumber - startNumber) / 100;

  while (startNumber <= endNumber) {
    const blockHash = await api.rpc.chain.getBlockHash(startNumber);
    const rawEvents = await api.query.system.events.at(blockHash);
    const events = rawEvents.toHuman();

    let matchedEvents = [];
    for (const e of events) {
      const method = e.event.method.toLowerCase();
      const section = e.event.section.toLowerCase();
      if (section === pallet && method === event) {
        const index = Number(e.phase.ApplyExtrinsic);
        const signedBlock = await api.rpc.chain.getBlock(blockHash);
        const xt = signedBlock.block.extrinsics[index];
        matchedEvents.push(xt.hash.toHex());
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
    return;
  }

  for (const result of results) {
    console.log(`✨ Found events in block #${result.block}`);
    for (const xt of result.extrinsics) {
      console.log(`     🔖 Event emitted extrinsic hash(${xt})`);
    }
    console.log();
  }
}

query_events().catch((error) => {
  console.error(error);
  process.exit(0);
});
