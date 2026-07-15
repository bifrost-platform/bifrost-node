import { expect } from 'chai';

import { Keyring } from '@polkadot/api';

import { TEST_CONTROLLERS } from '../../constants/keys';
import { getExtrinsicResult } from '../extrinsics';
import { describeDevNode, INodeContext } from '../set_dev_node';
import { sendPrecompileTx } from '../transactions';
import {
  encodeCreatePool, grantPermission, FIVE_PERCENT_APR, NFT_CONTRACT, PERMISSIONS_PRECOMPILE_ADDRESS,
  PERMISSIONS_SELECTORS, POOLS_PRECOMPILE_ADDRESS, POOLS_SELECTORS, VAULT_ADDRESS_A
} from './rwa_helpers';

const CHAIN_ID = 1;
const TRANCHE_ID = { chainId: CHAIN_ID, vaultAddress: VAULT_ADDRESS_A };
const UNKNOWN_TRANCHE_ID = { chainId: 99, vaultAddress: '0x' + '99'.repeat(20) };

async function createTestPool(
  context: INodeContext,
  poolAdmin: { public: string, private: string },
  borrower: { public: string, private: string },
  poolId: number,
) {
  const data = encodeCreatePool(
    context, poolId, borrower.public, 86_400, 3_600, true, true,
    [{ nftContract: NFT_CONTRACT, nftTokenId: poolId.toString() }],
    [{ chainId: CHAIN_ID, vaultAddress: VAULT_ADDRESS_A, isSenior: true, apr: FIVE_PERCENT_APR, maxDeposits: '0' }],
  );
  const block = await sendPrecompileTx(
    context, POOLS_PRECOMPILE_ADDRESS, POOLS_SELECTORS, poolAdmin.public, poolAdmin.private, 'create_pool', [data],
  );
  const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
  expect(Boolean(receipt.status)).eq(true);
}

describeDevNode('pallet_rwa_permissions - grant_permission / revoke_permission (PoolAdmin role)', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const poolAdmin: { public: string, private: string } = TEST_CONTROLLERS[2];
  let alithNonce: number;
  let baltatharNonce: number;

  beforeEach(async function () {
    const alithAccount = await context.polkadotApi.query.system.account(alith.address);
    alithNonce = alithAccount.nonce.toNumber();
    const baltatharAccount = await context.polkadotApi.query.system.account(baltathar.address);
    baltatharNonce = baltatharAccount.nonce.toNumber();
  });

  it('should fail to grant PoolAdmin from a non-root origin', async function () {
    await context.polkadotApi.tx.rwaPermissions.grantPermission(1, 'PoolAdmin', poolAdmin.public)
      .signAndSend(baltathar, { nonce: baltatharNonce++ });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'rwaPermissions', 'grantPermission');
    expect(extrinsicResult).eq('BadOrigin');
  });

  it('should fail to grant the reserved Borrower role, even via sudo', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.rwaPermissions.grantPermission(1, 'Borrower', poolAdmin.public)
    ).signAndSend(alith, { nonce: alithNonce++ });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'sudo', 'sudo');
    expect(extrinsicResult).eq('BorrowerRoleReserved');
  });

  it('should successfully grant PoolAdmin via sudo', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.rwaPermissions.grantPermission(1, 'PoolAdmin', poolAdmin.public)
    ).signAndSend(alith, { nonce: alithNonce++ });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'sudo', 'sudo');
    expect(extrinsicResult).eq(null);

    const rawPoolAdmin: any = await context.polkadotApi.query.rwaPermissions.poolAdmins(1);
    expect(rawPoolAdmin.unwrap().toHuman()).eq(poolAdmin.public);
  });

  it('should fail to grant PoolAdmin again for a pool that already has one', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.rwaPermissions.grantPermission(1, 'PoolAdmin', baltathar.address)
    ).signAndSend(alith, { nonce: alithNonce++ });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'sudo', 'sudo');
    expect(extrinsicResult).eq('AlreadyGranted');
  });

  it('should fail to revoke PoolAdmin from a non-root origin', async function () {
    await context.polkadotApi.tx.rwaPermissions.revokePermission(1, 'PoolAdmin', poolAdmin.public)
      .signAndSend(baltathar, { nonce: baltatharNonce++ });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'rwaPermissions', 'revokePermission');
    expect(extrinsicResult).eq('BadOrigin');
  });

  it('should fail to revoke the reserved Borrower role, even via sudo', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.rwaPermissions.revokePermission(1, 'Borrower', poolAdmin.public)
    ).signAndSend(alith, { nonce: alithNonce++ });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'sudo', 'sudo');
    expect(extrinsicResult).eq('BorrowerRoleReserved');
  });

  it('should successfully revoke PoolAdmin via sudo', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.rwaPermissions.revokePermission(1, 'PoolAdmin', poolAdmin.public)
    ).signAndSend(alith, { nonce: alithNonce++ });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'sudo', 'sudo');
    expect(extrinsicResult).eq(null);

    const rawPoolAdmin: any = await context.polkadotApi.query.rwaPermissions.poolAdmins(1);
    expect(rawPoolAdmin.isNone).eq(true);
  });

  it('should fail to revoke PoolAdmin again once already revoked', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.rwaPermissions.revokePermission(1, 'PoolAdmin', poolAdmin.public)
    ).signAndSend(alith, { nonce: alithNonce++ });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'sudo', 'sudo');
    expect(extrinsicResult).eq('NotGranted');
  });
});

