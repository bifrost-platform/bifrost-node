import { expect } from 'chai';

import { Keyring } from '@polkadot/api';

import { TEST_CONTROLLERS, TEST_RELAYERS } from '../../constants/keys';
import { getExtrinsicResult } from '../extrinsics';
import { describeDevNode } from '../set_dev_node';

describeDevNode('pallet_btc_registration_pool - request_vault', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const charleth = keyring.addFromUri(TEST_CONTROLLERS[2].private);

  it('should fail to join registration pool due to invalid refund address - wrong format', async function () {
    const refund = 'tb1p94937r32tem7qfh8v0erjqrvs5ca9js5wewmd93aa3yhdsr3pc5qdtsy5h1';

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
    const refund = 'tb1p94937r32tem7qfh8v0erjqrvs5ca9js5wewmd93aa3yhdsr3pc5qdtsy5h';

    await context.polkadotApi.tx.btcRegistrationPool.requestVault(refund).signAndSend(baltathar);
    await context.createBlock();

    const rawBondedRefund: any = await context.polkadotApi.query.btcRegistrationPool.bondedRefund(refund);
    expect(rawBondedRefund.toJSON()).eq(baltathar.address);

    const rawRegisteredBitcoinPair: any = await context.polkadotApi.query.btcRegistrationPool.registrationPool(baltathar.address);
    const registeredBitcoinPair = rawRegisteredBitcoinPair.toHuman();
    expect(registeredBitcoinPair.refundAddress).eq(refund);
    expect(registeredBitcoinPair.vaultAddress).eq('Pending');
    expect(registeredBitcoinPair.pubKeys).empty;
  });

  it('should fail to join registration pool due to duplicate refund address', async function () {
    const refund = 'tb1p94937r32tem7qfh8v0erjqrvs5ca9js5wewmd93aa3yhdsr3pc5qdtsy5h';

    await context.polkadotApi.tx.btcRegistrationPool.requestVault(refund).signAndSend(charleth);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcRegistrationPool', 'requestVault');
    expect(extrinsicResult).eq('RefundAddressAlreadyRegistered');
  });

  it('should fail to join registration pool due to duplicate user address', async function () {
    const refund = 'tb1qm25npfj7a5dzewgz95l4cqy5vxpqyx7n3yqsjc';

    await context.polkadotApi.tx.btcRegistrationPool.requestVault(refund).signAndSend(baltathar);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcRegistrationPool', 'requestVault');
    expect(extrinsicResult).eq('UserBfcAddressAlreadyRegistered');
  });
});

