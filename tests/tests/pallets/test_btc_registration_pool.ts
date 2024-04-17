import { expect } from 'chai';

import { Keyring } from '@polkadot/api';

import { TEST_CONTROLLERS, TEST_RELAYERS } from '../../constants/keys';
import { getExtrinsicResult } from '../extrinsics';
import { describeDevNode } from '../set_dev_node';

describeDevNode('pallet_btc_registration_pool - request_system_vault', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const alithRelayer = keyring.addFromUri(TEST_RELAYERS[0].private);

  it('should successfully request system vault', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.btcRegistrationPool.requestSystemVault()
    ).signAndSend(alith);
    await context.createBlock();

    const rawSystemVault: any = await context.polkadotApi.query.btcRegistrationPool.systemVault();
    const systemVault = rawSystemVault.toHuman();

    expect(systemVault.address).is.eq('Pending');
    expect(systemVault.pubKeys).is.empty;
    expect(systemVault.m).is.eq('1');
    expect(systemVault.n).is.eq('1');
  });

  it('should successfully submit pub key', async function () {
    const pubKey = '0x02c56c0cf38df8708f2e5725102f87a1d91f9356b0b7ebc4f6cafb396684e143b4';
    const submit = {
      authorityId: alithRelayer.address,
      pubKey
    };

    const signature = '0x912088929bce91c813eb42a393ed2e5b2a36250e8ba483192dc9a2e4663401df42767fbbce7b1faccd9364c516923964fbfb6a0e914cf3a929e454b0cd49560e1c';
    await context.polkadotApi.tx.btcRegistrationPool.submitSystemVaultKey(submit, signature).send();
    await context.createBlock();

    const rawSystemVault: any = await context.polkadotApi.query.btcRegistrationPool.systemVault();
    const systemVault = rawSystemVault.toHuman();

    expect(systemVault.address['Generated']).is.ok;
    expect(systemVault.pubKeys[alithRelayer.address]).is.eq(pubKey);

    const rawBondedPubKey: any = await context.polkadotApi.query.btcRegistrationPool.bondedPubKey(pubKey);
    const bondedPubKey = rawBondedPubKey.toHuman();
    expect(bondedPubKey).is.eq('0x0000000000000000000000000000000000000100');
  });
});

describeDevNode('pallet_btc_registration_pool - request_vault', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const charleth = keyring.addFromUri(TEST_CONTROLLERS[2].private);

  it('should fail to join registration pool due to invalid refund address - wrong format', async function () {
    const refund = 'bcrt1qurj4xpaw95jlr28lqhankfdqce7tatgkeqrk9q1';

    await context.polkadotApi.tx.btcRegistrationPool.requestVault(refund).signAndSend(baltathar);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcRegistrationPool', 'requestVault');
    expect(extrinsicResult).eq('InvalidBitcoinAddress');
  });

  it('should fail to join registration pool due to invalid refund address - wrong network', async function () {
    const refund = 'bc1qe5l5jde9jc0w9psn9jstgcp82gy5rtnkpak4k4';

    await context.polkadotApi.tx.btcRegistrationPool.requestVault(refund).signAndSend(baltathar);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcRegistrationPool', 'requestVault');
    expect(extrinsicResult).eq('InvalidBitcoinAddress');
  });

  it('should fail to join registration pool due to invalid refund address - out of bound', async function () {
    const refund = '0x618f6a4a53f26200229549e55c592d0e1ee1dcac6292ed7296c91d53383eb6411f6159acb6f46412cae2a46bea906d4d35d31856a19fafac57cc19526712f8271c';

    await context.polkadotApi.tx.btcRegistrationPool.requestVault(refund).signAndSend(baltathar);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcRegistrationPool', 'requestVault');
    expect(extrinsicResult).eq('InvalidBitcoinAddress');
  });

  it('should successfully join registration pool', async function () {
    const refund = 'bcrt1qurj4xpaw95jlr28lqhankfdqce7tatgkeqrk9q';

    await context.polkadotApi.tx.btcRegistrationPool.requestVault(refund).signAndSend(baltathar);
    await context.createBlock();

    const rawBondedRefund: any = await context.polkadotApi.query.btcRegistrationPool.bondedRefund(refund);
    expect(rawBondedRefund.toJSON()).eq(baltathar.address);

    const rawRegisteredBitcoinPair: any = await context.polkadotApi.query.btcRegistrationPool.registrationPool(baltathar.address);
    const registeredBitcoinPair = rawRegisteredBitcoinPair.toHuman();
    expect(registeredBitcoinPair.refundAddress).eq(refund);
    expect(registeredBitcoinPair.vault.address).eq('Pending');
    expect(registeredBitcoinPair.vault.pubKeys).empty;
  });

  it('should fail to join registration pool due to duplicate refund address', async function () {
    const refund = 'bcrt1qurj4xpaw95jlr28lqhankfdqce7tatgkeqrk9q';

    await context.polkadotApi.tx.btcRegistrationPool.requestVault(refund).signAndSend(charleth);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcRegistrationPool', 'requestVault');
    expect(extrinsicResult).eq('AddressAlreadyRegistered');
  });

  it('should fail to join registration pool due to duplicate user address', async function () {
    const refund = 'bcrt1qurj4xpaw95jlr28lqhankfdqce7tatgkeqrk9q';

    await context.polkadotApi.tx.btcRegistrationPool.requestVault(refund).signAndSend(baltathar);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcRegistrationPool', 'requestVault');
    expect(extrinsicResult).eq('AddressAlreadyRegistered');
  });
});

