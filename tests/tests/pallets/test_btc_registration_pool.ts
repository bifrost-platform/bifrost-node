import { expect } from 'chai';

import { Keyring } from '@polkadot/api';

import { TEST_CONTROLLERS, TEST_RELAYERS } from '../../constants/keys';
import { getExtrinsicResult } from '../extrinsics';
import { describeDevNode, INodeContext } from '../set_dev_node';

async function getCurrentRound(context: INodeContext) {
  const rawCurrentRound: any = await context.polkadotApi.query.btcRegistrationPool.currentRound();
  return rawCurrentRound.toJSON();
}

describeDevNode('pallet_btc_registration_pool - request_system_vault', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const alithRelayer = keyring.addFromUri(TEST_RELAYERS[0].private);

  it('should successfully request system vault', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.btcRegistrationPool.requestSystemVault(false)
    ).signAndSend(alith);
    await context.createBlock();

    const rawSystemVault: any = await context.polkadotApi.query.btcRegistrationPool.systemVault(await getCurrentRound(context));
    const systemVault = rawSystemVault.toHuman();

    expect(systemVault.address).is.eq('Pending');
    expect(systemVault.pubKeys).is.empty;
    expect(systemVault.m).is.eq('1');
    expect(systemVault.n).is.eq('1');
  });

  it('should successfully submit pub key', async function () {
    const pubKey = '0x02c56c0cf38df8708f2e5725102f87a1d91f9356b0b7ebc4f6cafb396684e143b4';
    const who = '0x0000000000000000000000000000000000000100';
    const submit = {
      authorityId: alithRelayer.address,
      who,
      pubKey,
      poolRound: await getCurrentRound(context),
    };

    const signature = '0xd19701003fb3b0ad88cad82c85da2bf01b1e6855c0636384fd23ba061ec0fbc077c386a05f013f3f0f53faa5fe59f977cc557a7176ba00acfc7655a6767a121d1b';
    await context.polkadotApi.tx.btcRegistrationPool.submitSystemVaultKey(submit, signature).send();
    await context.createBlock();

    const rawSystemVault: any = await context.polkadotApi.query.btcRegistrationPool.systemVault(await getCurrentRound(context));
    const systemVault = rawSystemVault.toHuman();

    expect(systemVault.descriptor).is.eq('wsh(sortedmulti(1,02c56c0cf38df8708f2e5725102f87a1d91f9356b0b7ebc4f6cafb396684e143b4))#0hzltyp0');

    expect(systemVault.address['Generated']).is.ok;
    expect(systemVault.pubKeys[alithRelayer.address]).is.eq(pubKey);

    const rawBondedPubKey: any = await context.polkadotApi.query.btcRegistrationPool.bondedPubKey(await getCurrentRound(context), pubKey);
    const bondedPubKey = rawBondedPubKey.toHuman();
    expect(bondedPubKey).is.eq(who);
  });

  it('should successfully clear vault', async function () {
    const vault = 'bcrt1qq8u3pf4z60udx43w534htszh8p7xmdk5njemsc7gsn6smgdgg58qvavm86';
    const pubKey = '0x02c56c0cf38df8708f2e5725102f87a1d91f9356b0b7ebc4f6cafb396684e143b4';

    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.btcRegistrationPool.clearVault(vault)
    ).signAndSend(alith);
    await context.createBlock();

    const rawBondedVault: any = await context.polkadotApi.query.btcRegistrationPool.bondedVault(await getCurrentRound(context), vault);
    const bondedVault = rawBondedVault.toHuman();
    expect(bondedVault).is.null;

    const rawBondedDescriptor: any = await context.polkadotApi.query.btcRegistrationPool.bondedDescriptor(await getCurrentRound(context), vault);
    const bondedDescriptor = rawBondedDescriptor.toHuman();
    expect(bondedDescriptor).is.null;

    const rawBondedPubKey: any = await context.polkadotApi.query.btcRegistrationPool.bondedPubKey(await getCurrentRound(context), pubKey);
    const bondedPubKey = rawBondedPubKey.toHuman()
    expect(bondedPubKey).is.null;

    const rawSystemVault: any = await context.polkadotApi.query.btcRegistrationPool.systemVault(await getCurrentRound(context));
    const systemVault = rawSystemVault.toHuman();
    expect(systemVault).is.null;
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

    const rawBondedRefund: any = await context.polkadotApi.query.btcRegistrationPool.bondedRefund(await getCurrentRound(context), refund);
    expect(rawBondedRefund.toJSON()[0]).eq(baltathar.address);

    const rawRegisteredBitcoinPair: any = await context.polkadotApi.query.btcRegistrationPool.registrationPool(await getCurrentRound(context), baltathar.address);
    const registeredBitcoinPair = rawRegisteredBitcoinPair.toHuman();
    expect(registeredBitcoinPair.refundAddress).eq(refund);
    expect(registeredBitcoinPair.vault.address).eq('Pending');
    expect(registeredBitcoinPair.vault.pubKeys).empty;
  });

  it('should fail to join registration pool due to duplicate refund address', async function () {
    const refund = 'bcrt1qurj4xpaw95jlr28lqhankfdqce7tatgkeqrk9q';

    await context.polkadotApi.tx.btcRegistrationPool.requestVault(refund).signAndSend(charleth);
    await context.createBlock();

    const rawBondedRefund: any = await context.polkadotApi.query.btcRegistrationPool.bondedRefund(await getCurrentRound(context), refund);
    expect(rawBondedRefund.toJSON()[1]).eq(charleth.address);
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
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
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
      poolRound: await getCurrentRound(context),
    };
    const signature = '0x119701003fb3b0ad88cad82c85da2bf01b1e6855c0636384fd23ba061ec0fbc077c386a05f013f3f0f53faa5fe59f977cc557a7176ba00acfc7655a6767a121d1b';

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
      poolRound: await getCurrentRound(context),
    };
    const signature = '0xd19701003fb3b0ad88cad82c85da2bf01b1e6855c0636384fd23ba061ec0fbc077c386a05f013f3f0f53faa5fe59f977cc557a7176ba00acfc7655a6767a121d1b';

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
      poolRound: await getCurrentRound(context),
    };
    const signature = '0xd19701003fb3b0ad88cad82c85da2bf01b1e6855c0636384fd23ba061ec0fbc077c386a05f013f3f0f53faa5fe59f977cc557a7176ba00acfc7655a6767a121d1b';

    await context.polkadotApi.tx.btcRegistrationPool.submitVaultKey(keySubmission, signature).send();
    await context.createBlock();

    const rawRegisteredBitcoinPair: any = await context.polkadotApi.query.btcRegistrationPool.registrationPool(await getCurrentRound(context), baltathar.address);
    const registeredBitcoinPair = rawRegisteredBitcoinPair.toHuman();

    const vault = registeredBitcoinPair.vault;
    expect(vault.address['Generated']).is.exist;

    const rawRatio: any = await context.polkadotApi.query.btcRegistrationPool.multiSigRatio();
    const ratio = rawRatio.toJSON();
    const rawRelayExec: any = await context.polkadotApi.query.relayExecutiveMembership.members();
    const relayExec = rawRelayExec.toJSON();

    expect(Number(vault.m)).eq(Math.ceil(relayExec.length * ratio / 100));
    expect(Number(vault.n)).eq(relayExec.length);

    expect(Object.keys(registeredBitcoinPair.vault.pubKeys).length).eq(Math.ceil(relayExec.length * ratio / 100));
    expect(registeredBitcoinPair.vault.pubKeys[alithRelayer.address]).eq(pubKey);
  });

  it('should fail to submit a key due to vault address already generated', async function () {
    const pubKey = '0x02c56c0cf38df8708f2e5725102f87a1d91f9356b0b7ebc4f6cafb396684e143b4';
    const keySubmission = {
      authorityId: alithRelayer.address,
      who: baltathar.address,
      pubKey,
      poolRound: await getCurrentRound(context),
    };
    const signature = '0xd19701003fb3b0ad88cad82c85da2bf01b1e6855c0636384fd23ba061ec0fbc077c386a05f013f3f0f53faa5fe59f977cc557a7176ba00acfc7655a6767a121d1b';

    await context.polkadotApi.tx.btcRegistrationPool.submitVaultKey(keySubmission, signature).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcRegistrationPool', 'submitVaultKey');
    expect(extrinsicResult).eq('VaultAlreadyGenerated');
  });

  it('should successfully clear vault', async function () {
    const vault = 'bcrt1qq8u3pf4z60udx43w534htszh8p7xmdk5njemsc7gsn6smgdgg58qvavm86';
    const refund = 'bcrt1qurj4xpaw95jlr28lqhankfdqce7tatgkeqrk9q';
    const pubKey = '0x02c56c0cf38df8708f2e5725102f87a1d91f9356b0b7ebc4f6cafb396684e143b4';

    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.btcRegistrationPool.clearVault(vault)
    ).signAndSend(alith);
    await context.createBlock();

    const rawBondedVault: any = await context.polkadotApi.query.btcRegistrationPool.bondedVault(await getCurrentRound(context), vault);
    const bondedVault = rawBondedVault.toHuman();
    expect(bondedVault).is.null;

    const rawBondedRefund: any = await context.polkadotApi.query.btcRegistrationPool.bondedRefund(await getCurrentRound(context), refund);
    const bondedRefund = rawBondedRefund.toHuman();
    expect(bondedRefund).is.empty;

    const rawBondedDescriptor: any = await context.polkadotApi.query.btcRegistrationPool.bondedDescriptor(await getCurrentRound(context), vault);
    const bondedDescriptor = rawBondedDescriptor.toHuman();
    expect(bondedDescriptor).is.null;

    const rawBondedPubKey: any = await context.polkadotApi.query.btcRegistrationPool.bondedPubKey(await getCurrentRound(context), pubKey);
    const bondedPubKey = rawBondedPubKey.toHuman()
    expect(bondedPubKey).is.null;

    const rawTarget: any = await context.polkadotApi.query.btcRegistrationPool.registrationPool(await getCurrentRound(context), baltathar.address);
    const target = rawTarget.toHuman();
    expect(target).is.null;
  });
});

