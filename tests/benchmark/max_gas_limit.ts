async function maxGasLimit() {
  const Web3 = require('web3');

  const PREFUND_ACCOUNTS: string[] = [
    // Baltithar
    '0x3Cd0A705a2DC65e5b1E1205896BaA2be8A07c6e0',
    // Charleth
    '0x798d4Ba9baf0064Ec19eB4F0a1a45785ae9D6DFc',
    // Dorothy
    '0x773539d4Ac0e786233D90A233654ccEE26a613D9',
    // Ethan
    '0xFf64d3F6efE2317EE2807d223a0Bdc4c0c49dfDB',
  ];

  const BOOT_ACCOUNT_PRIVATE_KEY: string = '0x5fb92d6e98884f76de468fa3f6278f8807c48bebc13595d45af5bdc4da702133';
  const LOCAL_NODE_ENDPOINT: string = 'http://127.0.0.1:9933';

  // send bfc to test accounts
  const provider = new Web3.providers.HttpProvider(LOCAL_NODE_ENDPOINT);
  const web3 = new Web3(provider);

  const owner = web3.eth.accounts.wallet.add(BOOT_ACCOUNT_PRIVATE_KEY);

  for (const account of PREFUND_ACCOUNTS) {
    await web3.eth.sendTransaction({
      from: owner.address,
      to: account,
      gas: 1500000,  // 14999999, 15000000
      value: web3.utils.toWei(web3.utils.toBN(1000000), 'ether'),
    }).on('transactionHash', (hash: string) => {
      console.log(`[*] send bfc to ${account} tx=${hash}`);
    });
  }
  console.log('[*] initialized account funds');
}
maxGasLimit();
