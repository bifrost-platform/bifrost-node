import { expect } from 'chai';

import { Keyring } from '@polkadot/api';

import { TEST_CONTROLLERS } from '../../constants/keys';
import { describeDevNode, INodeContext } from '../set_dev_node';
import { sendPrecompileTx } from '../transactions';
import {
  advanceBlocksUntil, advanceToSettlementWindow, encodeCreatePool, getTranche, grantPermission,
  FIVE_PERCENT_APR, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, NFT_CONTRACT,
  POOLS_PRECOMPILE_ADDRESS, POOLS_SELECTORS, VAULT_ADDRESS_A
} from './rwa_helpers';

const CHAIN_ID = 1;
const TRANCHE_ID = { chainId: CHAIN_ID, vaultAddress: VAULT_ADDRESS_A };
const EPOCH_LENGTH_SECS = 60;
const SETTLEMENT_OFFSET_SECS = 10;
const WAD = 1_000_000_000_000_000_000n;

function encodeOrder(context: INodeContext, poolId: number, investorId: string, amount: number): string {
  return context.web3.eth.abi.encodeParameters(
    ['uint64', 'uint64', 'address', 'address', 'uint256'],
    [poolId, CHAIN_ID, VAULT_ADDRESS_A, investorId, amount],
  );
}

function encodeApprove(context: INodeContext, poolId: number, borrower: string, investorIds: string[], epochIds: number[]): string {
  return context.web3.eth.abi.encodeParameters(
    ['uint64', 'uint64', 'address', 'address', 'address[]', 'uint64[]'],
    [poolId, CHAIN_ID, VAULT_ADDRESS_A, borrower, investorIds, epochIds],
  );
}

async function createApprovalPool(
  context: INodeContext,
  poolAdmin: { public: string, private: string },
  borrower: { public: string, private: string },
  poolId: number,
) {
  const data = encodeCreatePool(
    context, poolId, borrower.public, EPOCH_LENGTH_SECS, SETTLEMENT_OFFSET_SECS, true, true,
    [{ nftContract: NFT_CONTRACT, nftTokenId: poolId.toString() }],
    [{ chainId: CHAIN_ID, vaultAddress: VAULT_ADDRESS_A, isSenior: true, apr: FIVE_PERCENT_APR, maxDeposits: '0' }],
  );
  const block = await sendPrecompileTx(
    context, POOLS_PRECOMPILE_ADDRESS, POOLS_SELECTORS, poolAdmin.public, poolAdmin.private, 'create_pool', [data],
  );
  const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
  expect(Boolean(receipt.status)).eq(true);
}

async function currentEpoch(context: INodeContext, poolId: number): Promise<number> {
  const rawPool: any = await context.polkadotApi.query.rwaPools.pools(poolId);
  return rawPool.unwrap().toJSON().epoch.currentEpoch;
}

