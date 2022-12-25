import { ethers } from 'ethers';

import { AccessListish } from '@ethersproject/transactions';

import { TEST_CONTROLLERS } from '../constants/keys';
import { customWeb3Request } from './providers';
import { INodeContext } from './set_dev_node';

const alith: { public: string, private: string } = TEST_CONTROLLERS[0];
const baltathar: { public: string, private: string } = TEST_CONTROLLERS[1];

export interface TransactionOptions {
  from?: string;
  to?: string;
  privateKey?: string;
  nonce?: number;
  gas?: string | number;
  gasPrice?: string | number;
  maxFeePerGas?: string | number;
  maxPriorityFeePerGas?: string | number;
  value?: string | number;
  data?: string;
  accessList?: AccessListish; // AccessList | Array<[string, Array<string>]>
}

export const TRANSACTION_TEMPLATE: TransactionOptions = {
  nonce: undefined,
  gas: 12_000_000,
  gasPrice: 1_000_000_000_000,
  value: "0x00",
};

export const ALITH_TRANSACTION_TEMPLATE: TransactionOptions = {
  ...TRANSACTION_TEMPLATE,
  from: alith.public,
  privateKey: alith.private,
};

export const BALTATHAR_TRANSACTION_TEMPLATE: TransactionOptions = {
  ...TRANSACTION_TEMPLATE,
  from: baltathar.public,
  privateKey: baltathar.private,
};

const GAS_PRICE = '0x' + (1_000_000_000_000).toString(16);
export async function callPrecompile(
  context: INodeContext,
  from: string,
  precompileContractAddress: string,
  selectors: { [key: string]: string },
  selector: string,
  parameters: string[]
) {
  let data: string;
  if (selectors[selector]) {
    data = `0x${selectors[selector]}`;
  } else {
    throw new Error(`selector doesn't exist on the precompile contract`);
  }
  parameters.forEach((para: string) => {
    data += para.slice(2).padStart(64, '0');
  });

  return await customWeb3Request(context.web3, 'eth_call', [
    {
      from,
      value: '0x0',
      gas: '0x10000',
      gasPrice: GAS_PRICE,
      to: precompileContractAddress,
      data,
    },
  ]);
}

// The parameters passed to the function are assumed to have all been converted to hexadecimal
export async function sendPrecompileTx(
  context: INodeContext,
  precompileContractAddress: string,
  selectors: { [key: string]: string },
  from: string,
  privateKey: string,
  selector: string,
  parameters: string[]
) {
  let data: string;
  if (selectors[selector]) {
    data = `0x${selectors[selector]}`;
  } else {
    throw new Error(`selector doesn't exist on the precompile contract`);
  }
  parameters.forEach((para: string) => {
    data += para.slice(2).padStart(64, "0");
  });

  const tx = await createTransaction(context, {
    from,
    privateKey,
    value: "0x0",
    gas: "0x200000",
    gasPrice: ALITH_TRANSACTION_TEMPLATE.gasPrice,
    to: precompileContractAddress,
    data,
  });

  return context.createBlock({
    transactions: [tx],
  });
}

export const createTransaction = async (
  context: INodeContext,
  options: TransactionOptions
): Promise<string> => {
  const isLegacy = context.ethTransactionType === 'Legacy';
  const isEip2930 = context.ethTransactionType === 'EIP2930';
  // const isEip1559 = context.ethTransactionType === 'EIP1559';

  const gas = options.gas || 12_000_000;
  const gasPrice = options.gasPrice !== undefined ? options.gasPrice : 1_000_000_000_000;
  const maxPriorityFeePerGas =
    options.maxPriorityFeePerGas !== undefined ? options.maxPriorityFeePerGas : 0;
  const value = options.value !== undefined ? options.value : '0x00';
  const from = options.from || alith.public;
  const privateKey = options.privateKey !== undefined ? options.privateKey : alith.private;

  const maxFeePerGas = options.maxFeePerGas || 1_000_000_000_000;
  const accessList = options.accessList || [];
  const nonce = options.nonce || await context.web3.eth.getTransactionCount(from, 'pending');

  let data, rawTransaction;
  if (isLegacy) {
    data = {
      from,
      to: options.to,
      value: value && value.toString(),
      gasPrice,
      gas,
      nonce: nonce,
      data: options.data,
    };
    const tx = await context.web3.eth.accounts.signTransaction(data, privateKey);
    rawTransaction = tx.rawTransaction;
  } else {
    const signer = new ethers.Wallet(privateKey, context.ethers);
    const chainId = await context.web3.eth.getChainId();
    if (isEip2930) {
      data = {
        from,
        to: options.to,
        value: value && value.toString(),
        gasPrice,
        gasLimit: gas,
        nonce: nonce,
        data: options.data,
        accessList,
        chainId,
        type: 1,
      };
    } else {  // EIP1559
      data = {
        from,
        to: options.to,
        value: value && value.toString(),
        maxFeePerGas,
        maxPriorityFeePerGas,
        gasLimit: gas,
        nonce: nonce,
        data: options.data,
        accessList,
        chainId,
        type: 2,
      };
    }
    rawTransaction = await signer.signTransaction(data);
  }

  return rawTransaction!;
};
