import { expect } from 'chai';

import { Keyring } from '@polkadot/api';
import { numberToHex } from 'web3-utils';

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
    await context.polkadotApi.tx.rwaNavOracle.submitPnl(1, settledEpoch, 0, false).signAndSend(feeder, { nonce: feederNonce });
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
    await context.polkadotApi.tx.rwaNavOracle.submitPnl(1, depositEpoch, 0, false).signAndSend(feeder, { nonce: feederNonce++ });
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
    await context.polkadotApi.tx.rwaNavOracle.submitPnl(1, redeemEpoch, 0, false).signAndSend(feeder, { nonce: feederNonce });
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

describeDevNode('pallet_rwa_pools / precompile_rwa_investments - Automatic-mode deposit deferral at zero epoch_price', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const poolAdmin: { public: string, private: string } = TEST_CONTROLLERS[1];
  const poolAdminSigner = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const borrower: { public: string, private: string } = TEST_CONTROLLERS[2];
  const gateway: { public: string, private: string } = TEST_CONTROLLERS[3];
  const investorA: { public: string, private: string } = TEST_CONTROLLERS[4];
  const investorB: { public: string, private: string } = TEST_CONTROLLERS[6];
  const feeder = keyring.addFromUri(TEST_CONTROLLERS[5].private);
  const feederPublic: { public: string, private: string } = TEST_CONTROLLERS[5];
  let alithNonce: number;
  let poolAdminNonce: number;
  let epoch1: number;
  let epoch2: number;

  before('should create a pool, settle an initial deposit at par, and borrow the reserve out entirely', async function () {
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
    await grantPermission(context, poolAdminSigner, () => poolAdminNonce++, 1, { TrancheInvestor: TRANCHE_ID }, investorA.public, false);
    await grantPermission(context, poolAdminSigner, () => poolAdminNonce++, 1, { TrancheInvestor: TRANCHE_ID }, investorB.public, false);
    await grantPermission(context, poolAdminSigner, () => poolAdminNonce++, 1, 'OracleFeeder', feederPublic.public, false);

    const depositBlock = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'submit_deposit_order', [encodeOrder(context, 1, investorA.public, 1_000)],
    );
    expect(Boolean((await context.web3.eth.getTransactionReceipt(depositBlock.txResults[0])).status)).eq(true);

    const epoch0 = await currentEpoch(context, 1);
    const feederNonce0 = (await context.polkadotApi.query.system.account(feeder.address)).nonce.toNumber();
    await context.polkadotApi.tx.rwaNavOracle.submitPnl(1, epoch0, 0, false).signAndSend(feeder, { nonce: feederNonce0 });
    await context.createBlock();

    await advanceToSettlementWindow(context, 1, EPOCH_LENGTH_SECS, SETTLEMENT_OFFSET_SECS);

    const tranche = await getTranche(context, 1, CHAIN_ID, VAULT_ADDRESS_A);
    expect(BigInt(tranche.reserve)).eq(1_000n);
    expect(BigInt(tranche.tokenSupply)).eq(1_000n);
    expect(BigInt(tranche.accruedNav)).eq(1_000n);

    // Borrow the full reserve out so a subsequent loss can drive oracle_nav (and thus this
    // tranche's price) all the way to exactly 0, with no reserve cushion left to absorb it.
    const borrowBlock = await sendPrecompileTx(
      context, POOLS_PRECOMPILE_ADDRESS, POOLS_SELECTORS, gateway.public, gateway.private, 'borrow',
      [numberToHex(1), numberToHex(CHAIN_ID), VAULT_ADDRESS_A, borrower.public, numberToHex(1_000)],
    );
    expect(Boolean((await context.web3.eth.getTransactionReceipt(borrowBlock.txResults[0])).status)).eq(true);
  });

  it('should defer a deposit that would settle at a zero epoch_price', async function () {
    await advanceBlocksUntil(context, async () => (await currentEpoch(context, 1)) > 0);
    epoch1 = await currentEpoch(context, 1);

    const investorBDepositBlock = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'submit_deposit_order', [encodeOrder(context, 1, investorB.public, 200)],
    );
    expect(Boolean((await context.web3.eth.getTransactionReceipt(investorBDepositBlock.txResults[0])).status)).eq(true);

    const feederNonce = (await context.polkadotApi.query.system.account(feeder.address)).nonce.toNumber();
    await context.polkadotApi.tx.rwaNavOracle.submitPnl(1, epoch1, 1_000, true).signAndSend(feeder, { nonce: feederNonce });
    await context.createBlock();

    await advanceToSettlementWindow(context, 1, EPOCH_LENGTH_SECS, SETTLEMENT_OFFSET_SECS);

    // oracle_nav = max(0, total_borrowed(1_000) - loss(1_000)) = 0; total_reserve = 0 (fully
    // borrowed out) => total_pool_value = 0 => senior claims min(0, accrued_nav) = 0 => price 0.
    const tranche = await getTranche(context, 1, CHAIN_ID, VAULT_ADDRESS_A);
    expect(BigInt(tranche.epochPrice)).eq(0n);
    expect(BigInt(tranche.reserve)).eq(0n); // investor B's 200 must NOT have been absorbed
    expect(BigInt(tranche.tokenSupply)).eq(1_000n); // unchanged — no 0-share mint happened
    expect(BigInt(tranche.pendingOrders.deposit)).eq(200n); // still pending, not cleared

    const rawPending: any = await context.polkadotApi.query.rwaInvestments.pendingDepositOrders(TRANCHE_ID, investorB.public, epoch1);
    expect(rawPending.isSome).eq(true);
    expect(BigInt(rawPending.unwrap().amount.toJSON())).eq(200n);

    const rawClaimable: any = await context.polkadotApi.query.rwaInvestments.claimableDepositOrders(TRANCHE_ID, investorB.public, epoch1);
    expect(rawClaimable.isNone).eq(true);
  });

  it('should settle the deferred deposit once a later epochs price is nonzero', async function () {
    await advanceBlocksUntil(context, async () => (await currentEpoch(context, 1)) > epoch1);
    epoch2 = await currentEpoch(context, 1);

    const feederNonce = (await context.polkadotApi.query.system.account(feeder.address)).nonce.toNumber();
    await context.polkadotApi.tx.rwaNavOracle.submitPnl(1, epoch2, 0, false).signAndSend(feeder, { nonce: feederNonce });
    await context.createBlock();

    await advanceToSettlementWindow(context, 1, EPOCH_LENGTH_SECS, SETTLEMENT_OFFSET_SECS);

    // oracle_nav = total_borrowed(1_000) + 0 = 1_000; total_reserve = 0 => total_pool_value =
    // 1_000 => senior claims min(1_000, accrued_nav=1_000) = 1_000 => price = 1_000*WAD/1_000
    // (still just investor A's tokens, pre-settlement) = WAD exactly. Investor B's carried-over
    // 200 then settles at that price: shares_minted = 200*WAD/WAD = 200 exactly.
    const tranche = await getTranche(context, 1, CHAIN_ID, VAULT_ADDRESS_A);
    expect(BigInt(tranche.epochPrice)).eq(WAD);
    expect(BigInt(tranche.reserve)).eq(200n);
    expect(BigInt(tranche.tokenSupply)).eq(1_200n);
    expect(BigInt(tranche.pendingOrders.deposit)).eq(0n);

    const rawPending: any = await context.polkadotApi.query.rwaInvestments.pendingDepositOrders(TRANCHE_ID, investorB.public, epoch2);
    expect(rawPending.isNone).eq(true);

    const rawClaimable: any = await context.polkadotApi.query.rwaInvestments.claimableDepositOrders(TRANCHE_ID, investorB.public, epoch2);
    const claimable = rawClaimable.unwrap();
    expect(BigInt(claimable.amount.toJSON())).eq(200n);
    expect(BigInt(claimable.sharesToMint.toJSON())).eq(200n);
  });
});

