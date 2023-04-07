import Web3 from 'web3';

async function query_extrinsics() {
  const Gauge = require('gauge');
  const yargs = require('yargs/yargs');
  const { hideBin } = require('yargs/helpers');
  const { ApiPromise, HttpProvider } = require('@polkadot/api');

  const argv = yargs(hideBin(process.argv))
    .usage('Usage: npm run query_extrinsics [args]')
    .version('1.0.0')
    .options({
      from: {
        type: 'string',
        describe: 'The address who signed the extrinsic to query (with 0x prefix). This field is optional.'
      },
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
        describe: 'The name of the pallet where the extrinsic locates.'
      },
      extrinsic: {
        type: 'string',
        describe: 'The name of the extrinsic to query.'
      },
      provider: {
        type: 'string',
        describe: 'The provider URL.',
        default: 'http://127.0.0.1:9933'
      },
    }).help().argv;

  if (argv.from && !Web3.utils.isAddress(argv.from)) {
    console.error('Please enter a valid `from` address');
    return;
  }
  let from = argv.from;

  if (!argv.start) {
    console.error('Please enter a valid `start` block number');
    return;
  }

  if (!argv.pallet) {
    console.error('Please enter a valid `pallet` name');
    return;
  }
  if (!argv.extrinsic) {
    console.error('Please enter a valid `extrinsic` name');
    return;
  }
  let pallet = argv.pallet.toLowerCase();
  let extrinsic = argv.extrinsic.toLowerCase();

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
    const signedBlock = await api.rpc.chain.getBlock(blockHash);

    let matchedExtrinsics = [];
    for (const rawXt of signedBlock.block.extrinsics) {
      const xt = rawXt.toHuman();
      const method = xt.method.method.toLowerCase();
      const section = xt.method.section.toLowerCase();

      if (from && xt.signer) {
        from = from.toLowerCase();
        const signer = xt.signer.toLowerCase();
        if (signer === from && section === pallet && method === extrinsic) {
          matchedExtrinsics.push(rawXt.hash.toHex());
        }
        continue;
      }

      if (section === pallet && method === extrinsic) {
        matchedExtrinsics.push(rawXt.hash.toHex());
      }
    }

    if (matchedExtrinsics.length) {
      results.push({ block: startNumber, extrinsics: matchedExtrinsics });
    }

    gauge.pulse(); gauge.show(`Querying extrinsics(${startNumber}…${endNumber})`, currentProcess);
    currentProcess += singleProcessRate;
    startNumber += 1;
  }
  gauge.hide();

  if (!results.length) {
    console.log('✨ Matching extrinsics not found.');
    return;
  }

  for (const result of results) {
    console.log(`✨ Found extrinsics in block #${result.block}`);
    for (const xt of result.extrinsics) {
      console.log(`     🔖 Extrinsic hash(${xt})`);
    }
    console.log();
  }
}

query_extrinsics().catch((error) => {
  console.error(error);
  process.exit(0);
});
