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

function encodeClaim(poolId: number, investorId: string): string[] {
  return [
    poolId.toString(16).padStart(64, '0'),
    CHAIN_ID.toString(16).padStart(64, '0'),
    VAULT_ADDRESS_A,
    investorId,
  ];
}

async function createAutomaticPool(
  context: INodeContext,
  poolAdmin: { public: string, private: string },
  borrower: { public: string, private: string },
  poolId: number,
) {
  const data = encodeCreatePool(
    context, poolId, borrower.public, EPOCH_LENGTH_SECS, SETTLEMENT_OFFSET_SECS, false, false,
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

describeDevNode('pallet_rwa_pools / precompile_rwa_investments - Automatic-mode deposit settlement and claim_shares', (context) => {
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
  let settledEpoch: number;

  before('should create an Automatic-mode pool, whitelist an investor, and settle a deposit order', async function () {
    const alithAccount = await context.polkadotApi.query.system.account(alith.address);
    alithNonce = alithAccount.nonce.toNumber();

    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.rwaPools.setGateway(gateway.public)
    ).signAndSend(alith, { nonce: alithNonce++ });
    await context.createBlock();

    await grantPermission(context, alith, () => alithNonce++, 1, 'PoolAdmin', poolAdmin.public, true);
    await createAutomaticPool(context, poolAdmin, borrower, 1);

    const poolAdminAccount = await context.polkadotApi.query.system.account(poolAdminSigner.address);
    poolAdminNonce = poolAdminAccount.nonce.toNumber();
    await grantPermission(context, poolAdminSigner, () => poolAdminNonce++, 1, { TrancheInvestor: TRANCHE_ID }, investor.public, false);
    await grantPermission(context, poolAdminSigner, () => poolAdminNonce++, 1, 'OracleFeeder', feederPublic.public, false);

    const depositBlock = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'submit_deposit_order', [encodeOrder(context, 1, investor.public, 2_000)],
    );
    expect(Boolean((await context.web3.eth.getTransactionReceipt(depositBlock.txResults[0])).status)).eq(true);

    settledEpoch = await currentEpoch(context, 1);
    const feederNonce = (await context.polkadotApi.query.system.account(feeder.address)).nonce.toNumber();
    await context.polkadotApi.tx.rwaNavOracle.submitEarnings(1, settledEpoch, 0).signAndSend(feeder, { nonce: feederNonce });
    await context.createBlock();

    await advanceToSettlementWindow(context, 1, EPOCH_LENGTH_SECS, SETTLEMENT_OFFSET_SECS);
  });

  it('should have auto-settled the pending deposit order into ClaimableDepositOrders', async function () {
    const rawPending: any = await context.polkadotApi.query.rwaInvestments.pendingDepositOrders(TRANCHE_ID, investor.public, settledEpoch);
    expect(rawPending.isNone).eq(true);

    const rawClaimable: any = await context.polkadotApi.query.rwaInvestments.claimableDepositOrders(TRANCHE_ID, investor.public, settledEpoch);
    const claimable = rawClaimable.unwrap();
    expect(BigInt(claimable.amount.toJSON())).eq(2_000n);
    expect(BigInt(claimable.sharesToMint.toJSON())).eq(2_000n); // first-ever mint: price = WAD, so shares == assets

    const tranche = await getTranche(context, 1, CHAIN_ID, VAULT_ADDRESS_A);
    expect(BigInt(tranche.reserve)).eq(2_000n);
    expect(BigInt(tranche.tokenSupply)).eq(2_000n);
    expect(BigInt(tranche.accruedNav)).eq(2_000n);
    expect(BigInt(tranche.pendingOrders.deposit)).eq(0n);
  });

  it('should fail to claim shares from a non-gateway caller', async function () {
    const block = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, investor.public, investor.private,
      'claim_shares', encodeClaim(1, investor.public),
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(false);
  });

  it('should successfully claim shares via the gateway', async function () {
    const block = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'claim_shares', encodeClaim(1, investor.public),
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(true);

    const rawClaimable: any = await context.polkadotApi.query.rwaInvestments.claimableDepositOrders(TRANCHE_ID, investor.public, settledEpoch);
    expect(rawClaimable.isNone).eq(true);

    const claimEpoch = await currentEpoch(context, 1);
    const rawApproved: any = await context.polkadotApi.query.rwaInvestments.approvedDepositOrders(TRANCHE_ID, investor.public, claimEpoch);
    const approved = rawApproved.unwrap();
    expect(BigInt(approved.amount.toJSON())).eq(2_000n);
    expect(BigInt(approved.sharesToMint.toJSON())).eq(2_000n);
  });

  it('should fail to claim shares again once already claimed', async function () {
    const block = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'claim_shares', encodeClaim(1, investor.public),
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(false);
  });
});

