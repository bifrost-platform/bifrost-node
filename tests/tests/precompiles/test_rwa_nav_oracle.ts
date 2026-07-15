import { expect } from 'chai';

import { Keyring } from '@polkadot/api';

import { TEST_CONTROLLERS } from '../../constants/keys';
import { getExtrinsicResult } from '../extrinsics';
import { describeDevNode, INodeContext } from '../set_dev_node';
import { sendPrecompileTx } from '../transactions';
import {
  advanceBlocksUntil, encodeCreatePool, grantPermission, FIVE_PERCENT_APR, NFT_CONTRACT,
  POOLS_PRECOMPILE_ADDRESS, POOLS_SELECTORS, VAULT_ADDRESS_A
} from './rwa_helpers';

async function createTestPool(
  context: INodeContext,
  poolAdmin: { public: string, private: string },
  borrower: { public: string, private: string },
  poolId: number,
  epochLengthSecs: number,
  settlementOffsetSecs: number,
) {
  const data = encodeCreatePool(
    context, poolId, borrower.public, epochLengthSecs, settlementOffsetSecs, true, true,
    [{ nftContract: NFT_CONTRACT, nftTokenId: poolId.toString() }],
    [{ chainId: 1, vaultAddress: VAULT_ADDRESS_A, isSenior: true, apr: FIVE_PERCENT_APR, maxDeposits: '0' }],
  );
  const block = await sendPrecompileTx(
    context, POOLS_PRECOMPILE_ADDRESS, POOLS_SELECTORS, poolAdmin.public, poolAdmin.private, 'create_pool', [data],
  );
  const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
  expect(Boolean(receipt.status)).eq(true);
}

describeDevNode('pallet_rwa_nav_oracle - submit_earnings', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const poolAdmin: { public: string, private: string } = TEST_CONTROLLERS[1];
  const poolAdminSigner = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const borrower: { public: string, private: string } = TEST_CONTROLLERS[2];
  const feeder = keyring.addFromUri(TEST_CONTROLLERS[3].private);
  const feederPublic: { public: string, private: string } = TEST_CONTROLLERS[3];
  const outsider = keyring.addFromUri(TEST_CONTROLLERS[4].private);
  let alithNonce: number;
  let poolAdminNonce: number;
  let feederNonce: number;
  let outsiderNonce: number;

  before('should grant PoolAdmin, create a pool, and grant OracleFeeder', async function () {
    const alithAccount = await context.polkadotApi.query.system.account(alith.address);
    alithNonce = alithAccount.nonce.toNumber();

    await grantPermission(context, alith, () => alithNonce++, 1, 'PoolAdmin', poolAdmin.public, true);
    await createTestPool(context, poolAdmin, borrower, 1, 86_400, 3_600);

    const poolAdminAccount = await context.polkadotApi.query.system.account(poolAdminSigner.address);
    poolAdminNonce = poolAdminAccount.nonce.toNumber();
    await grantPermission(context, poolAdminSigner, () => poolAdminNonce++, 1, 'OracleFeeder', feederPublic.public, false);
  });

  beforeEach(async function () {
    feederNonce = (await context.polkadotApi.query.system.account(feeder.address)).nonce.toNumber();
    outsiderNonce = (await context.polkadotApi.query.system.account(outsider.address)).nonce.toNumber();
  });

  it('should fail to submit earnings for a pool that does not exist', async function () {
    await context.polkadotApi.tx.rwaNavOracle.submitEarnings(999, 0, 100)
      .signAndSend(feeder, { nonce: feederNonce++ });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'rwaNavOracle', 'submitEarnings');
    expect(extrinsicResult).eq('PoolNotFound');
  });

  it('should fail to submit earnings from a non-oracle-feeder caller', async function () {
    await context.polkadotApi.tx.rwaNavOracle.submitEarnings(1, 0, 100)
      .signAndSend(outsider, { nonce: outsiderNonce++ });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'rwaNavOracle', 'submitEarnings');
    expect(extrinsicResult).eq('Unauthorized');
  });

  it('should fail to submit earnings for an epoch that is not the current one', async function () {
    await context.polkadotApi.tx.rwaNavOracle.submitEarnings(1, 1, 100)
      .signAndSend(feeder, { nonce: feederNonce++ });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'rwaNavOracle', 'submitEarnings');
    expect(extrinsicResult).eq('InvalidEpochId');
  });

  it('should successfully submit earnings for the current epoch', async function () {
    await context.polkadotApi.tx.rwaNavOracle.submitEarnings(1, 0, 1_000)
      .signAndSend(feeder, { nonce: feederNonce++ });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'rwaNavOracle', 'submitEarnings');
    expect(extrinsicResult).eq(null);

    const rawEntry: any = await context.polkadotApi.query.rwaNavOracle.poolEarnings(1, 0);
    expect(BigInt(rawEntry.unwrap().cumulativeEarnings.toJSON())).eq(1_000n);
  });

  it('should allow an intra-epoch downward correction', async function () {
    await context.polkadotApi.tx.rwaNavOracle.submitEarnings(1, 0, 500)
      .signAndSend(feeder, { nonce: feederNonce++ });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'rwaNavOracle', 'submitEarnings');
    expect(extrinsicResult).eq(null);

    const rawEntry: any = await context.polkadotApi.query.rwaNavOracle.poolEarnings(1, 0);
    expect(BigInt(rawEntry.unwrap().cumulativeEarnings.toJSON())).eq(500n);
  });

  it('should allow an intra-epoch value to increase again', async function () {
    await context.polkadotApi.tx.rwaNavOracle.submitEarnings(1, 0, 2_000)
      .signAndSend(feeder, { nonce: feederNonce++ });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'rwaNavOracle', 'submitEarnings');
    expect(extrinsicResult).eq(null);

    const rawEntry: any = await context.polkadotApi.query.rwaNavOracle.poolEarnings(1, 0);
    expect(BigInt(rawEntry.unwrap().cumulativeEarnings.toJSON())).eq(2_000n);
  });
});

