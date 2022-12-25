import { ERC20ABI, ERC20BYTECODE } from '../constants/ERC20';
import { web3 } from './index';

export const erc20abi = JSON.parse(ERC20ABI);

export default async function deployERC20(pk: string) {
  const deployTx = (new web3.eth.Contract(erc20abi)).deploy({
    data: ERC20BYTECODE,
    arguments: [1, 1]
  });

  const signedTx = (await web3.eth.accounts.signTransaction({
    data: deployTx.encodeABI(),
    gas: 3000000
  }, pk)).rawTransaction;

  if (signedTx) {
    const deployReceipt = await web3.eth.sendSignedTransaction(signedTx);
    console.log(`[*] ERC20 deployed: ${deployReceipt.contractAddress}`);
    return deployReceipt.contractAddress;
  }
}