describeDevNode('pallet_btc_registration_pool - submit_key (1-of-1)', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const alithRelayer = keyring.addFromUri(TEST_RELAYERS[0].private);
  const charlethRelayer = keyring.addFromUri(TEST_RELAYERS[2].private);

  before('should successfully join registration pool', async function () {
    const refund = 'bcrt1qurj4xpaw95jlr28lqhankfdqce7tatgkeqrk9q';

    await context.polkadotApi.tx.btcRegistrationPool.requestVault(refund).signAndSend(baltathar);
    await context.createBlock();
  });

  it('should fail to submit a key due to invalid signature', async function () {
    const pubKey = '0x02c56c0cf38df8708f2e5725102f87a1d91f9356b0b7ebc4f6cafb396684e143b4';
    const keySubmission = {
      authorityId: alithRelayer.address,
      who: baltathar.address,
      pubKey,
    };
    const signature = '0x012088929bce91c813eb42a393ed2e5b2a36250e8ba483192dc9a2e4663401df42767fbbce7b1faccd9364c516923964fbfb6a0e914cf3a929e454b0cd49560e1c';

    let errorMsg = '';
    await context.polkadotApi.tx.btcRegistrationPool.submitVaultKey(keySubmission, signature).send().catch(err => {
      if (err instanceof Error) {
        errorMsg = err.message;
      }
    });
    await context.createBlock();

    expect(errorMsg).eq('1010: Invalid Transaction: Transaction has a bad signature');
  });

  it('should fail to submit a key due to unknown relay executive', async function () {
    const pubKey = '0x02c56c0cf38df8708f2e5725102f87a1d91f9356b0b7ebc4f6cafb396684e143b4';
    const keySubmission = {
      authorityId: charlethRelayer.address,
      who: baltathar.address,
      pubKey,
    };
    const signature = '0x912088929bce91c813eb42a393ed2e5b2a36250e8ba483192dc9a2e4663401df42767fbbce7b1faccd9364c516923964fbfb6a0e914cf3a929e454b0cd49560e1c';

    let errorMsg = '';
    await context.polkadotApi.tx.btcRegistrationPool.submitVaultKey(keySubmission, signature).send().catch(err => {
      if (err instanceof Error) {
        errorMsg = err.message;
      }
    });
    await context.createBlock();

    expect(errorMsg).eq('1010: Invalid Transaction: Invalid signing address');
  });

  it('should successfully submit public key', async function () {
    const pubKey = '0x02c56c0cf38df8708f2e5725102f87a1d91f9356b0b7ebc4f6cafb396684e143b4';
    const keySubmission = {
      authorityId: alithRelayer.address,
      who: baltathar.address,
      pubKey,
    };
    const signature = '0x912088929bce91c813eb42a393ed2e5b2a36250e8ba483192dc9a2e4663401df42767fbbce7b1faccd9364c516923964fbfb6a0e914cf3a929e454b0cd49560e1c';

    await context.polkadotApi.tx.btcRegistrationPool.submitVaultKey(keySubmission, signature).send();
    await context.createBlock();

    const rawRegisteredBitcoinPair: any = await context.polkadotApi.query.btcRegistrationPool.registrationPool(baltathar.address);
    const registeredBitcoinPair = rawRegisteredBitcoinPair.toHuman();

    const vault = registeredBitcoinPair.vault;
    expect(vault.address['Generated']).is.exist;
    expect(Number(vault.m)).eq(Number(await context.polkadotApi.query.btcRegistrationPool.requiredM()));
    expect(Number(vault.n)).eq(Number(await context.polkadotApi.query.btcRegistrationPool.requiredN()));

    expect(Object.keys(registeredBitcoinPair.vault.pubKeys).length).eq(Number(await context.polkadotApi.query.btcRegistrationPool.requiredN()));
    expect(registeredBitcoinPair.vault.pubKeys[alithRelayer.address]).eq(pubKey);
  });

  it('should fail to submit a key due to vault address already generated', async function () {
    const pubKey = '0x02c56c0cf38df8708f2e5725102f87a1d91f9356b0b7ebc4f6cafb396684e143b4';
    const keySubmission = {
      authorityId: alithRelayer.address,
      who: baltathar.address,
      pubKey,
    };
    const signature = '0x912088929bce91c813eb42a393ed2e5b2a36250e8ba483192dc9a2e4663401df42767fbbce7b1faccd9364c516923964fbfb6a0e914cf3a929e454b0cd49560e1c';

    await context.polkadotApi.tx.btcRegistrationPool.submitVaultKey(keySubmission, signature).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcRegistrationPool', 'submitVaultKey');
    expect(extrinsicResult).eq('VaultAlreadyGenerated');
  });
});

