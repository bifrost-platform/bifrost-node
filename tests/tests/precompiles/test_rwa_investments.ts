import { expect } from 'chai';

import { Keyring } from '@polkadot/api';

import { TEST_CONTROLLERS } from '../../constants/keys';
import { describeDevNode, INodeContext } from '../set_dev_node';
import { sendPrecompileTx } from '../transactions';
import {
  advanceBlocksUntil, currentTimestampSecs, encodeCreatePool, getTranche, grantPermission,
  FIVE_PERCENT_APR, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, NFT_CONTRACT,
  POOLS_PRECOMPILE_ADDRESS, POOLS_SELECTORS, VAULT_ADDRESS_A
} from './rwa_helpers';

const CHAIN_ID = 1;
const TRANCHE_ID = { chainId: CHAIN_ID, vaultAddress: VAULT_ADDRESS_A };

function encodeSubmitOrder(context: INodeContext, poolId: number, investorId: string, amount: number): string {
  return context.web3.eth.abi.encodeParameters(
    ['uint64', 'uint64', 'address', 'address', 'uint256'],
    [poolId, CHAIN_ID, VAULT_ADDRESS_A, investorId, amount],
  );
}

function encodeCancelOrder(poolId: number, investorId: string, epochId: number): string[] {
  return [
    poolId.toString(16).padStart(64, '0'),
    CHAIN_ID.toString(16).padStart(64, '0'),
    VAULT_ADDRESS_A,
    investorId,
    epochId.toString(16).padStart(64, '0'),
  ];
}

async function createTestPool(
  context: INodeContext,
  alith: any,
  nonce: () => number,
  poolId: number,
  poolAdmin: { public: string, private: string },
  borrower: { public: string, private: string },
  epochLengthSecs: number,
  settlementOffsetSecs: number,
  depositApproval: boolean,
  redeemApproval: boolean,
) {
  await grantPermission(context, alith, nonce, poolId, 'PoolAdmin', poolAdmin.public, true);

  const data = encodeCreatePool(
    context, poolId, borrower.public, epochLengthSecs, settlementOffsetSecs, depositApproval, redeemApproval,
    [{ nftContract: NFT_CONTRACT, nftTokenId: poolId.toString() }],
    [{ chainId: CHAIN_ID, vaultAddress: VAULT_ADDRESS_A, isSenior: true, apr: FIVE_PERCENT_APR, maxDeposits: '0' }],
  );
  const block = await sendPrecompileTx(
    context, POOLS_PRECOMPILE_ADDRESS, POOLS_SELECTORS, poolAdmin.public, poolAdmin.private, 'create_pool', [data],
  );
  const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
  expect(Boolean(receipt.status)).eq(true);
}