describeDevNode('pallet_rwa_permissions - grant_permission / revoke_permission (OracleFeeder / TrancheInvestor)', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const poolAdmin: { public: string, private: string } = TEST_CONTROLLERS[1];
  const poolAdminSigner = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const borrower: { public: string, private: string } = TEST_CONTROLLERS[2];
  const outsider = keyring.addFromUri(TEST_CONTROLLERS[3].private);
  const feederA: { public: string, private: string } = TEST_CONTROLLERS[4];
  const feederB: { public: string, private: string } = TEST_CONTROLLERS[5];
  const investor: { public: string, private: string } = TEST_CONTROLLERS[6];
  let alithNonce: number;
  let poolAdminNonce: number;
  let outsiderNonce: number;

  before('should grant PoolAdmin and create a pool', async function () {
    const alithAccount = await context.polkadotApi.query.system.account(alith.address);
    alithNonce = alithAccount.nonce.toNumber();

    await grantPermission(context, alith, () => alithNonce++, 1, 'PoolAdmin', poolAdmin.public, true);
    await createTestPool(context, poolAdmin, borrower, 1);
  });

  beforeEach(async function () {
    const poolAdminAccount = await context.polkadotApi.query.system.account(poolAdminSigner.address);
    poolAdminNonce = poolAdminAccount.nonce.toNumber();
    const outsiderAccount = await context.polkadotApi.query.system.account(outsider.address);
    outsiderNonce = outsiderAccount.nonce.toNumber();
  });

  it('should fail to grant OracleFeeder from a non-pool-admin caller', async function () {
    await context.polkadotApi.tx.rwaPermissions.grantPermission(1, 'OracleFeeder', feederA.public)
      .signAndSend(outsider, { nonce: outsiderNonce++ });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'rwaPermissions', 'grantPermission');
    expect(extrinsicResult).eq('NotPoolAdmin');
  });

  it('should successfully grant OracleFeeder from the pool admin', async function () {
    await context.polkadotApi.tx.rwaPermissions.grantPermission(1, 'OracleFeeder', feederA.public)
      .signAndSend(poolAdminSigner, { nonce: poolAdminNonce++ });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'rwaPermissions', 'grantPermission');
    expect(extrinsicResult).eq(null);

    const rawFeeder: any = await context.polkadotApi.query.rwaPermissions.oracleFeeders(1, feederA.public);
    expect(rawFeeder.isSome).eq(true);
  });

  it('should fail to grant OracleFeeder again to the same account', async function () {
    await context.polkadotApi.tx.rwaPermissions.grantPermission(1, 'OracleFeeder', feederA.public)
      .signAndSend(poolAdminSigner, { nonce: poolAdminNonce++ });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'rwaPermissions', 'grantPermission');
    expect(extrinsicResult).eq('AlreadyGranted');
  });

  it('should successfully grant OracleFeeder to a second, distinct account', async function () {
    await context.polkadotApi.tx.rwaPermissions.grantPermission(1, 'OracleFeeder', feederB.public)
      .signAndSend(poolAdminSigner, { nonce: poolAdminNonce++ });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'rwaPermissions', 'grantPermission');
    expect(extrinsicResult).eq(null);

    const rawFeederA: any = await context.polkadotApi.query.rwaPermissions.oracleFeeders(1, feederA.public);
    const rawFeederB: any = await context.polkadotApi.query.rwaPermissions.oracleFeeders(1, feederB.public);
    expect(rawFeederA.isSome).eq(true);
    expect(rawFeederB.isSome).eq(true);
  });

  it('should fail to grant TrancheInvestor for an unknown tranche', async function () {
    await context.polkadotApi.tx.rwaPermissions.grantPermission(1, { TrancheInvestor: UNKNOWN_TRANCHE_ID }, investor.public)
      .signAndSend(poolAdminSigner, { nonce: poolAdminNonce++ });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'rwaPermissions', 'grantPermission');
    expect(extrinsicResult).eq('PoolOrTrancheNotFound');
  });

  it('should successfully grant TrancheInvestor for a real tranche', async function () {
    await context.polkadotApi.tx.rwaPermissions.grantPermission(1, { TrancheInvestor: TRANCHE_ID }, investor.public)
      .signAndSend(poolAdminSigner, { nonce: poolAdminNonce++ });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'rwaPermissions', 'grantPermission');
    expect(extrinsicResult).eq(null);

    const rawInvestor: any = await context.polkadotApi.query.rwaPermissions.trancheInvestors(TRANCHE_ID, investor.public);
    expect(rawInvestor.isSome).eq(true);
  });

  it('should fail to grant TrancheInvestor again for the same investor', async function () {
    await context.polkadotApi.tx.rwaPermissions.grantPermission(1, { TrancheInvestor: TRANCHE_ID }, investor.public)
      .signAndSend(poolAdminSigner, { nonce: poolAdminNonce++ });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'rwaPermissions', 'grantPermission');
    expect(extrinsicResult).eq('AlreadyGranted');
  });

  it('should fail to revoke OracleFeeder from a non-pool-admin caller', async function () {
    await context.polkadotApi.tx.rwaPermissions.revokePermission(1, 'OracleFeeder', feederA.public)
      .signAndSend(outsider, { nonce: outsiderNonce++ });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'rwaPermissions', 'revokePermission');
    expect(extrinsicResult).eq('NotPoolAdmin');
  });

  it('should successfully revoke TrancheInvestor', async function () {
    await context.polkadotApi.tx.rwaPermissions.revokePermission(1, { TrancheInvestor: TRANCHE_ID }, investor.public)
      .signAndSend(poolAdminSigner, { nonce: poolAdminNonce++ });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'rwaPermissions', 'revokePermission');
    expect(extrinsicResult).eq(null);

    const rawInvestor: any = await context.polkadotApi.query.rwaPermissions.trancheInvestors(TRANCHE_ID, investor.public);
    expect(rawInvestor.isNone).eq(true);
  });

  it('should fail to revoke TrancheInvestor again once already revoked', async function () {
    await context.polkadotApi.tx.rwaPermissions.revokePermission(1, { TrancheInvestor: TRANCHE_ID }, investor.public)
      .signAndSend(poolAdminSigner, { nonce: poolAdminNonce++ });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'rwaPermissions', 'revokePermission');
    expect(extrinsicResult).eq('NotGranted');
  });
});