describeDevNode('pallet_rwa_pools / precompile_rwa_investments - Automatic-mode redeem settlement and claim_assets', (context) => {
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
  let depositEpoch: number;
  let redeemEpoch: number;
  let reserveBeforeRedeem: bigint;
  let tokenSupplyBeforeRedeem: bigint;

  before('should fund the treasury reserve with a settled deposit in epoch 0', async function () {
    const alithAccount = await context.polkadotApi.query.system.account(alith.address);
    alithNonce = alithAccount.nonce.toNumber();

    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.rwaPools.setGateway(gateway.public)
    ).signAndSend(alith, { nonce: alithNonce++ });
    await context.createBlock();

    await grantPermission(context, alith, () => alithNonce++, 1, 'PoolAdmin', poolAdmin.public, true);
    await createAutomaticPool(context, poolAdmin, borrower, 1);

    const poolAdminAccount = await context.polkadotApi.query.system.account(poolAdminSigner.address);
    poolAdminNonce = poolAdminAccount.nonce.toNumber();
    await grantPermission(context, poolAdminSigner, () => poolAdminNonce++, 1, { TrancheInvestor: TRANCHE_ID }, investor.public, false);
    await grantPermission(context, poolAdminSigner, () => poolAdminNonce++, 1, 'OracleFeeder', feederPublic.public, false);

    const depositBlock = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'submit_deposit_order', [encodeOrder(context, 1, investor.public, 2_000)],
    );
    expect(Boolean((await context.web3.eth.getTransactionReceipt(depositBlock.txResults[0])).status)).eq(true);

    depositEpoch = await currentEpoch(context, 1);
    let feederNonce = (await context.polkadotApi.query.system.account(feeder.address)).nonce.toNumber();
    await context.polkadotApi.tx.rwaNavOracle.submitEarnings(1, depositEpoch, 0).signAndSend(feeder, { nonce: feederNonce++ });
    await context.createBlock();

    await advanceToSettlementWindow(context, 1, EPOCH_LENGTH_SECS, SETTLEMENT_OFFSET_SECS);

    const trancheAfterDeposit = await getTranche(context, 1, CHAIN_ID, VAULT_ADDRESS_A);
    expect(BigInt(trancheAfterDeposit.reserve)).eq(2_000n);

    // Move into the next epoch, submit a redeem order and this epoch's earnings *before* its
    // settlement window opens, then let it settle.
    await advanceBlocksUntil(context, async () => (await currentEpoch(context, 1)) > depositEpoch);

    const redeemBlock = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'submit_redeem_order', [encodeOrder(context, 1, investor.public, 500)],
    );
    expect(Boolean((await context.web3.eth.getTransactionReceipt(redeemBlock.txResults[0])).status)).eq(true);

    redeemEpoch = await currentEpoch(context, 1);
    feederNonce = (await context.polkadotApi.query.system.account(feeder.address)).nonce.toNumber();
    await context.polkadotApi.tx.rwaNavOracle.submitEarnings(1, redeemEpoch, 0).signAndSend(feeder, { nonce: feederNonce });
    await context.createBlock();

    const trancheBeforeRedeemSettles = await getTranche(context, 1, CHAIN_ID, VAULT_ADDRESS_A);
    reserveBeforeRedeem = BigInt(trancheBeforeRedeemSettles.reserve);
    tokenSupplyBeforeRedeem = BigInt(trancheBeforeRedeemSettles.tokenSupply);

    await advanceToSettlementWindow(context, 1, EPOCH_LENGTH_SECS, SETTLEMENT_OFFSET_SECS);
  });

  it('should have auto-settled the pending redeem order into ClaimableRedeemOrders', async function () {
    const rawPending: any = await context.polkadotApi.query.rwaInvestments.pendingRedeemOrders(TRANCHE_ID, investor.public, redeemEpoch);
    expect(rawPending.isNone).eq(true);

    const tranche = await getTranche(context, 1, CHAIN_ID, VAULT_ADDRESS_A);
    const epochPrice = BigInt(tranche.epochPrice);
    const expectedPayout = (500n * epochPrice) / WAD;

    const rawClaimable: any = await context.polkadotApi.query.rwaInvestments.claimableRedeemOrders(TRANCHE_ID, investor.public, redeemEpoch);
    const claimable = rawClaimable.unwrap();
    expect(BigInt(claimable.sharesRedeemed.toJSON())).eq(500n);
    expect(BigInt(claimable.payout.toJSON())).eq(expectedPayout);

    expect(BigInt(tranche.reserve)).eq(reserveBeforeRedeem - expectedPayout);
    expect(BigInt(tranche.tokenSupply)).eq(tokenSupplyBeforeRedeem - 500n);
    expect(BigInt(tranche.pendingOrders.redeem)).eq(0n);
  });

  it('should fail to claim assets from a non-gateway caller', async function () {
    const block = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, investor.public, investor.private,
      'claim_assets', encodeClaim(1, investor.public),
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(false);
  });

  it('should successfully claim assets via the gateway', async function () {
    const tranche = await getTranche(context, 1, CHAIN_ID, VAULT_ADDRESS_A);
    const epochPrice = BigInt(tranche.epochPrice);
    const expectedPayout = (500n * epochPrice) / WAD;

    const block = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'claim_assets', encodeClaim(1, investor.public),
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(true);

    const rawClaimable: any = await context.polkadotApi.query.rwaInvestments.claimableRedeemOrders(TRANCHE_ID, investor.public, redeemEpoch);
    expect(rawClaimable.isNone).eq(true);

    const claimEpoch = await currentEpoch(context, 1);
    const rawApproved: any = await context.polkadotApi.query.rwaInvestments.approvedRedeemOrders(TRANCHE_ID, investor.public, claimEpoch);
    const approved = rawApproved.unwrap();
    expect(BigInt(approved.sharesRedeemed.toJSON())).eq(500n);
    expect(BigInt(approved.payout.toJSON())).eq(expectedPayout);
  });

  it('should fail to claim assets again once already claimed', async function () {
    const block = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'claim_assets', encodeClaim(1, investor.public),
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(false);
  });
});