describeDevNode('precompile_rwa_investments - submit_deposit_order / submit_redeem_order', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const poolAdmin: { public: string, private: string } = TEST_CONTROLLERS[1];
  const poolAdminSigner = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const borrower: { public: string, private: string } = TEST_CONTROLLERS[2];
  const gateway: { public: string, private: string } = TEST_CONTROLLERS[3];
  const investor: { public: string, private: string } = TEST_CONTROLLERS[4];
  const outsiderInvestor: { public: string, private: string } = TEST_CONTROLLERS[5];
  const outsiderCaller: { public: string, private: string } = TEST_CONTROLLERS[6];
  let alithNonce: number;
  let poolAdminNonce: number;

  before('should set the gateway, create a pool, and whitelist an investor', async function () {
    const alithAccount = await context.polkadotApi.query.system.account(alith.address);
    alithNonce = alithAccount.nonce.toNumber();

    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.rwaPools.setGateway(gateway.public)
    ).signAndSend(alith, { nonce: alithNonce++ });
    await context.createBlock();

    await createTestPool(context, alith, () => alithNonce++, 1, poolAdmin, borrower, 86_400, 3_600, true, true);

    const poolAdminAccount = await context.polkadotApi.query.system.account(poolAdminSigner.address);
    poolAdminNonce = poolAdminAccount.nonce.toNumber();
    await grantPermission(
      context, poolAdminSigner, () => poolAdminNonce++, 1, { TrancheInvestor: TRANCHE_ID }, investor.public, false,
    );
  });

  it('should fail to submit a deposit order from a non-gateway caller', async function () {
    const block = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, outsiderCaller.public, outsiderCaller.private,
      'submit_deposit_order', [encodeSubmitOrder(context, 1, investor.public, 1_000)],
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(false);
  });

  it('should fail to submit a deposit order for a non-whitelisted investor', async function () {
    const block = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'submit_deposit_order', [encodeSubmitOrder(context, 1, outsiderInvestor.public, 1_000)],
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(false);
  });

  it('should fail to submit a deposit order of zero amount', async function () {
    const block = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'submit_deposit_order', [encodeSubmitOrder(context, 1, investor.public, 0)],
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(false);
  });

  it('should successfully submit a deposit order', async function () {
    const block = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'submit_deposit_order', [encodeSubmitOrder(context, 1, investor.public, 1_000)],
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(true);

    const tranche = await getTranche(context, 1, CHAIN_ID, VAULT_ADDRESS_A);
    expect(BigInt(tranche.pendingOrders.deposit)).eq(1_000n);
  });

  it('should accumulate a top-up deposit order at the same epoch', async function () {
    const block = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'submit_deposit_order', [encodeSubmitOrder(context, 1, investor.public, 500)],
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(true);

    const tranche = await getTranche(context, 1, CHAIN_ID, VAULT_ADDRESS_A);
    expect(BigInt(tranche.pendingOrders.deposit)).eq(1_500n);
  });

  it('should successfully submit a redeem order', async function () {
    const block = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'submit_redeem_order', [encodeSubmitOrder(context, 1, investor.public, 200)],
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(true);

    const tranche = await getTranche(context, 1, CHAIN_ID, VAULT_ADDRESS_A);
    expect(BigInt(tranche.pendingOrders.redeem)).eq(200n);
  });

  it('should fail to submit an order for an unknown tranche', async function () {
    const data = context.web3.eth.abi.encodeParameters(
      ['uint64', 'uint64', 'address', 'address', 'uint256'],
      [1, 99, '0x' + '99'.repeat(20), investor.public, 1_000],
    );
    const block = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'submit_deposit_order', [data],
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(false);
  });
});

describeDevNode('precompile_rwa_investments - cancel_deposit_order / cancel_redeem_order', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const poolAdmin: { public: string, private: string } = TEST_CONTROLLERS[1];
  const poolAdminSigner = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const borrower: { public: string, private: string } = TEST_CONTROLLERS[2];
  const gateway: { public: string, private: string } = TEST_CONTROLLERS[3];
  const investor: { public: string, private: string } = TEST_CONTROLLERS[4];
  let alithNonce: number;
  let poolAdminNonce: number;

  before('should set up a pool with pending deposit and redeem orders', async function () {
    const alithAccount = await context.polkadotApi.query.system.account(alith.address);
    alithNonce = alithAccount.nonce.toNumber();

    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.rwaPools.setGateway(gateway.public)
    ).signAndSend(alith, { nonce: alithNonce++ });
    await context.createBlock();

    await createTestPool(context, alith, () => alithNonce++, 1, poolAdmin, borrower, 86_400, 3_600, true, true);

    const poolAdminAccount = await context.polkadotApi.query.system.account(poolAdminSigner.address);
    poolAdminNonce = poolAdminAccount.nonce.toNumber();
    await grantPermission(
      context, poolAdminSigner, () => poolAdminNonce++, 1, { TrancheInvestor: TRANCHE_ID }, investor.public, false,
    );

    let block = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'submit_deposit_order', [encodeSubmitOrder(context, 1, investor.public, 1_000)],
    );
    let receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(true);

    block = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'submit_redeem_order', [encodeSubmitOrder(context, 1, investor.public, 200)],
    );
    receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(true);
  });

  it('should successfully cancel a pending deposit order', async function () {
    const block = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'cancel_deposit_order', encodeCancelOrder(1, investor.public, 0),
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(true);

    const tranche = await getTranche(context, 1, CHAIN_ID, VAULT_ADDRESS_A);
    expect(BigInt(tranche.pendingOrders.deposit)).eq(0n);
  });

  it('should fail to cancel a deposit order that no longer exists', async function () {
    const block = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'cancel_deposit_order', encodeCancelOrder(1, investor.public, 0),
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(false);
  });

  it('should successfully cancel a pending redeem order', async function () {
    const block = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'cancel_redeem_order', encodeCancelOrder(1, investor.public, 0),
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(true);

    const tranche = await getTranche(context, 1, CHAIN_ID, VAULT_ADDRESS_A);
    expect(BigInt(tranche.pendingOrders.redeem)).eq(0n);
  });

  it('should fail to cancel a redeem order that no longer exists', async function () {
    const block = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'cancel_redeem_order', encodeCancelOrder(1, investor.public, 0),
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(false);
  });
});

