import BigNumber from 'bignumber.js';
import Web3 from 'web3';
import { privateKeyToAccount } from 'web3-eth-accounts';
import yargs from 'yargs';
import { hideBin } from 'yargs/helpers';

import { ApiPromise, HttpProvider, Keyring, WsProvider } from '@polkadot/api';

import { batchTransfer, singleTransfer } from './tx_traffic';

export let web3: Web3;
export let api: ApiPromise;
export let signer: string;

async function main() {
  const argv = await yargs(hideBin(process.argv))
      .usage('Usage: npm run benchmark -- <command> [args]')
      .version('1.0.0')
      .options({
        provider: {
          type: 'string',
          describe: 'The provider endpoint. Http and WebSocket is both allowed.',
          default: 'http://127.0.0.1:9944'
        },
        pk: {
          type: 'string',
          describe: 'The private key of the sender (with 0x prefix)'
        },
        quantity: {
          type: 'number',
          describe: 'The total number of transactions to be sent.',
          default: 1
        },
        isEvm: {
          type: 'boolean',
          describe: 'The flag that represents whether the transactions will be sent in an EVM form. If set to false, it will be sent as a Substrate form.',
          default: false
        },
        isBatch: {
          type: 'boolean',
          describe: 'The flag that represents whether the transactions will be sent in a batch form. If set to false, it will be sent individually.',
          default: false
        }
      }).help().argv;

  if (!argv.pk) {
    console.error('⚠️  Please enter a valid sender private key.');
    process.exit(1);
  }
  if (!argv.provider || (!argv.provider.startsWith('http') && !argv.provider.startsWith('ws'))) {
    console.error('⚠️  Please enter a valid provider endpoint.');
    process.exit(1);
  }
  try {
    privateKeyToAccount(argv.pk);
  } catch (err) {
    console.error('⚠️  Please enter a valid sender private key.');
    process.exit(1);
  }
  if (argv.quantity < 1) {
    console.error('⚠️  Please enter a positive quantity.');
    process.exit(1);
  }

  if (argv.provider.startsWith('http')) {
    web3 = new Web3(new Web3.providers.HttpProvider(argv.provider));
    api = await ApiPromise.create({ provider: new HttpProvider(argv.provider), noInitWarn: true });
  } else {
    web3 = new Web3(new Web3.providers.WebsocketProvider(argv.provider));
    api = await ApiPromise.create({ provider: new WsProvider(argv.provider), noInitWarn: true });
  }
  signer = web3.eth.accounts.wallet.add(argv.pk)[0].address;

  try {
    const isSyncing = await web3.eth.isSyncing();
    if (isSyncing) {
      console.error('⚠️  Node is not completely synced yet. Please use a fully synced node.');
      process.exit(1);
    }
  } catch (error) {
    if (error instanceof Error) {
      console.error(`⚠️  The given node endpoint is not reachable: ${error.message}`);
    }
    process.exit(1);
  }

  const amount = new BigNumber(10 ** 9).toFixed();

  // benchmark by substrate
  if (!argv.isEvm) {
    const keyring = new Keyring({ type: 'ethereum' });
    const account = keyring.addFromUri(argv.pk);

    if (argv.isBatch) {
      const extrinsics = [];
      for (let idx = 0; idx < argv.quantity; idx++) {
        extrinsics.push(api.tx.balances.transfer('0xf24FF3a9CF04c71Dbc94D0b566f7A27B94566cac', amount));
      }
      await api.tx.utility.batch(extrinsics).signAndSend(account, { nonce: -1 }).catch(error => {
        console.error(error.message);
        process.exit(1);
      });
    } else {
      for (let idx = 0; idx < argv.quantity; idx++) {
        if (idx % 2 === 0) {
          await api.tx.balances
              .transferKeepAlive('0xf24FF3a9CF04c71Dbc94D0b566f7A27B94566cac', amount)
              .signAndSend(account, { nonce: -1 });
        } else {
          await api.tx.balances
              .transfer('0xf24FF3a9CF04c71Dbc94D0b566f7A27B94566cac', amount)
              .signAndSend(account, { nonce: -1 });
        }
      }
    }
    process.exit(0);
  }

  // benchmark by evm
  if (argv.isBatch) {
    await batchTransfer(argv.quantity, argv.pk, amount);
  } else {
    await singleTransfer(argv.quantity, argv.pk, amount);
  }
  process.exit(0);
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