describeDevNode('pallet_rwa_pools - on_initialize retries NAV finalization every block within an open settlement window', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const poolAdmin: { public: string, private: string } = TEST_CONTROLLERS[1];
  const poolAdminSigner = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const borrower: { public: string, private: string } = TEST_CONTROLLERS[2];
  const gateway: { public: string, private: string } = TEST_CONTROLLERS[3];
  const investor: { public: string, private: string } = TEST_CONTROLLERS[4];
  const feeder = keyring.addFromUri(TEST_CONTROLLERS[5].private);
  const feederPublic: { public: string, private: string } = TEST_CONTROLLERS[5];
  // A wider offset than the usual 10s, so there's comfortable buffer between the deliberately
  // skipped first window block and the late P&L submission without racing epoch end.
  const WIDE_SETTLEMENT_OFFSET_SECS = 30;
  let alithNonce: number;
  let poolAdminNonce: number;

  before('should create a pool, whitelist an investor, and submit a pending deposit with no P&L yet', async function () {
    const alithAccount = await context.polkadotApi.query.system.account(alith.address);
    alithNonce = alithAccount.nonce.toNumber();

    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.rwaPools.setGateway(gateway.public)
    ).signAndSend(alith, { nonce: alithNonce++ });
    await context.createBlock();

    await grantPermission(context, alith, () => alithNonce++, 1, 'PoolAdmin', poolAdmin.public, true);
    const data = encodeCreatePool(
      context, 1, borrower.public, EPOCH_LENGTH_SECS, WIDE_SETTLEMENT_OFFSET_SECS, false, false,
      [{ nftContract: NFT_CONTRACT, nftTokenId: '1' }],
      [{ chainId: CHAIN_ID, vaultAddress: VAULT_ADDRESS_A, isSenior: true, apr: FIVE_PERCENT_APR, maxDeposits: '0' }],
    );
    const createBlock = await sendPrecompileTx(
      context, POOLS_PRECOMPILE_ADDRESS, POOLS_SELECTORS, poolAdmin.public, poolAdmin.private, 'create_pool', [data],
    );
    expect(Boolean((await context.web3.eth.getTransactionReceipt(createBlock.txResults[0])).status)).eq(true);

    const poolAdminAccount = await context.polkadotApi.query.system.account(poolAdminSigner.address);
    poolAdminNonce = poolAdminAccount.nonce.toNumber();
    await grantPermission(context, poolAdminSigner, () => poolAdminNonce++, 1, { TrancheInvestor: TRANCHE_ID }, investor.public, false);
    await grantPermission(context, poolAdminSigner, () => poolAdminNonce++, 1, 'OracleFeeder', feederPublic.public, false);

    const depositBlock = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'submit_deposit_order', [encodeOrder(context, 1, investor.public, 1_000)],
    );
    expect(Boolean((await context.web3.eth.getTransactionReceipt(depositBlock.txResults[0])).status)).eq(true);
    // Deliberately no submitPnl call here — the whole point of this block is to exercise the
    // "oracle hasn't submitted yet when the window opens" path.
  });

  it('should leave epoch_price unset when the window opens with no oracle submission yet', async function () {
    await advanceToSettlementWindow(context, 1, EPOCH_LENGTH_SECS, WIDE_SETTLEMENT_OFFSET_SECS);

    const tranche = await getTranche(context, 1, CHAIN_ID, VAULT_ADDRESS_A);
    expect(tranche.epochPrice).eq(null);
    expect(BigInt(tranche.pendingOrders.deposit)).eq(1_000n);
  });

  it('should retry and successfully finalize once the oracle submits later within the same still-open window', async function () {
    const feederNonce = (await context.polkadotApi.query.system.account(feeder.address)).nonce.toNumber();
    await context.polkadotApi.tx.rwaNavOracle.submitPnl(1, 0, 0, false).signAndSend(feeder, { nonce: feederNonce });
    await context.createBlock();
    // One more block for on_initialize to observe the just-submitted value (same timestamp-lag
    // reasoning documented on advanceToSettlementWindow itself).
    await context.createBlock();

    const tranche = await getTranche(context, 1, CHAIN_ID, VAULT_ADDRESS_A);
    expect(BigInt(tranche.epochPrice)).eq(WAD); // first-ever mint, despite missing the window's first block
    expect(BigInt(tranche.reserve)).eq(1_000n);
    expect(BigInt(tranche.tokenSupply)).eq(1_000n);
    expect(BigInt(tranche.pendingOrders.deposit)).eq(0n);
  });
});