describeDevNode('precompile_rwa_investments - approve_deposit_orders (Approval mode)', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const poolAdmin: { public: string, private: string } = TEST_CONTROLLERS[1];
  const poolAdminSigner = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const borrower: { public: string, private: string } = TEST_CONTROLLERS[2];
  const gateway: { public: string, private: string } = TEST_CONTROLLERS[3];
  const investor: { public: string, private: string } = TEST_CONTROLLERS[4];
  const oracleFeeder: { public: string, private: string } = TEST_CONTROLLERS[6];
  const EPOCH_LENGTH_SECS = 60;
  const SETTLEMENT_OFFSET_SECS = 10;
  let alithNonce: number;
  let poolAdminNonce: number;

  before('should set the gateway and whitelist an investor + oracle feeder for pool 1 (Approval) and pool 2 (Automatic)', async function () {
    const alithAccount = await context.polkadotApi.query.system.account(alith.address);
    alithNonce = alithAccount.nonce.toNumber();

    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.rwaPools.setGateway(gateway.public)
    ).signAndSend(alith, { nonce: alithNonce++ });
    await context.createBlock();

    // Pool 1: Approval mode — used for the main success/failure paths below.
    await createTestPool(
      context, alith, () => alithNonce++, 1, poolAdmin, borrower, EPOCH_LENGTH_SECS, SETTLEMENT_OFFSET_SECS, true, true,
    );
    // Pool 2: Automatic mode — used only for the WrongSettlementMode case.
    await grantPermission(context, alith, () => alithNonce++, 2, 'PoolAdmin', poolAdmin.public, true);
    const dataAutomatic = encodeCreatePool(
      context, 2, borrower.public, EPOCH_LENGTH_SECS, SETTLEMENT_OFFSET_SECS, false, false,
      [{ nftContract: '0x' + '44'.repeat(20), nftTokenId: '2' }],
      [{ chainId: 2, vaultAddress: '0x' + '55'.repeat(20), isSenior: true, apr: FIVE_PERCENT_APR, maxDeposits: '0' }],
    );
    const automaticBlock = await sendPrecompileTx(
      context, POOLS_PRECOMPILE_ADDRESS, POOLS_SELECTORS, poolAdmin.public, poolAdmin.private, 'create_pool', [dataAutomatic],
    );
    expect(Boolean((await context.web3.eth.getTransactionReceipt(automaticBlock.txResults[0])).status)).eq(true);

    const poolAdminAccount = await context.polkadotApi.query.system.account(poolAdminSigner.address);
    poolAdminNonce = poolAdminAccount.nonce.toNumber();
    await grantPermission(
      context, poolAdminSigner, () => poolAdminNonce++, 1, { TrancheInvestor: TRANCHE_ID }, investor.public, false,
    );
    await grantPermission(
      context, poolAdminSigner, () => poolAdminNonce++, 1, 'OracleFeeder', oracleFeeder.public, false,
    );

    const depositBlock = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'submit_deposit_order', [encodeSubmitOrder(context, 1, investor.public, 1_000)],
    );
    expect(Boolean((await context.web3.eth.getTransactionReceipt(depositBlock.txResults[0])).status)).eq(true);
  });

  function encodeApprove(poolId: number, investorIds: string[], epochIds: number[]): string {
    return context.web3.eth.abi.encodeParameters(
      ['uint64', 'uint64', 'address', 'address', 'address[]', 'uint64[]'],
      [poolId, CHAIN_ID, VAULT_ADDRESS_A, borrower.public, investorIds, epochIds],
    );
  }

  it('should fail with WrongSettlementMode on an Automatic-mode pool', async function () {
    const block = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'approve_deposit_orders', [encodeApprove(2, [investor.public], [0])],
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(false);
  });

  it('should fail with NotInSettlementWindow before the settlement window opens', async function () {
    const block = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'approve_deposit_orders', [encodeApprove(1, [investor.public], [0])],
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(false);
  });

  it('should fail with EpochPriceNotSet inside the window before the oracle submits earnings', async function () {
    const rawPool: any = await context.polkadotApi.query.rwaPools.pools(1);
    const pool = rawPool.unwrap().toJSON();
    const windowStartSecs = pool.epoch.epochStartSecs + EPOCH_LENGTH_SECS - SETTLEMENT_OFFSET_SECS;
    await advanceBlocksUntil(context, async () => (await currentTimestampSecs(context)) >= windowStartSecs);

    const block = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'approve_deposit_orders', [encodeApprove(1, [investor.public], [0])],
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(false);
  });

  it('should successfully approve a pending deposit order once NAV is finalized', async function () {
    // This pool's settlement window is now permanently missed for epoch 0 (see
    // feedback_rwa_pools_test_gotchas: on_initialize only attempts NAV finalization on the
    // first block that enters the window). Advance to epoch 1 and submit earnings *before*
    // its window opens this time.
    await advanceBlocksUntil(context, async () => {
      const rawPool: any = await context.polkadotApi.query.rwaPools.pools(1);
      return rawPool.unwrap().toJSON().epoch.currentEpoch === 1;
    });

    const oracleFeederSigner = keyring.addFromUri(TEST_CONTROLLERS[6].private);
    const feederNonce = (await context.polkadotApi.query.system.account(oracleFeederSigner.address)).nonce.toNumber();
    await context.polkadotApi.tx.rwaNavOracle.submitPnl(1, 1, 0, false).signAndSend(oracleFeederSigner, { nonce: feederNonce });
    await context.createBlock();

    const rawPool: any = await context.polkadotApi.query.rwaPools.pools(1);
    const pool = rawPool.unwrap().toJSON();
    const windowStartSecs = pool.epoch.epochStartSecs + EPOCH_LENGTH_SECS - SETTLEMENT_OFFSET_SECS;
    await advanceBlocksUntil(context, async () => (await currentTimestampSecs(context)) >= windowStartSecs);
    // `on_initialize` for block N reads the timestamp set by block N-1's inherent (inherents
    // apply after on_initialize within the same block), so one more block is needed for the
    // hook to actually observe `now >= windowStartSecs` and lock `epoch_price`.
    await context.createBlock();

    const trancheBefore = await getTranche(context, 1, CHAIN_ID, VAULT_ADDRESS_A);
    expect(trancheBefore.epochPrice).not.eq(null);

    const block = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'approve_deposit_orders', [encodeApprove(1, [investor.public], [0])],
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(true);

    const trancheAfter = await getTranche(context, 1, CHAIN_ID, VAULT_ADDRESS_A);
    expect(BigInt(trancheAfter.reserve)).eq(1_000n);
    expect(BigInt(trancheAfter.pendingOrders.deposit)).eq(0n);
    expect(BigInt(trancheAfter.tokenSupply) > 0n).eq(true);
  });

  it('should fail to approve the same order twice', async function () {
    const block = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'approve_deposit_orders', [encodeApprove(1, [investor.public], [0])],
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(false);
  });
});