describeDevNode('precompile_rwa_investments - approve_redeem_orders (Approval mode, success path)', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const poolAdmin: { public: string, private: string } = TEST_CONTROLLERS[1];
  const poolAdminSigner = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const borrower: { public: string, private: string } = TEST_CONTROLLERS[2];
  const gateway: { public: string, private: string } = TEST_CONTROLLERS[3];
  const investor: { public: string, private: string } = TEST_CONTROLLERS[4];
  const feeder = keyring.addFromUri(TEST_CONTROLLERS[5].private);
  const feederPublic: { public: string, private: string } = TEST_CONTROLLERS[5];
  const SENIOR_DEPOSIT = 2_000;
  const REDEEM_AMOUNT = 500;
  let alithNonce: number;
  let poolAdminNonce: number;
  let depositEpoch: number;
  let redeemEpoch: number;
  let reserveBeforeApproval: bigint;
  let tokenSupplyBeforeApproval: bigint;

  before('should fund the reserve via an approved epoch-0 deposit', async function () {
    const alithAccount = await context.polkadotApi.query.system.account(alith.address);
    alithNonce = alithAccount.nonce.toNumber();

    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.rwaPools.setGateway(gateway.public)
    ).signAndSend(alith, { nonce: alithNonce++ });
    await context.createBlock();

    await grantPermission(context, alith, () => alithNonce++, 1, 'PoolAdmin', poolAdmin.public, true);
    await createApprovalPool(context, poolAdmin, borrower, 1);

    const poolAdminAccount = await context.polkadotApi.query.system.account(poolAdminSigner.address);
    poolAdminNonce = poolAdminAccount.nonce.toNumber();
    await grantPermission(context, poolAdminSigner, () => poolAdminNonce++, 1, { TrancheInvestor: TRANCHE_ID }, investor.public, false);
    await grantPermission(context, poolAdminSigner, () => poolAdminNonce++, 1, 'OracleFeeder', feederPublic.public, false);

    const depositBlock = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'submit_deposit_order', [encodeOrder(context, 1, investor.public, SENIOR_DEPOSIT)],
    );
    expect(Boolean((await context.web3.eth.getTransactionReceipt(depositBlock.txResults[0])).status)).eq(true);

    depositEpoch = await currentEpoch(context, 1);
    let feederNonce = (await context.polkadotApi.query.system.account(feeder.address)).nonce.toNumber();
    await context.polkadotApi.tx.rwaNavOracle.submitPnl(1, depositEpoch, 0, false).signAndSend(feeder, { nonce: feederNonce++ });
    await context.createBlock();

    await advanceToSettlementWindow(context, 1, EPOCH_LENGTH_SECS, SETTLEMENT_OFFSET_SECS);

    const approveDepositBlock = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'approve_deposit_orders', [encodeApprove(context, 1, borrower.public, [investor.public], [depositEpoch])],
    );
    expect(Boolean((await context.web3.eth.getTransactionReceipt(approveDepositBlock.txResults[0])).status)).eq(true);

    const trancheAfterDeposit = await getTranche(context, 1, CHAIN_ID, VAULT_ADDRESS_A);
    expect(BigInt(trancheAfterDeposit.reserve)).eq(BigInt(SENIOR_DEPOSIT));

    // Move to the next epoch and submit a redeem order + this epoch's earnings *before* its
    // settlement window opens (see feedback_rwa_pools_test_gotchas #9).
    await advanceBlocksUntil(context, async () => (await currentEpoch(context, 1)) > depositEpoch);

    const redeemBlock = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'submit_redeem_order', [encodeOrder(context, 1, investor.public, REDEEM_AMOUNT)],
    );
    expect(Boolean((await context.web3.eth.getTransactionReceipt(redeemBlock.txResults[0])).status)).eq(true);

    redeemEpoch = await currentEpoch(context, 1);
    feederNonce = (await context.polkadotApi.query.system.account(feeder.address)).nonce.toNumber();
    await context.polkadotApi.tx.rwaNavOracle.submitPnl(1, redeemEpoch, 0, false).signAndSend(feeder, { nonce: feederNonce });
    await context.createBlock();

    const trancheBeforeApproval = await getTranche(context, 1, CHAIN_ID, VAULT_ADDRESS_A);
    reserveBeforeApproval = BigInt(trancheBeforeApproval.reserve);
    tokenSupplyBeforeApproval = BigInt(trancheBeforeApproval.tokenSupply);

    await advanceToSettlementWindow(context, 1, EPOCH_LENGTH_SECS, SETTLEMENT_OFFSET_SECS);
  });

  it('should successfully approve the pending redeem order', async function () {
    const tranche = await getTranche(context, 1, CHAIN_ID, VAULT_ADDRESS_A);
    const epochPrice = BigInt(tranche.epochPrice);
    const expectedPayout = (BigInt(REDEEM_AMOUNT) * epochPrice) / WAD;

    const block = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'approve_redeem_orders', [encodeApprove(context, 1, borrower.public, [investor.public], [redeemEpoch])],
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(true);

    const rawPending: any = await context.polkadotApi.query.rwaInvestments.pendingRedeemOrders(TRANCHE_ID, investor.public, redeemEpoch);
    expect(rawPending.isNone).eq(true);

    const rawApproved: any = await context.polkadotApi.query.rwaInvestments.approvedRedeemOrders(TRANCHE_ID, investor.public, redeemEpoch);
    const approved = rawApproved.unwrap();
    expect(BigInt(approved.sharesRedeemed.toJSON())).eq(BigInt(REDEEM_AMOUNT));
    expect(BigInt(approved.payout.toJSON())).eq(expectedPayout);

    const trancheAfter = await getTranche(context, 1, CHAIN_ID, VAULT_ADDRESS_A);
    expect(BigInt(trancheAfter.reserve)).eq(reserveBeforeApproval - expectedPayout);
    expect(BigInt(trancheAfter.tokenSupply)).eq(tokenSupplyBeforeApproval - BigInt(REDEEM_AMOUNT));
    expect(BigInt(trancheAfter.pendingOrders.redeem)).eq(0n);
  });

  it('should fail to approve the same order again (PendingOrderNotFound)', async function () {
    const block = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'approve_redeem_orders', [encodeApprove(context, 1, borrower.public, [investor.public], [redeemEpoch])],
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(false);
  });
});

