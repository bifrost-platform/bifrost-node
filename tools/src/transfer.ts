import Web3 from 'web3';

async function transfer() {
  const yargs = require('yargs/yargs');
  const { hideBin } = require('yargs/helpers');

  const argv = yargs(hideBin(process.argv))
    .usage('Usage: npm run transfer [args]')
    .version('1.0.0')
    .options({
      fromPrivate: {
        type: 'string', describe: 'The private key of the sender (with 0x prefix)'
      },
      to: {
        type: 'string', describe: 'The address of the receiver (with 0x prefix)'
      },
      provider: {
        type: 'string',
        describe: 'Provider endpoint',
        default: 'http://127.0.0.1:9933'
      },
      value: {
        type: 'string',
        describe: 'The amount to transfer in wei',
      }
    }).help().argv;

  if (!argv.fromPrivate) {
    console.error('Please enter a valid sender private key');
    return;
  }
  if (!argv.to) {
    console.error('Please enter a valid receiver address');
    return;
  }
  if (!argv.provider || !argv.provider.startsWith('http')) {
    console.error('Please enter a valid provider endpoint');
    return;
  }
  let Accounts = require('web3-eth-accounts');
  let accounts = new Accounts();
  try {
    accounts.privateKeyToAccount(argv.fromPrivate);
  } catch (err) {
    console.error('Please enter a valid sender private key');
    return;
  }

  const web3 = new Web3(argv.provider);
  if (!web3.utils.isAddress(argv.to)) {
    console.error('Please enter a valid receiver address');
    return;
  }
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

  try {
    const owner = web3.eth.accounts.wallet.add(argv.fromPrivate);
    await web3.eth.sendTransaction({
      from: owner.address,
      to: argv.to,
      gas: 21000,
      value: argv.value,
    }).on('transactionHash', (hash) => {
      console.log(`🎁 Successfully transferred → ${hash}`);
    });
  } catch (error) {
    if (error instanceof Error) {
      console.error(
        `Failed to transfer due to the following error: ${error.message}`);
    }
  }
}

transfer().catch((error) => {
  console.error(error);
  process.exit(0);
});