describeDevNode('pallet_btc_registration_pool - submit_key (1-of-1)', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const alithRelayer = keyring.addFromUri(TEST_RELAYERS[0].private);
  const charlethRelayer = keyring.addFromUri(TEST_RELAYERS[2].private);

  before('should successfully join registration pool', async function () {
    const refund = 'tb1p94937r32tem7qfh8v0erjqrvs5ca9js5wewmd93aa3yhdsr3pc5qdtsy5h';

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
    const signature = '0x57d23044a8da46ad8be01497332c8797f6369dabec84a5be6bac5a8d41d766c52cc82839ad99d12c46bfd65aaff20ff58aadbd71b4011697a378b26a94a2dde511';

    let errorMsg = '';
    await context.polkadotApi.tx.btcRegistrationPool.submitKey(keySubmission, signature).send().catch(err => {
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
    const signature = '0x57d23044a8da46ad8be01497332c8797f6369dabec84a5be6bac5a8d41d766c52cc82839ad99d12c46bfd65aaff20ff58aadbd71b4011697a378b26a94a2dde511';

    let errorMsg = '';
    await context.polkadotApi.tx.btcRegistrationPool.submitKey(keySubmission, signature).send().catch(err => {
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
    const signature = '0xd14ca16f2cc2f960ce31bb4199b6f7a3434794d316d8792b0c1f934be7ebd7ce28b07c4dbf2c07d3ec3731fdf97c4dfd3d5b6c9edf7d36abc42067a98c41b1741c';

    await context.polkadotApi.tx.btcRegistrationPool.submitKey(keySubmission, signature).send();
    await context.createBlock();

    const rawRegisteredBitcoinPair: any = await context.polkadotApi.query.btcRegistrationPool.registrationPool(baltathar.address);
    const registeredBitcoinPair = rawRegisteredBitcoinPair.toHuman();

    const vault = registeredBitcoinPair.vaultAddress['Generated'];
    expect(vault).is.exist;
    expect(vault.address).is.exist;
    expect(Number(vault.m)).eq(Number(await context.polkadotApi.query.btcRegistrationPool.requiredM()));
    expect(Number(vault.n)).eq(Number(await context.polkadotApi.query.btcRegistrationPool.requiredN()));

    expect(Object.keys(registeredBitcoinPair.pubKeys).length).eq(Number(await context.polkadotApi.query.btcRegistrationPool.requiredN()));
    expect(registeredBitcoinPair.pubKeys[alithRelayer.address]).eq(pubKey);
  });

  it('should fail to submit a key due to vault address already generated', async function () {
    const pubKey = '0x0248033c224979a9a190cfb147488d84b153a3352273dfac63fd895baefffb1697';
    const keySubmission = {
      authorityId: alithRelayer.address,
      who: baltathar.address,
      pubKey,
    };
    const signature = '0x57d23044a8da46ad8be01497332c8797f6369dabec84a5be6bac5a8d41d766c52cc82839ad99d12c46bfd65aaff20ff58aadbd71b4011697a378b26a94a2dde51b';

    await context.polkadotApi.tx.btcRegistrationPool.submitKey(keySubmission, signature).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcRegistrationPool', 'submitKey');
    expect(extrinsicResult).eq('VaultAddressAlreadyGenerated');
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
    const refund = 'tb1p94937r32tem7qfh8v0erjqrvs5ca9js5wewmd93aa3yhdsr3pc5qdtsy5h';

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
    const signature = '0xd14ca16f2cc2f960ce31bb4199b6f7a3434794d316d8792b0c1f934be7ebd7ce28b07c4dbf2c07d3ec3731fdf97c4dfd3d5b6c9edf7d36abc42067a98c41b1741c';

    await context.polkadotApi.tx.btcRegistrationPool.submitKey(keySubmission, signature).send();
    await context.createBlock();

    const rawRegisteredBitcoinPair: any = await context.polkadotApi.query.btcRegistrationPool.registrationPool(baltathar.address);
    const registeredBitcoinPair = rawRegisteredBitcoinPair.toHuman();
    expect(registeredBitcoinPair.vaultAddress).eq('Pending');
    expect(Object.keys(registeredBitcoinPair.pubKeys).length).eq(1);
    expect(registeredBitcoinPair.pubKeys[alithRelayer.address]).eq(pubKey);
  });

  it('should fail to submit a key due to already submitted authority', async function () {
    const pubKey = '0x0200c708c3eef9658fd000b3262a5ddc4821f2adcd3f777eb3b2d002dcc04efb87';
    const keySubmission = {
      authorityId: alithRelayer.address,
      who: baltathar.address,
      pubKey,
    };
    const signature = '0x282bb4f02c01570de8cde8f9d86459eedc622143822c70f720560a94d3dd5b401e8ef18e11377e142dd6995817d9f971d19ba83d7e96e9434111d68b15b15f0f1c';

    await context.polkadotApi.tx.btcRegistrationPool.submitKey(keySubmission, signature).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcRegistrationPool', 'submitKey');
    expect(extrinsicResult).eq('AuthorityAlreadySubmittedPubKey');
  });

  it('should fail to submit a key due to already submitted key', async function () {
    const pubKey = '0x02c56c0cf38df8708f2e5725102f87a1d91f9356b0b7ebc4f6cafb396684e143b4';
    const keySubmission = {
      authorityId: charlethRelayer.address,
      who: baltathar.address,
      pubKey,
    };
    const signature = '0x29c01893855199ee261ed2754d2676c6c611eb9eb2612e663c526ed097fd7d5e7349ae5794aa22f4befd3bb372430e007878920e175665dacdec995968f94c781b';

    await context.polkadotApi.tx.btcRegistrationPool.submitKey(keySubmission, signature).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcRegistrationPool', 'submitKey');
    expect(extrinsicResult).eq('VaultAddressAlreadyContainsPubKey');
  });

  it('should fail to submit a key due unknown user', async function () {
    const pubKey = '0x02c56c0cf38df8708f2e5725102f87a1d91f9356b0b7ebc4f6cafb396684e143b4';
    const keySubmission = {
      authorityId: charlethRelayer.address,
      who: alith.address,
      pubKey,
    };
    const signature = '0x8964b849d29c912b3f6402f780eb5243bab7129f9e6782e4f5457509460f2c795669f53f14fa8846211f20a22c026a3d8ef680fcc75e6ed57e64a4c52deb2e3c1c';

    await context.polkadotApi.tx.btcRegistrationPool.submitKey(keySubmission, signature).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcRegistrationPool', 'submitKey');
    expect(extrinsicResult).eq('UserDNE');
  });

  it('should successfully submit public key', async function () {
    const pubKey = '0x03495cb39c9c8a5c20f78e7eb33569bc12f583af7ed956b5f171edefaa5b1e5bd3';
    const keySubmission = {
      authorityId: charlethRelayer.address,
      who: baltathar.address,
      pubKey,
    };
    const signature = '0x28d6e5dedddfc98b598ace3128c7df6bb595566d3ae5724d3964804487a4b77773c2b82f4c6be892a57c2c58f845b6f9df888d168cd4458450a2d3018088c3ca1b';

    await context.polkadotApi.tx.btcRegistrationPool.submitKey(keySubmission, signature).send();
    await context.createBlock();

    const rawRegisteredBitcoinPair: any = await context.polkadotApi.query.btcRegistrationPool.registrationPool(baltathar.address);
    const registeredBitcoinPair = rawRegisteredBitcoinPair.toHuman();

    const vault = registeredBitcoinPair.vaultAddress['Generated'];
    expect(vault).is.exist;
    expect(vault.address).is.exist;
    expect(Number(vault.m)).eq(Number(await context.polkadotApi.query.btcRegistrationPool.requiredM()));
    expect(Number(vault.n)).eq(Number(await context.polkadotApi.query.btcRegistrationPool.requiredN()));

    expect(Object.keys(registeredBitcoinPair.pubKeys).length).eq(Number(await context.polkadotApi.query.btcRegistrationPool.requiredN()));
    expect(registeredBitcoinPair.pubKeys[charlethRelayer.address]).eq(pubKey);
  });
});
