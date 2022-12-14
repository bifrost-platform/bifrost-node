import { erc20abi } from './deployERC20';
import { fromAccount, web3 } from './index';
import { sendRequests } from './tx_traffic';

export default async function evmTraffic(reqCount: number, pk: string, contractAddress: string) {
  const erc20 = new web3.eth.Contract(erc20abi, contractAddress);
  let nonce = await web3.eth.getTransactionCount(fromAccount.address);

  const transferReqs = [];
  for (let i = 0; i < reqCount; i++) {
    const signedTx = (await web3.eth.accounts.signTransaction({
      from: fromAccount.address,
      to: contractAddress,
      value: 0,
      gas: 500000,
      data: erc20.methods.transfer('0xc62a8D60ec60A17E73813Fe289aE711A57356109', 100).encodeABI(),
      nonce
    }, pk)).rawTransaction;
    nonce += 1;
    if (signedTx) {
      transferReqs.push(sendRequests(signedTx));
    }
  }

  const transferResults = await Promise.all(transferReqs);

  const blockNums: Array<number> = [];
  transferResults.forEach(res => {
    const insertedNum = res.result.blockNumber;
    if (!blockNums.includes(insertedNum)) {
      blockNums.push(insertedNum);
    }
  });

  return blockNums;
}