describeDevNode('pallet_btc_registration_pool - submit_key (2-of-2)', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const alithRelayer = keyring.addFromUri(TEST_RELAYERS[0].private);
  const charlethRelayer = keyring.addFromUri(TEST_RELAYERS[2].private);

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
      poolRound: await getCurrentRound(context),
    };
    const signature = '0xd19701003fb3b0ad88cad82c85da2bf01b1e6855c0636384fd23ba061ec0fbc077c386a05f013f3f0f53faa5fe59f977cc557a7176ba00acfc7655a6767a121d1b';

    await context.polkadotApi.tx.btcRegistrationPool.submitVaultKey(keySubmission, signature).send();
    await context.createBlock();

    const rawRegisteredBitcoinPair: any = await context.polkadotApi.query.btcRegistrationPool.registrationPool(await getCurrentRound(context), baltathar.address);
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
      poolRound: await getCurrentRound(context),
    };
    const signature = '0xdee1cda6379f7fb3ab5df5fb7ac1c2263c535c5dd81c8ae2d35ca195be23d1fb4b5d3ac43470343c1be171104dcea17a8a19bccbc6e67be8d1cfb816ba00f9c91b';

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
      poolRound: await getCurrentRound(context),
    };
    const signature = '0x77b52fdc48d10cdbfeb90b0e9f58209dc5ef6e57881f63fc880f29b6469f54ec6e5d1f2d0dfb9f628682d5ac88985c1238bcf9354003a6163189ec2ef9defe211c';

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
      poolRound: await getCurrentRound(context),
    };
    const signature = '0x77b52fdc48d10cdbfeb90b0e9f58209dc5ef6e57881f63fc880f29b6469f54ec6e5d1f2d0dfb9f628682d5ac88985c1238bcf9354003a6163189ec2ef9defe211c';

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
      poolRound: await getCurrentRound(context),
    };
    const signature = '0x2b8c33210483b1787d9aad10281c5f41583002ed3b7406672ca469b77990febb3c104a871698c0e367eb9343f977e48908a7b1d65756408b0a116a069febd4e61c';

    await context.polkadotApi.tx.btcRegistrationPool.submitVaultKey(keySubmission, signature).send();
    await context.createBlock();

    const rawRegisteredBitcoinPair: any = await context.polkadotApi.query.btcRegistrationPool.registrationPool(await getCurrentRound(context), baltathar.address);
    const registeredBitcoinPair = rawRegisteredBitcoinPair.toHuman();

    const vault = registeredBitcoinPair.vault;
    expect(vault.address['Generated']).is.exist;
    expect(vault.address).is.exist;

    const rawRatio: any = await context.polkadotApi.query.btcRegistrationPool.multiSigRatio();
    const ratio = rawRatio.toJSON();
    const rawRelayExec: any = await context.polkadotApi.query.relayExecutiveMembership.members();
    const relayExec = rawRelayExec.toJSON();

    expect(Number(vault.m)).eq(Math.ceil(relayExec.length * ratio / 100));
    expect(Number(vault.n)).eq(relayExec.length);

    expect(Object.keys(registeredBitcoinPair.vault.pubKeys).length).eq(Math.ceil(relayExec.length * ratio / 100));
    expect(registeredBitcoinPair.vault.pubKeys[charlethRelayer.address]).eq(pubKey);
  });
});