describeDevNode('pallet_btc_registration_pool - submit_key (1-of-2)', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const alithRelayer = keyring.addFromUri(TEST_RELAYERS[0].private);
  const charlethRelayer = keyring.addFromUri(TEST_RELAYERS[2].private);

  before('should successfully set vault config', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.btcRegistrationPool.setVaultConfig(1, 2)
    ).signAndSend(alith);
    await context.createBlock();

    const m = Number(await context.polkadotApi.query.btcRegistrationPool.requiredM());
    const n = Number(await context.polkadotApi.query.btcRegistrationPool.requiredN());
    expect(m).eq(1);
    expect(n).eq(2);
  });

  before('should successfully add relay executive member', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.relayExecutiveMembership.addMember(charlethRelayer.address)
    ).signAndSend(alith);
    await context.createBlock();

    const rawMembers = await context.polkadotApi.query.relayExecutiveMembership.members();
    const members = rawMembers.toJSON();
    expect(members).contains(charlethRelayer.address);
  });

  before('should successfully join registration pool', async function () {
    const refund = 'bcrt1qurj4xpaw95jlr28lqhankfdqce7tatgkeqrk9q';

    await context.polkadotApi.tx.btcRegistrationPool.requestVault(refund).signAndSend(baltathar);
    await context.createBlock();
  });

  it('should successfully submit public key', async function () {
    const pubKey = '0x02c56c0cf38df8708f2e5725102f87a1d91f9356b0b7ebc4f6cafb396684e143b4';
    const keySubmission = {
      authorityId: alithRelayer.address,
      who: baltathar.address,
      pubKey,
    };
    const signature = '0x912088929bce91c813eb42a393ed2e5b2a36250e8ba483192dc9a2e4663401df42767fbbce7b1faccd9364c516923964fbfb6a0e914cf3a929e454b0cd49560e1c';

    await context.polkadotApi.tx.btcRegistrationPool.submitVaultKey(keySubmission, signature).send();
    await context.createBlock();

    const rawRegisteredBitcoinPair: any = await context.polkadotApi.query.btcRegistrationPool.registrationPool(baltathar.address);
    const registeredBitcoinPair = rawRegisteredBitcoinPair.toHuman();
    expect(registeredBitcoinPair.vault.address).eq('Pending');
    expect(Object.keys(registeredBitcoinPair.vault.pubKeys).length).eq(1);
    expect(registeredBitcoinPair.vault.pubKeys[alithRelayer.address]).eq(pubKey);
  });

  it('should fail to submit a key due to already submitted authority', async function () {
    const pubKey = '0x0200c708c3eef9658fd000b3262a5ddc4821f2adcd3f777eb3b2d002dcc04efb87';
    const keySubmission = {
      authorityId: alithRelayer.address,
      who: baltathar.address,
      pubKey,
    };
    const signature = '0xa1446489cfe7890ae4283f0a46993741025876a090858834f6a34fb0f9483517299c096cb6542aa790a26155f00292b5b17dd813033310cd46bf61833c4611041c';

    await context.polkadotApi.tx.btcRegistrationPool.submitVaultKey(keySubmission, signature).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcRegistrationPool', 'submitVaultKey');
    expect(extrinsicResult).eq('AuthorityAlreadySubmittedPubKey');
  });

  it('should fail to submit a key due to already submitted key', async function () {
    const pubKey = '0x02c56c0cf38df8708f2e5725102f87a1d91f9356b0b7ebc4f6cafb396684e143b4';
    const keySubmission = {
      authorityId: charlethRelayer.address,
      who: baltathar.address,
      pubKey,
    };
    const signature = '0xbd0acef6dab6157ce6f3992b450c59c81265adcb09d006f9062ff9fe36c227ca4001cd6929aee93166baf6e2ff057fbe57473274c70d563de99d4ee1217e7de41b';

    await context.polkadotApi.tx.btcRegistrationPool.submitVaultKey(keySubmission, signature).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcRegistrationPool', 'submitVaultKey');
    expect(extrinsicResult).eq('VaultAlreadyContainsPubKey');
  });

  it('should fail to submit a key due unknown user', async function () {
    const pubKey = '0x02c56c0cf38df8708f2e5725102f87a1d91f9356b0b7ebc4f6cafb396684e143b4';
    const keySubmission = {
      authorityId: charlethRelayer.address,
      who: alith.address,
      pubKey,
    };
    const signature = '0xbd0acef6dab6157ce6f3992b450c59c81265adcb09d006f9062ff9fe36c227ca4001cd6929aee93166baf6e2ff057fbe57473274c70d563de99d4ee1217e7de41b';

    await context.polkadotApi.tx.btcRegistrationPool.submitVaultKey(keySubmission, signature).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcRegistrationPool', 'submitVaultKey');
    expect(extrinsicResult).eq('UserDNE');
  });

  it('should successfully submit public key', async function () {
    const pubKey = '0x03495cb39c9c8a5c20f78e7eb33569bc12f583af7ed956b5f171edefaa5b1e5bd3';
    const keySubmission = {
      authorityId: charlethRelayer.address,
      who: baltathar.address,
      pubKey,
    };
    const signature = '0x97f5cebf5e669c81c3cb3cdc34519d9e7455235e40ffba92d3cf555d7523c7ca5e1d6d6895adbbeb936d1791753dbd70fdab4ae33f604a8857853b3fa02671771c';

    await context.polkadotApi.tx.btcRegistrationPool.submitVaultKey(keySubmission, signature).send();
    await context.createBlock();

    const rawRegisteredBitcoinPair: any = await context.polkadotApi.query.btcRegistrationPool.registrationPool(baltathar.address);
    const registeredBitcoinPair = rawRegisteredBitcoinPair.toHuman();

    const vault = registeredBitcoinPair.vault;
    expect(vault.address['Generated']).is.exist;
    expect(vault.address).is.exist;
    expect(Number(vault.m)).eq(Number(await context.polkadotApi.query.btcRegistrationPool.requiredM()));
    expect(Number(vault.n)).eq(Number(await context.polkadotApi.query.btcRegistrationPool.requiredN()));

    expect(Object.keys(registeredBitcoinPair.vault.pubKeys).length).eq(Number(await context.polkadotApi.query.btcRegistrationPool.requiredN()));
    expect(registeredBitcoinPair.vault.pubKeys[charlethRelayer.address]).eq(pubKey);
  });
});