describeDevNode('precompile_rwa_investments - approve_redeem_orders (Approval mode, error paths)', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const poolAdmin: { public: string, private: string } = TEST_CONTROLLERS[1];
  const poolAdminSigner = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const borrower: { public: string, private: string } = TEST_CONTROLLERS[2];
  const gateway: { public: string, private: string } = TEST_CONTROLLERS[3];
  const investor: { public: string, private: string } = TEST_CONTROLLERS[4];
  const feeder = keyring.addFromUri(TEST_CONTROLLERS[5].private);
  const feederPublic: { public: string, private: string } = TEST_CONTROLLERS[5];
  let alithNonce: number;
  let poolAdminNonce: number;
  let submittedEpoch: number;

  before('should create a pool with an unfunded reserve and a pending redeem order', async function () {
    const alithAccount = await context.polkadotApi.query.system.account(alith.address);
    alithNonce = alithAccount.nonce.toNumber();

    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.rwaPools.setGateway(gateway.public)
    ).signAndSend(alith, { nonce: alithNonce++ });
    await context.createBlock();

    await grantPermission(context, alith, () => alithNonce++, 2, 'PoolAdmin', poolAdmin.public, true);
    await createApprovalPool(context, poolAdmin, borrower, 2);

    const poolAdminAccount = await context.polkadotApi.query.system.account(poolAdminSigner.address);
    poolAdminNonce = poolAdminAccount.nonce.toNumber();
    await grantPermission(context, poolAdminSigner, () => poolAdminNonce++, 2, { TrancheInvestor: TRANCHE_ID }, investor.public, false);
    await grantPermission(context, poolAdminSigner, () => poolAdminNonce++, 2, 'OracleFeeder', feederPublic.public, false);

    // No deposit is ever settled here, so this tranche's reserve stays at zero — any nonzero
    // redeem order will exceed available liquidity once approved.
    const redeemBlock = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'submit_redeem_order', [encodeOrder(context, 2, investor.public, 100)],
    );
    expect(Boolean((await context.web3.eth.getTransactionReceipt(redeemBlock.txResults[0])).status)).eq(true);

    submittedEpoch = await currentEpoch(context, 2);
    const feederNonce = (await context.polkadotApi.query.system.account(feeder.address)).nonce.toNumber();
    await context.polkadotApi.tx.rwaNavOracle.submitPnl(2, submittedEpoch, 0, false).signAndSend(feeder, { nonce: feederNonce });
    await context.createBlock();

    await advanceToSettlementWindow(context, 2, EPOCH_LENGTH_SECS, SETTLEMENT_OFFSET_SECS);
  });

  it('should fail with a duplicate order key in the same batch', async function () {
    const block = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'approve_redeem_orders',
      [encodeApprove(context, 2, borrower.public, [investor.public, investor.public], [submittedEpoch, submittedEpoch])],
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(false);
  });

  it('should fail with InsufficientLiquidity when the tranche reserve cannot cover the payout', async function () {
    const tranche = await getTranche(context, 2, CHAIN_ID, VAULT_ADDRESS_A);
    expect(BigInt(tranche.reserve)).eq(0n);

    const block = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'approve_redeem_orders', [encodeApprove(context, 2, borrower.public, [investor.public], [submittedEpoch])],
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(false);

    // The pre-flight liquidity check runs before any state mutation, so the order must still
    // be pending.
    const rawPending: any = await context.polkadotApi.query.rwaInvestments.pendingRedeemOrders(TRANCHE_ID, investor.public, submittedEpoch);
    expect(rawPending.isSome).eq(true);
  });
});