describeDevNode('precompile_rwa_permissions - add_tranche_investor / remove_tranche_investor', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const poolAdmin: { public: string, private: string } = TEST_CONTROLLERS[1];
  const borrower: { public: string, private: string } = TEST_CONTROLLERS[2];
  const outsider: { public: string, private: string } = TEST_CONTROLLERS[3];
  const investor: { public: string, private: string } = TEST_CONTROLLERS[4];
  let alithNonce: number;

  before('should grant PoolAdmin and create a pool (Gateway address left unset)', async function () {
    const alithAccount = await context.polkadotApi.query.system.account(alith.address);
    alithNonce = alithAccount.nonce.toNumber();

    await grantPermission(context, alith, () => alithNonce++, 1, 'PoolAdmin', poolAdmin.public, true);
    await createTestPool(context, poolAdmin, borrower, 1);
  });

  function encodeArgs(investorId: string): string {
    return context.web3.eth.abi.encodeParameters(
      ['uint64', 'uint64', 'address', 'address'],
      [1, CHAIN_ID, VAULT_ADDRESS_A, investorId],
    );
  }

  it('should fail to add a tranche investor from a non-pool-admin caller', async function () {
    const block = await sendPrecompileTx(
      context, PERMISSIONS_PRECOMPILE_ADDRESS, PERMISSIONS_SELECTORS, outsider.public, outsider.private,
      'add_tranche_investor', [encodeArgs(investor.public)],
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(false);
  });

  it('should successfully add a tranche investor', async function () {
    const block = await sendPrecompileTx(
      context, PERMISSIONS_PRECOMPILE_ADDRESS, PERMISSIONS_SELECTORS, poolAdmin.public, poolAdmin.private,
      'add_tranche_investor', [encodeArgs(investor.public)],
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(true);

    const rawInvestor: any = await context.polkadotApi.query.rwaPermissions.trancheInvestors(TRANCHE_ID, investor.public);
    expect(rawInvestor.isSome).eq(true);
  });

  it('should fail to add the same tranche investor twice', async function () {
    const block = await sendPrecompileTx(
      context, PERMISSIONS_PRECOMPILE_ADDRESS, PERMISSIONS_SELECTORS, poolAdmin.public, poolAdmin.private,
      'add_tranche_investor', [encodeArgs(investor.public)],
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(false);
  });

  it('should successfully remove a tranche investor', async function () {
    const block = await sendPrecompileTx(
      context, PERMISSIONS_PRECOMPILE_ADDRESS, PERMISSIONS_SELECTORS, poolAdmin.public, poolAdmin.private,
      'remove_tranche_investor', [encodeArgs(investor.public)],
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(true);

    const rawInvestor: any = await context.polkadotApi.query.rwaPermissions.trancheInvestors(TRANCHE_ID, investor.public);
    expect(rawInvestor.isNone).eq(true);
  });

  it('should fail to remove a tranche investor that is not whitelisted', async function () {
    const block = await sendPrecompileTx(
      context, PERMISSIONS_PRECOMPILE_ADDRESS, PERMISSIONS_SELECTORS, poolAdmin.public, poolAdmin.private,
      'remove_tranche_investor', [encodeArgs(investor.public)],
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(false);
  });
});
