import BigNumber from 'bignumber.js';
import Web3 from 'web3';
import { Account } from 'web3-core';

import { ApiPromise, HttpProvider, Keyring, WsProvider } from '@polkadot/api';

import deployERC20 from './deployERC20';
import evmTraffic from './evm_traffic';
import { batchTransfer, singleTransfer } from './tx_traffic';

const yargs = require('yargs/yargs');
const { hideBin } = require('yargs/helpers');

export const web3 = new Web3();
export let fromAccount: Account;

async function main() {
  const argv = yargs(hideBin(process.argv))
    .usage('Usage: npm run benchmark -- <command> [args]')
    .version('1.0.0')
    .options({
      provider: {
        type: 'string', default: undefined
      },
      worker: {
        type: 'number', describe: 'Number of workers', default: 1
      },
      pk: {
        type: 'string', describe: 'Testnet Holder\'s private key'
      },
      batchQuantity: {
        type: 'number',
        describe: 'Tx batch amount. (If 0, send single tx every 0.01s continuously)',
        default: 0
      },
      evm: {
        type: 'boolean', default: false,
      },
      single: {
        type: 'boolean', default: false,
      },
    }).help().argv;

  web3.setProvider(argv.provider);
  fromAccount = web3.eth.accounts.privateKeyToAccount(argv.pk);

  if (!argv.evm) {
    const keyring = new Keyring({ type: 'ethereum' });
    const account = keyring.addFromUri(argv.pk);
    let provider;
    if (argv.provider.startsWith('http')) {
      provider = new HttpProvider(argv.provider);
    } else if (argv.provider.startsWith('ws')) {
      provider = new WsProvider(argv.provider);
    } else {
      console.error(`wrong provider received: ${argv.provider}`);
      return;
    }
    const api = await ApiPromise.create({ provider });

    const amount = new BigNumber(10 ** 9).toFixed();

    // const accounts = [];

    // for (let idx = 0; idx < Number(argv.batchQuantity); idx++) {
    //   const amt = new BigNumber(10 ** 18).toFixed();
    //   const a = web3.eth.accounts.create();
    //   await api.tx.balances
    //     .transferKeepAlive(a.address, amt)
    //     .signAndSend(account, { nonce: -1 });
    //   accounts.push(keyring.addFromUri(a.privateKey));
    // }

    if (argv.batchQuantity) {
      if (argv.single) {
        let nonce = await web3.eth.getTransactionCount(account.address);
        for (let idx = 0; idx < Number(argv.batchQuantity); idx++) {
          if (idx % 2 === 0) {
            await api.tx.balances
              .transferKeepAlive('0xf24FF3a9CF04c71Dbc94D0b566f7A27B94566cac', amount)
              .signAndSend(account, { nonce });
          } else {
            await api.tx.balances
              .transfer('0xf24FF3a9CF04c71Dbc94D0b566f7A27B94566cac', amount)
              .signAndSend(account, { nonce });
          }
          nonce += 1;
        }
        console.timeEnd('Substrate Tx');
      } else {
        const extrinsics = [];
        for (let idx = 0; idx < Number(argv.batchQuantity); idx++) {
          extrinsics.push(api.tx.balances.transfer('0xf24FF3a9CF04c71Dbc94D0b566f7A27B94566cac', amount));
        }
        await api.tx.utility.batch(extrinsics).signAndSend(account, { nonce: -1 });
      }
    } else {
      await api.tx.balances.transfer('0xf24FF3a9CF04c71Dbc94D0b566f7A27B94566cac', amount).signAndSend(account, { nonce: -1 });
    }
    return;
  }

  let txLoadResult = [0];
  const _transferStartTime = (new Date()).getTime();
  if (argv.batchQuantity) {
    txLoadResult = await batchTransfer(argv.batchQuantity, argv.pk);
  } else {
    let nonce = await web3.eth.getTransactionCount(fromAccount.address);
    while (1) {
      singleTransfer(nonce, argv.pk);
      nonce += 1;
      await new Promise(r => setTimeout(r, 10));
    }
  }
  const _transferEndTime = (new Date()).getTime();
  console.log(`[*] ${argv.batchQuantity} transfer transaction mined in ${_transferEndTime - _transferStartTime} ms. Inserted in ${txLoadResult.length} blocks ${JSON.stringify(txLoadResult)}`);

  if (argv.evm) {
    console.log(`[*] Start EVM interaction test`);

    const erc20address = await deployERC20(argv.pk);
    let evmLoadResult;
    const _evmStartTime = (new Date()).getTime();
    if (erc20address) {
      evmLoadResult = await evmTraffic(argv.batchQuantity, argv.pk, erc20address);
    }
    const _evmEndTime = (new Date()).getTime();
    console.log(`[*] ${argv.batchQuantity} EVM interactions mined in ${_evmEndTime - _evmStartTime} ms. Inserted in blocks ${JSON.stringify(evmLoadResult)}`);
  }
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
