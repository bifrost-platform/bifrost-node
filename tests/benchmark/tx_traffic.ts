import { fromAccount, web3 } from './index';

export async function sendRequests(signedTx: string) {
  const _startTime = (new Date()).getTime();
  const result = await web3.eth.sendSignedTransaction(signedTx);
  const _endTime = (new Date()).getTime();
  return { delay: (_endTime - _startTime), result };
}

export async function singleTransfer(nonce: number, pk: string) {
  const signedTx = (await web3.eth.accounts.signTransaction({
    to: '0xc62a8D60ec60A17E73813Fe289aE711A57356109',
    value: 1000000000000000,
    gas: 30000,
    nonce
  }, pk)).rawTransaction;
  if (signedTx) {
    const receipt = await sendRequests(signedTx);
    console.log(`transaction sent: ${receipt.result.transactionHash} - ${nonce}`);
  }
}

export async function batchTransfer(reqCount: number, pk: string) {
  let nonce = await web3.eth.getTransactionCount(fromAccount.address);
  const transferReqs = [];

  for (let i = 0; i < reqCount; i++) {
    const signedTx = (await web3.eth.accounts.signTransaction({
      to: '0xc62a8D60ec60A17E73813Fe289aE711A57356109',
      value: 1000000000000000,
      gas: 30000,
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
    const insertedBlockNum = res.result.blockNumber;
    if (!(blockNums.includes(insertedBlockNum))) {
      blockNums.push(insertedBlockNum);
    }
  });

  return blockNums;
}
