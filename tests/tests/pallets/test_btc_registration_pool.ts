import { expect } from 'chai';

import { Keyring } from '@polkadot/api';

import { TEST_CONTROLLERS } from '../../constants/keys';
import { getExtrinsicResult } from '../extrinsics';
import { describeDevNode } from '../set_dev_node';

const ISSUER = '0x052368678191fe3754ffd28015ff6ac54601e76c';

describeDevNode('pallet_btc_registration_pool - register', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const charleth = keyring.addFromUri(TEST_CONTROLLERS[2].private);
  const dorothy = keyring.addFromUri(TEST_CONTROLLERS[3].private);
  const ethan = keyring.addFromUri(TEST_CONTROLLERS[4].private);

  before('should succesfully set issuer', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.btcRegistrationPool.setIssuer(ISSUER)
    ).signAndSend(alith);
    await context.createBlock();

    const rawIssuer: any = await context.polkadotApi.query.btcRegistrationPool.addressIssuer();
    const issuer = rawIssuer.toJSON();
    expect(issuer).eq(ISSUER);
  });

  it('should fail to join bitcoin registration pool due to invalid vault address', async function () {
    const refund = 'bc1qe5l5jde9jc0w9psn9jstgcp82gy5rtnkpak4k4';
    const signature = '0x618f6a4a53f26200229549e55c592d0e1ee1dcac6292ed7296c91d53383eb6411f6159acb6f46412cae2a46bea906d4d35d31856a19fafac57cc19526712f8271c';
    const vault = signature;

    await context.polkadotApi.tx.btcRegistrationPool.register(refund, vault, signature).signAndSend(baltathar);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcRegistrationPool', 'register');
    expect(extrinsicResult).eq('InvalidBitcoinAddress');
  });

  it('should fail to join bitcoin registration pool due to invalid refund address', async function () {
    const vault = 'bc1qx8g09fhlza3whpt7ae7z3tyrh8akfc7anyfr3c'
    const signature = '0x618f6a4a53f26200229549e55c592d0e1ee1dcac6292ed7296c91d53383eb6411f6159acb6f46412cae2a46bea906d4d35d31856a19fafac57cc19526712f8271c';
    const refund = signature;

    await context.polkadotApi.tx.btcRegistrationPool.register(refund, vault, signature).signAndSend(baltathar);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcRegistrationPool', 'register');
    expect(extrinsicResult).eq('InvalidBitcoinAddress');
  });

  it('should fail to join bitcoin registration pool due to invalid signature - segwit refund', async function () {
    const refund = 'bc1qe5l5jde9jc0w9psn9jstgcp82gy5rtnkpak4k4';
    const vault = 'bc1qx8g09fhlza3whpt7ae7z3tyrh8akfc7anyfr3c'
    const signature = '0x618f6a4a53f26200229549e55c592d0e1ee1dcac6292ed7296c91d53383eb6411f6159acb6f46412cae2a46bea906d4d35d31856a19fafac57cc19526712f82711';

    await context.polkadotApi.tx.btcRegistrationPool.register(refund, vault, signature).signAndSend(baltathar);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcRegistrationPool', 'register');
    expect(extrinsicResult).eq('InvalidSignature');
  });

  it('should successfully join bitcoin registration pool - segwit refund', async function () {
    const refund = 'bc1qe5l5jde9jc0w9psn9jstgcp82gy5rtnkpak4k4';
    const vault = 'bc1qx8g09fhlza3whpt7ae7z3tyrh8akfc7anyfr3c'
    const signature = '0x618f6a4a53f26200229549e55c592d0e1ee1dcac6292ed7296c91d53383eb6411f6159acb6f46412cae2a46bea906d4d35d31856a19fafac57cc19526712f8271c';

    await context.polkadotApi.tx.btcRegistrationPool.register(refund, vault, signature).signAndSend(baltathar);
    await context.createBlock();

    const rawBondedVault: any = await context.polkadotApi.query.btcRegistrationPool.bondedVault(vault);
    expect(rawBondedVault.toJSON()).eq(baltathar.address);

    const rawBondedRefund: any = await context.polkadotApi.query.btcRegistrationPool.bondedRefund(refund);
    expect(rawBondedRefund.toJSON()).eq(baltathar.address);

    const rawRegisteredBitcoinPair: any = await context.polkadotApi.query.btcRegistrationPool.registrationPool(baltathar.address);
    const registeredBitcoinPair = rawRegisteredBitcoinPair.toHuman();
    expect(registeredBitcoinPair.refundAddress).eq(refund);
    expect(registeredBitcoinPair.vaultAddress).eq(vault);
  });

  it('should fail to join bitcoin registration pool due to already registered refund address', async function () {
    const refund = 'bc1qe5l5jde9jc0w9psn9jstgcp82gy5rtnkpak4k4';
    const vault = 'bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq'
    const signature = '0x893047e180c024f270d705f3baccfd5317c495a5d4d7de250588aa4312ff99750075ca3d4d5a1f9fd8062aa9b8c3cc823b4b57ce24044b17ef3953c4d435ff261c';

    await context.polkadotApi.tx.btcRegistrationPool.register(refund, vault, signature).signAndSend(charleth);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcRegistrationPool', 'register');
    expect(extrinsicResult).eq('RefundAddressAlreadyRegistered');
  });

  it('should fail to join bitcoin registration pool due to already registered vault address', async function () {
    const refund = 'bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq';
    const vault = 'bc1qx8g09fhlza3whpt7ae7z3tyrh8akfc7anyfr3c'
    const signature = '0x367160f12d45bb7f8dcebf92213e9672d3be8b95bad3178a5217e577a62975c50bb324f859d4d71b5d06f2c17a7eb73980b8b35533e464b2d87e0804ef3b0d791b';

    await context.polkadotApi.tx.btcRegistrationPool.register(refund, vault, signature).signAndSend(charleth);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcRegistrationPool', 'register');
    expect(extrinsicResult).eq('VaultAddressAlreadyRegistered');
  });

  it('should fail to join bitcoin registration pool due to already registered bifrost address', async function () {
    const refund = 'bc1p5d7rjq7g6rdk2yhzks9smlaqtedr4dekq08ge8ztwac72sfr9rusxg3297';
    const vault = 'bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq'
    const signature = '0x10c0bda2d96b7f089def4c302861eda19df7d3e85f3bac4af4620490446fa7b84f3da1adb866d1ba3494cecd55d85049c86466ddffa123152af3e8e547ffac5c1c';

    await context.polkadotApi.tx.btcRegistrationPool.register(refund, vault, signature).signAndSend(baltathar);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcRegistrationPool', 'register');
    expect(extrinsicResult).eq('UserBfcAddressAlreadyRegistered');
  });

  it('should fail to join bitcoin registration pool due to identical bitcoin address pair', async function () {
    const refund = 'bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq';
    const vault = 'bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq'
    const signature = '0x10c0bda2d96b7f089def4c302861eda19df7d3e85f3bac4af4620490446fa7b84f3da1adb866d1ba3494cecd55d85049c86466ddffa123152af3e8e547ffac5c1c';

    await context.polkadotApi.tx.btcRegistrationPool.register(refund, vault, signature).signAndSend(charleth);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcRegistrationPool', 'register');
    expect(extrinsicResult).eq('RefundAndVaultAddressIdentical');
  });

  it('should fail to join bitcoin registration pool due to invalid signature - taproot refund', async function () {
    const refund = 'bc1p5d7rjq7g6rdk2yhzks9smlaqtedr4dekq08ge8ztwac72sfr9rusxg3297';
    const vault = 'bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq'
    const signature = '0x10c0bda2d96b7f089def4c302861eda19df7d3e85f3bac4af4620490446fa7b84f3da1adb866d1ba3494cecd55d85049c86466ddffa123152af3e8e547ffac5c11';

    await context.polkadotApi.tx.btcRegistrationPool.register(refund, vault, signature).signAndSend(charleth);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcRegistrationPool', 'register');
    expect(extrinsicResult).eq('InvalidSignature');
  });

  it('should successfully join bitcoin registration pool - taproot refund', async function () {
    const refund = 'bc1p5d7rjq7g6rdk2yhzks9smlaqtedr4dekq08ge8ztwac72sfr9rusxg3297';
    const vault = 'bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq'
    const signature = '0x10c0bda2d96b7f089def4c302861eda19df7d3e85f3bac4af4620490446fa7b84f3da1adb866d1ba3494cecd55d85049c86466ddffa123152af3e8e547ffac5c1c';

    await context.polkadotApi.tx.btcRegistrationPool.register(refund, vault, signature).signAndSend(charleth);
    await context.createBlock();

    const rawBondedVault: any = await context.polkadotApi.query.btcRegistrationPool.bondedVault(vault);
    expect(rawBondedVault.toJSON()).eq(charleth.address);

    const rawBondedRefund: any = await context.polkadotApi.query.btcRegistrationPool.bondedRefund(refund);
    expect(rawBondedRefund.toJSON()).eq(charleth.address);

    const rawRegisteredBitcoinPair: any = await context.polkadotApi.query.btcRegistrationPool.registrationPool(charleth.address);
    const registeredBitcoinPair = rawRegisteredBitcoinPair.toHuman();
    expect(registeredBitcoinPair.refundAddress).eq(refund);
    expect(registeredBitcoinPair.vaultAddress).eq(vault);
  });

  it('should fail to join bitcoin registration pool due to invalid signature - script refund', async function () {
    const refund = '3J98t1WpEZ73CNmQviecrnyiWrnqRhWNLy';
    const vault = 'bc1q07r5j9vmdu3pznajeerr762afhw265q5g8652l'
    const signature = '0xfd994a34e215d7caa23bb123ad79d5e6355d0e417c8bb4892da4866b6eb3cc751ebd81ee895b3913182efa811a01f53ba69e8feadf309ab9ec25d8b012e39bf811';

    await context.polkadotApi.tx.btcRegistrationPool.register(refund, vault, signature).signAndSend(dorothy);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcRegistrationPool', 'register');
    expect(extrinsicResult).eq('InvalidSignature');
  });

  it('should successfully join bitcoin registration pool - script refund', async function () {
    const refund = '3J98t1WpEZ73CNmQviecrnyiWrnqRhWNLy';
    const vault = 'bc1q07r5j9vmdu3pznajeerr762afhw265q5g8652l'
    const signature = '0xfd994a34e215d7caa23bb123ad79d5e6355d0e417c8bb4892da4866b6eb3cc751ebd81ee895b3913182efa811a01f53ba69e8feadf309ab9ec25d8b012e39bf81c';

    await context.polkadotApi.tx.btcRegistrationPool.register(refund, vault, signature).signAndSend(dorothy);
    await context.createBlock();

    const rawBondedVault: any = await context.polkadotApi.query.btcRegistrationPool.bondedVault(vault);
    expect(rawBondedVault.toJSON()).eq(dorothy.address);

    const rawBondedRefund: any = await context.polkadotApi.query.btcRegistrationPool.bondedRefund(refund);
    expect(rawBondedRefund.toJSON()).eq(dorothy.address);

    const rawRegisteredBitcoinPair: any = await context.polkadotApi.query.btcRegistrationPool.registrationPool(dorothy.address);
    const registeredBitcoinPair = rawRegisteredBitcoinPair.toHuman();
    expect(registeredBitcoinPair.refundAddress).eq(refund);
    expect(registeredBitcoinPair.vaultAddress).eq(vault);
  });

  it('should fail to join bitcoin registration pool due to invalid signature - legacy refund', async function () {
    const refund = '1BvBMSEYstWetqTFn5Au4m4GFg7xJaNVN2';
    const vault = 'bc1qjh8tkutv2t9ka46rxz5phzx6r2fdklm5tmv362'
    const signature = '0x79b8b95ec6eae5124038904ccd3b11689f4ccff136b4c5bf1e991999c75b707b0590825ad027896983e0fd8c1c0c288499e590f84d1c734ca6a87b9aab9befac11';

    await context.polkadotApi.tx.btcRegistrationPool.register(refund, vault, signature).signAndSend(ethan);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'btcRegistrationPool', 'register');
    expect(extrinsicResult).eq('InvalidSignature');
  });

  it('should successfully join bitcoin registration pool - legacy refund', async function () {
    const refund = '1BvBMSEYstWetqTFn5Au4m4GFg7xJaNVN2';
    const vault = 'bc1qjh8tkutv2t9ka46rxz5phzx6r2fdklm5tmv362'
    const signature = '0x79b8b95ec6eae5124038904ccd3b11689f4ccff136b4c5bf1e991999c75b707b0590825ad027896983e0fd8c1c0c288499e590f84d1c734ca6a87b9aab9befac1b';

    await context.polkadotApi.tx.btcRegistrationPool.register(refund, vault, signature).signAndSend(ethan);
    await context.createBlock();

    const rawBondedVault: any = await context.polkadotApi.query.btcRegistrationPool.bondedVault(vault);
    expect(rawBondedVault.toJSON()).eq(ethan.address);

    const rawBondedRefund: any = await context.polkadotApi.query.btcRegistrationPool.bondedRefund(refund);
    expect(rawBondedRefund.toJSON()).eq(ethan.address);

    const rawRegisteredBitcoinPair: any = await context.polkadotApi.query.btcRegistrationPool.registrationPool(ethan.address);
    const registeredBitcoinPair = rawRegisteredBitcoinPair.toHuman();
    expect(registeredBitcoinPair.refundAddress).eq(refund);
    expect(registeredBitcoinPair.vaultAddress).eq(vault);
  });
});
