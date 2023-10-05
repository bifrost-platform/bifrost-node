import { signer, web3 } from './index';
import { sendRequests } from './tx_traffic';
import { ERC20_ABI } from '../constants/ERC20';

export default async function evmTraffic(reqCount: number, pk: string, contractAddress: string) {
  const erc20: any = new web3.eth.Contract(ERC20_ABI, contractAddress);
  let nonce = Number(await web3.eth.getTransactionCount(signer));

  const transferReqs = [];
  for (let i = 0; i < reqCount; i++) {
    const signedTx = (await web3.eth.accounts.signTransaction({
      from: signer,
      to: contractAddress,
      value: 0,
      gas: 500000,
      gasPrice: web3.utils.toWei(1000, 'gwei'),
      data: erc20.methods.transfer('0xc62a8D60ec60A17E73813Fe289aE711A57356109', 100).encodeABI(),
      nonce
    }, pk)).rawTransaction;
    nonce += 1;
    if (signedTx) {
      transferReqs.push(sendRequests(signedTx));
    }
  }

  await Promise.all(transferReqs);
}
