import { signer, web3 } from './index';

export async function sendRequests(signedTx: string) {
  await web3.requestManager.send({ method: 'eth_sendRawTransaction', params: [signedTx] })
}

export async function singleTransfer(quantity: number, pk: string, value: string) {
  let nonce = Number(await web3.eth.getTransactionCount(signer));
  for (let i = 0; i < quantity; i++) {
    const signedTx = (await web3.eth.accounts.signTransaction({
      from: signer,
      to: '0xf24FF3a9CF04c71Dbc94D0b566f7A27B94566cac',
      gasPrice: web3.utils.toWei(1000, 'gwei'),
      gas: 21000,
      value,
      nonce
    }, pk)).rawTransaction;
    await sendRequests(signedTx);
    nonce += 1;
  }
}

export async function batchTransfer(quantity: number, pk: string, value: string) {
  const batch = [];
  let nonce = Number(await web3.eth.getTransactionCount(signer));

  for (let i = 0; i < quantity; i++) {
    const signedTx = (await web3.eth.accounts.signTransaction({
      from: signer,
      to: '0xf24FF3a9CF04c71Dbc94D0b566f7A27B94566cac',
      gasPrice: web3.utils.toWei(1000, 'gwei'),
      gas: 21000,
      value,
      nonce,
    }, pk)).rawTransaction;
    nonce += 1;
    batch.push(sendRequests(signedTx));
  }
  await Promise.all(batch);
}