describeDevNode('pallet_rwa_nav_oracle - cross-epoch monotonicity and pruning', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const poolAdmin: { public: string, private: string } = TEST_CONTROLLERS[1];
  const poolAdminSigner = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const borrower: { public: string, private: string } = TEST_CONTROLLERS[2];
  const feeder = keyring.addFromUri(TEST_CONTROLLERS[3].private);
  const feederPublic: { public: string, private: string } = TEST_CONTROLLERS[3];
  // A short epoch is needed to reach epoch boundaries without excessive block production, but
  // too short and the on_initialize timestamp lag (see feedback_rwa_pools_test_gotchas #9) can
  // let the epoch advance twice within one block. 60s (proven stable in the investments suite)
  // leaves enough headroom.
  const EPOCH_LENGTH_SECS = 60;
  const SETTLEMENT_OFFSET_SECS = 10;
  let alithNonce: number;
  let poolAdminNonce: number;
  let feederNonce: number;
  let epochAtEpoch0Submit: number;
  let epochAtEpoch1Submit: number;

  before('should grant PoolAdmin, create a short-epoch pool, and grant OracleFeeder', async function () {
    const alithAccount = await context.polkadotApi.query.system.account(alith.address);
    alithNonce = alithAccount.nonce.toNumber();

    await grantPermission(context, alith, () => alithNonce++, 1, 'PoolAdmin', poolAdmin.public, true);
    await createTestPool(context, poolAdmin, borrower, 1, EPOCH_LENGTH_SECS, SETTLEMENT_OFFSET_SECS);

    const poolAdminAccount = await context.polkadotApi.query.system.account(poolAdminSigner.address);
    poolAdminNonce = poolAdminAccount.nonce.toNumber();
    await grantPermission(context, poolAdminSigner, () => poolAdminNonce++, 1, 'OracleFeeder', feederPublic.public, false);
  });

  beforeEach(async function () {
    feederNonce = (await context.polkadotApi.query.system.account(feeder.address)).nonce.toNumber();
  });

  async function currentEpoch(): Promise<number> {
    const rawPool: any = await context.polkadotApi.query.rwaPools.pools(1);
    return rawPool.unwrap().toJSON().epoch.currentEpoch;
  }

  it('should submit earnings for the pools starting epoch', async function () {
    epochAtEpoch0Submit = await currentEpoch();
    await context.polkadotApi.tx.rwaNavOracle.submitEarnings(1, epochAtEpoch0Submit, 1_000)
      .signAndSend(feeder, { nonce: feederNonce++ });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'rwaNavOracle', 'submitEarnings');
    expect(extrinsicResult).eq(null);
  });

  it('should fail to resubmit for that epoch once it is stale', async function () {
    await advanceBlocksUntil(context, async () => (await currentEpoch()) > epochAtEpoch0Submit);

    await context.polkadotApi.tx.rwaNavOracle.submitEarnings(1, epochAtEpoch0Submit, 999)
      .signAndSend(feeder, { nonce: feederNonce++ });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'rwaNavOracle', 'submitEarnings');
    expect(extrinsicResult).eq('InvalidEpochId');
  });

  it('should fail with EarningsDecreased when the new epoch is submitted below the prior epochs value', async function () {
    epochAtEpoch1Submit = await currentEpoch();
    await context.polkadotApi.tx.rwaNavOracle.submitEarnings(1, epochAtEpoch1Submit, 500)
      .signAndSend(feeder, { nonce: feederNonce++ });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'rwaNavOracle', 'submitEarnings');
    expect(extrinsicResult).eq('EarningsDecreased');
  });

  it('should successfully submit for that epoch at or above the prior epochs value', async function () {
    epochAtEpoch1Submit = await currentEpoch();
    await context.polkadotApi.tx.rwaNavOracle.submitEarnings(1, epochAtEpoch1Submit, 1_500)
      .signAndSend(feeder, { nonce: feederNonce++ });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'rwaNavOracle', 'submitEarnings');
    expect(extrinsicResult).eq(null);

    const rawEpoch0: any = await context.polkadotApi.query.rwaNavOracle.poolEarnings(1, epochAtEpoch0Submit);
    expect(rawEpoch0.isSome).eq(true);
    const rawEpoch1: any = await context.polkadotApi.query.rwaNavOracle.poolEarnings(1, epochAtEpoch1Submit);
    expect(BigInt(rawEpoch1.unwrap().cumulativeEarnings.toJSON())).eq(1_500n);
  });

  it('should prune the epoch two submissions back once a new one lands', async function () {
    await advanceBlocksUntil(context, async () => (await currentEpoch()) > epochAtEpoch1Submit);
    const epochAtEpoch2Submit = await currentEpoch();

    await context.polkadotApi.tx.rwaNavOracle.submitEarnings(1, epochAtEpoch2Submit, 2_000)
      .signAndSend(feeder, { nonce: feederNonce++ });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'rwaNavOracle', 'submitEarnings');
    expect(extrinsicResult).eq(null);

    const rawEpoch0: any = await context.polkadotApi.query.rwaNavOracle.poolEarnings(1, epochAtEpoch0Submit);
    expect(rawEpoch0.isNone).eq(true);
    const rawEpoch1: any = await context.polkadotApi.query.rwaNavOracle.poolEarnings(1, epochAtEpoch1Submit);
    expect(rawEpoch1.isSome).eq(true);
    const rawEpoch2: any = await context.polkadotApi.query.rwaNavOracle.poolEarnings(1, epochAtEpoch2Submit);
    expect(BigInt(rawEpoch2.unwrap().cumulativeEarnings.toJSON())).eq(2_000n);
  });
});
