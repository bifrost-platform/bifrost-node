import { create as web3AccountCreate } from 'web3-eth-accounts';
import yargs from 'yargs';
import { hideBin } from 'yargs/helpers';

async function create_accounts() {
  const argv = await yargs(hideBin(process.argv))
    .usage('Usage: npm run create_accounts [args]')
    .version('1.0.0')
    .options({
      full: {
        type: 'boolean',
        describe: 'Full node tier required. Relayer account will be returned if true',
        default: false,
      },
    }).help().argv;

  const controller = web3AccountCreate();
  const stash = web3AccountCreate();

  console.log(`👤 Controller:`);
  console.log(`    address: ${controller.address}`);
  console.log(`    privateKey: ${controller.privateKey}`);
  console.log('👤 Stash:');
  console.log(`    address: ${stash.address}`);
  console.log(`    privateKey: ${stash.privateKey}`);

  if (argv.full) {
    const relayer = web3AccountCreate();
    console.log('👤 Relayer:');
    console.log(`    address: ${relayer.address}`);
    console.log(`    privateKey: ${relayer.privateKey}`);
  }
}

create_accounts().catch(error => {
  console.error(error);
  process.exit(1);
});
