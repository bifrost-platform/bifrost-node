const Web3 = require('web3');

async function create_accounts() {
  const web3 = new Web3();
  const yargs = require('yargs/yargs');
  const { hideBin } = require('yargs/helpers');

  const argv = yargs(hideBin(process.argv))
    .usage('Usage: npm run create_accounts [args]')
    .version('1.0.0')
    .options({
      full: {
        type: 'boolean',
        describe: 'Full node tier required. Relayer account will be returned if true',
        default: false,
      },
    }).help().argv;

  const controller = web3.eth.accounts.create();
  const stash = web3.eth.accounts.create();

  console.log(`👤 Controller:`);
  console.log(`    address: ${controller.address}`);
  console.log(`    privateKey: ${controller.privateKey}`);
  console.log('👤 Stash:');
  console.log(`    address: ${stash.address}`);
  console.log(`    privateKey: ${stash.privateKey}`);

  if (argv.full) {
    const relayer = web3.eth.accounts.create();
    console.log('👤 Relayer:');
    console.log(`    address: ${relayer.address}`);
    console.log(`    privateKey: ${relayer.privateKey}`);
  }
}

create_accounts().catch(error => {
  console.error(error);
  process.exit(1);
});