describeDevNode('pallet_rwa_pools - Automatic-mode deposit settlement spans multiple blocks when pending orders exceed the cap', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const poolAdmin: { public: string, private: string } = TEST_CONTROLLERS[1];
  const poolAdminSigner = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const borrower: { public: string, private: string } = TEST_CONTROLLERS[2];
  const gateway: { public: string, private: string } = TEST_CONTROLLERS[3];
  const feeder = keyring.addFromUri(TEST_CONTROLLERS[5].private);
  const feederPublic: { public: string, private: string } = TEST_CONTROLLERS[5];
  const investors: { public: string, private: string }[] =
    [4, 6, 7, 8, 9].map((i) => TEST_CONTROLLERS[i]);
  const CAP = 3;
  let alithNonce: number;
  let poolAdminNonce: number;

  before('should create a pool, set a cap of 3, and submit 5 pending deposits of 100 each', async function () {
    const alithAccount = await context.polkadotApi.query.system.account(alith.address);
    alithNonce = alithAccount.nonce.toNumber();

    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.rwaPools.setGateway(gateway.public)
    ).signAndSend(alith, { nonce: alithNonce++ });
    await context.createBlock();

    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.rwaInvestments.setAutoSettlementCap(CAP)
    ).signAndSend(alith, { nonce: alithNonce++ });
    await context.createBlock();

    await grantPermission(context, alith, () => alithNonce++, 1, 'PoolAdmin', poolAdmin.public, true);
    await createAutomaticPool(context, poolAdmin, borrower, 1);

    const poolAdminAccount = await context.polkadotApi.query.system.account(poolAdminSigner.address);
    poolAdminNonce = poolAdminAccount.nonce.toNumber();
    for (const investor of investors) {
      await grantPermission(context, poolAdminSigner, () => poolAdminNonce++, 1, { TrancheInvestor: TRANCHE_ID }, investor.public, false);
    }
    await grantPermission(context, poolAdminSigner, () => poolAdminNonce++, 1, 'OracleFeeder', feederPublic.public, false);

    for (const investor of investors) {
      const block = await sendPrecompileTx(
        context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
        'submit_deposit_order', [encodeOrder(context, 1, investor.public, 100)],
      );
      expect(Boolean((await context.web3.eth.getTransactionReceipt(block.txResults[0])).status)).eq(true);
    }

    const epoch0 = await currentEpoch(context, 1);
    const feederNonce = (await context.polkadotApi.query.system.account(feeder.address)).nonce.toNumber();
    await context.polkadotApi.tx.rwaNavOracle.submitPnl(1, epoch0, 0, false).signAndSend(feeder, { nonce: feederNonce });
    await context.createBlock();
  });

  it('should settle only the first 3 of 5 pending orders in the windows first block', async function () {
    await advanceToSettlementWindow(context, 1, EPOCH_LENGTH_SECS, SETTLEMENT_OFFSET_SECS);

    const tranche = await getTranche(context, 1, CHAIN_ID, VAULT_ADDRESS_A);
    expect(BigInt(tranche.epochPrice)).eq(WAD);
    expect(BigInt(tranche.reserve)).eq(300n);
    expect(BigInt(tranche.tokenSupply)).eq(300n);
    expect(BigInt(tranche.pendingOrders.deposit)).eq(200n);
  });

  it('should settle the remaining 2 orders on the very next block, completing finalization', async function () {
    await context.createBlock();

    const tranche = await getTranche(context, 1, CHAIN_ID, VAULT_ADDRESS_A);
    expect(BigInt(tranche.epochPrice)).eq(WAD);
    expect(BigInt(tranche.reserve)).eq(500n);
    expect(BigInt(tranche.tokenSupply)).eq(500n);
    expect(BigInt(tranche.pendingOrders.deposit)).eq(0n);
  });
});

describeDevNode('pallet_rwa_pools - Automatic-mode deposit settlement carries remaining orders to the next epoch when the window closes first', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const poolAdmin: { public: string, private: string } = TEST_CONTROLLERS[1];
  const poolAdminSigner = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const borrower: { public: string, private: string } = TEST_CONTROLLERS[2];
  const gateway: { public: string, private: string } = TEST_CONTROLLERS[3];
  const feeder = keyring.addFromUri(TEST_CONTROLLERS[5].private);
  const feederPublic: { public: string, private: string } = TEST_CONTROLLERS[5];
  const investorA: { public: string, private: string } = TEST_CONTROLLERS[4];
  const investorB: { public: string, private: string } = TEST_CONTROLLERS[6];
  // A short epoch/window relative to per-block time (~3s), so only one settlement attempt
  // fits before the window closes and the epoch advances — forcing a genuine carry-over.
  const SHORT_EPOCH_LENGTH_SECS = 12;
  const SHORT_SETTLEMENT_OFFSET_SECS = 4;
  const CAP = 1;
  let alithNonce: number;
  let poolAdminNonce: number;

  before('should create a short-epoch pool, set a cap of 1, and submit 2 pending deposits of 100 each', async function () {
    const alithAccount = await context.polkadotApi.query.system.account(alith.address);
    alithNonce = alithAccount.nonce.toNumber();

    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.rwaPools.setGateway(gateway.public)
    ).signAndSend(alith, { nonce: alithNonce++ });
    await context.createBlock();

    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.rwaInvestments.setAutoSettlementCap(CAP)
    ).signAndSend(alith, { nonce: alithNonce++ });
    await context.createBlock();

    await grantPermission(context, alith, () => alithNonce++, 1, 'PoolAdmin', poolAdmin.public, true);
    const data = encodeCreatePool(
      context, 1, borrower.public, SHORT_EPOCH_LENGTH_SECS, SHORT_SETTLEMENT_OFFSET_SECS, false, false,
      [{ nftContract: NFT_CONTRACT, nftTokenId: '1' }],
      [{ chainId: CHAIN_ID, vaultAddress: VAULT_ADDRESS_A, isSenior: true, apr: FIVE_PERCENT_APR, maxDeposits: '0' }],
    );
    const createBlock = await sendPrecompileTx(
      context, POOLS_PRECOMPILE_ADDRESS, POOLS_SELECTORS, poolAdmin.public, poolAdmin.private, 'create_pool', [data],
    );
    expect(Boolean((await context.web3.eth.getTransactionReceipt(createBlock.txResults[0])).status)).eq(true);

    const poolAdminAccount = await context.polkadotApi.query.system.account(poolAdminSigner.address);
    poolAdminNonce = poolAdminAccount.nonce.toNumber();
    await grantPermission(context, poolAdminSigner, () => poolAdminNonce++, 1, { TrancheInvestor: TRANCHE_ID }, investorA.public, false);
    await grantPermission(context, poolAdminSigner, () => poolAdminNonce++, 1, { TrancheInvestor: TRANCHE_ID }, investorB.public, false);
    await grantPermission(context, poolAdminSigner, () => poolAdminNonce++, 1, 'OracleFeeder', feederPublic.public, false);

    const blockA = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'submit_deposit_order', [encodeOrder(context, 1, investorA.public, 100)],
    );
    expect(Boolean((await context.web3.eth.getTransactionReceipt(blockA.txResults[0])).status)).eq(true);
    const blockB = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'submit_deposit_order', [encodeOrder(context, 1, investorB.public, 100)],
    );
    expect(Boolean((await context.web3.eth.getTransactionReceipt(blockB.txResults[0])).status)).eq(true);

    const epoch0 = await currentEpoch(context, 1);
    const feederNonce = (await context.polkadotApi.query.system.account(feeder.address)).nonce.toNumber();
    await context.polkadotApi.tx.rwaNavOracle.submitPnl(1, epoch0, 0, false).signAndSend(feeder, { nonce: feederNonce });
    await context.createBlock();
  });

  it('should carry an unsettled order into the next epoch once the short window closes', async function () {
    await advanceBlocksUntil(context, async () => (await currentEpoch(context, 1)) > 0);

    const tranche = await getTranche(context, 1, CHAIN_ID, VAULT_ADDRESS_A);
    expect(tranche.epochPrice).eq(null); // epoch advanced, price reset
    // With cap=1 and a window this short, the two orders cannot both settle before the
    // epoch advances — at most one of the two settled, so not everything drained.
    expect(BigInt(tranche.pendingOrders.deposit)).gt(0n);
    expect(BigInt(tranche.reserve)).lt(200n);
  });

  it('should settle the carried-over order normally in a later epochs window', async function () {
    // The short epoch (12s / ~4 blocks) that forced the carry-over above also makes it easy
    // to miss any single epoch's window while waiting, so resubmit PnL for whichever epoch
    // is current on every iteration — matching how a real oracle feeder would keep retrying
    // — rather than betting on hitting one specific epoch's window in time.
    let settled = false;
    for (let i = 0; i < 15 && !settled; i++) {
      const epoch = await currentEpoch(context, 1);
      const feederNonce = (await context.polkadotApi.query.system.account(feeder.address)).nonce.toNumber();
      await context.polkadotApi.tx.rwaNavOracle.submitPnl(1, epoch, 0, false).signAndSend(feeder, { nonce: feederNonce });
      await context.createBlock();

      const tranche = await getTranche(context, 1, CHAIN_ID, VAULT_ADDRESS_A);
      settled = BigInt(tranche.pendingOrders.deposit) === 0n;
    }
    expect(settled).eq(true);

    const tranche = await getTranche(context, 1, CHAIN_ID, VAULT_ADDRESS_A);
    expect(BigInt(tranche.reserve)).eq(200n);
    expect(BigInt(tranche.tokenSupply)).eq(200n);
    expect(BigInt(tranche.pendingOrders.deposit)).eq(0n);
  });
});
