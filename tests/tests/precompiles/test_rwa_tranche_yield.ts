import { expect } from 'chai';

import { Keyring } from '@polkadot/api';

import { TEST_CONTROLLERS } from '../../constants/keys';
import { describeDevNode, INodeContext } from '../set_dev_node';
import { sendPrecompileTx } from '../transactions';
import {
  advanceBlocksUntil, advanceToSettlementWindow, getTranche, grantPermission,
  FIVE_PERCENT_APR, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, NFT_CONTRACT,
  POOLS_PRECOMPILE_ADDRESS, POOLS_SELECTORS, VAULT_ADDRESS_A
} from './rwa_helpers';

const CHAIN_ID = 1;
const VAULT_ADDRESS_JUNIOR = '0x' + '33'.repeat(20);
const SENIOR_TRANCHE_ID = { chainId: CHAIN_ID, vaultAddress: VAULT_ADDRESS_A };
const JUNIOR_TRANCHE_ID = { chainId: CHAIN_ID, vaultAddress: VAULT_ADDRESS_JUNIOR };
const EPOCH_LENGTH_SECS = 60;
const SETTLEMENT_OFFSET_SECS = 10;
const WAD = 1_000_000_000_000_000_000n;

const SENIOR_DEPOSIT = 2_000;
const JUNIOR_DEPOSIT = 1_000;
const CUMULATIVE_EARNINGS_EPOCH_1 = 500;

function encodeOrder(context: INodeContext, poolId: number, vaultAddress: string, investorId: string, amount: number): string {
  return context.web3.eth.abi.encodeParameters(
    ['uint64', 'uint64', 'address', 'address', 'uint256'],
    [poolId, CHAIN_ID, vaultAddress, investorId, amount],
  );
}

async function currentEpoch(context: INodeContext, poolId: number): Promise<number> {
  const rawPool: any = await context.polkadotApi.query.rwaPools.pools(poolId);
  return rawPool.unwrap().toJSON().epoch.currentEpoch;
}

describeDevNode('pallet_rwa_pools - Senior/Junior waterfall NAV split with nonzero oracle earnings', (context) => {
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
  let pricingEpoch: number;

  before('should create a Senior+Junior pool, whitelist an investor for both tranches, and settle epoch-0 deposits', async function () {
    const alithAccount = await context.polkadotApi.query.system.account(alith.address);
    alithNonce = alithAccount.nonce.toNumber();

    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.rwaPools.setGateway(gateway.public)
    ).signAndSend(alith, { nonce: alithNonce++ });
    await context.createBlock();

    await grantPermission(context, alith, () => alithNonce++, 1, 'PoolAdmin', poolAdmin.public, true);

    // Automatic/Automatic mode: settlement happens purely via on_initialize, no approve_* calls
    // needed, keeping this test focused on the NAV waterfall rather than settlement-mode plumbing.
    const data = context.web3.eth.abi.encodeParameters(
      [
        'uint64', 'address', 'uint64', 'uint64', 'bool', 'bool',
        'tuple(address,uint256)[]',
        'tuple(uint64,address,bool,uint256,uint256)[]',
      ],
      [
        1, borrower.public, EPOCH_LENGTH_SECS, SETTLEMENT_OFFSET_SECS, false, false,
        [[NFT_CONTRACT, '1']],
        [
          [CHAIN_ID, VAULT_ADDRESS_A, true, FIVE_PERCENT_APR, '0'],
          [CHAIN_ID, VAULT_ADDRESS_JUNIOR, false, '0', '0'],
        ],
      ],
    );
    const createBlock = await sendPrecompileTx(
      context, POOLS_PRECOMPILE_ADDRESS, POOLS_SELECTORS, poolAdmin.public, poolAdmin.private, 'create_pool', [data],
    );
    expect(Boolean((await context.web3.eth.getTransactionReceipt(createBlock.txResults[0])).status)).eq(true);

    const poolAdminAccount = await context.polkadotApi.query.system.account(poolAdminSigner.address);
    poolAdminNonce = poolAdminAccount.nonce.toNumber();
    await grantPermission(context, poolAdminSigner, () => poolAdminNonce++, 1, { TrancheInvestor: SENIOR_TRANCHE_ID }, investor.public, false);
    await grantPermission(context, poolAdminSigner, () => poolAdminNonce++, 1, { TrancheInvestor: JUNIOR_TRANCHE_ID }, investor.public, false);
    await grantPermission(context, poolAdminSigner, () => poolAdminNonce++, 1, 'OracleFeeder', feederPublic.public, false);

    const seniorDepositBlock = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'submit_deposit_order', [encodeOrder(context, 1, VAULT_ADDRESS_A, investor.public, SENIOR_DEPOSIT)],
    );
    expect(Boolean((await context.web3.eth.getTransactionReceipt(seniorDepositBlock.txResults[0])).status)).eq(true);

    const juniorDepositBlock = await sendPrecompileTx(
      context, INVESTMENTS_PRECOMPILE_ADDRESS, INVESTMENTS_SELECTORS, gateway.public, gateway.private,
      'submit_deposit_order', [encodeOrder(context, 1, VAULT_ADDRESS_JUNIOR, investor.public, JUNIOR_DEPOSIT)],
    );
    expect(Boolean((await context.web3.eth.getTransactionReceipt(juniorDepositBlock.txResults[0])).status)).eq(true);

    depositEpoch = await currentEpoch(context, 1);
    const feederNonce = (await context.polkadotApi.query.system.account(feeder.address)).nonce.toNumber();
    await context.polkadotApi.tx.rwaNavOracle.submitEarnings(1, depositEpoch, 0).signAndSend(feeder, { nonce: feederNonce });
    await context.createBlock();

    await advanceToSettlementWindow(context, 1, EPOCH_LENGTH_SECS, SETTLEMENT_OFFSET_SECS);
  });

  it('should have settled both tranches epoch-0 deposits at price WAD (first-ever mint)', async function () {
    const senior = await getTranche(context, 1, CHAIN_ID, VAULT_ADDRESS_A);
    expect(BigInt(senior.reserve)).eq(BigInt(SENIOR_DEPOSIT));
    expect(BigInt(senior.tokenSupply)).eq(BigInt(SENIOR_DEPOSIT));
    expect(BigInt(senior.accruedNav)).eq(BigInt(SENIOR_DEPOSIT));

    const junior = await getTranche(context, 1, CHAIN_ID, VAULT_ADDRESS_JUNIOR);
    expect(BigInt(junior.reserve)).eq(BigInt(JUNIOR_DEPOSIT));
    expect(BigInt(junior.tokenSupply)).eq(BigInt(JUNIOR_DEPOSIT));
    // Junior never accrues NAV of its own — its claim is always the waterfall residual.
    expect(BigInt(junior.accruedNav)).eq(0n);
  });

  it('should split epoch-1 NAV so the senior claims its accrued_nav and the junior gets the residual', async function () {
    await advanceBlocksUntil(context, async () => (await currentEpoch(context, 1)) > depositEpoch);
    pricingEpoch = await currentEpoch(context, 1);

    const feederNonce = (await context.polkadotApi.query.system.account(feeder.address)).nonce.toNumber();
    await context.polkadotApi.tx.rwaNavOracle.submitEarnings(1, pricingEpoch, CUMULATIVE_EARNINGS_EPOCH_1)
      .signAndSend(feeder, { nonce: feederNonce });
    await context.createBlock();

    await advanceToSettlementWindow(context, 1, EPOCH_LENGTH_SECS, SETTLEMENT_OFFSET_SECS);

    const senior = await getTranche(context, 1, CHAIN_ID, VAULT_ADDRESS_A);
    const junior = await getTranche(context, 1, CHAIN_ID, VAULT_ADDRESS_JUNIOR);

    // oracle_nav = total_borrowed(0) + cumulative_earnings - repaid_earnings(0) = 500.
    // total_pool_value = oracle_nav + (senior.reserve + junior.reserve) = 500 + 2000 + 1000 = 3500.
    // Senior claims min(total_pool_value, accrued_nav). Interest for one 60s epoch at 5% APR on a
    // principal this small floors to exactly zero (see feedback_rwa_pools_test_gotchas #12/#13),
    // so accrued_nav is still exactly SENIOR_DEPOSIT and well below total_pool_value — the senior
    // is not capped, so its claim is its full (unchanged) accrued_nav.
    const totalPoolValue = BigInt(CUMULATIVE_EARNINGS_EPOCH_1) + BigInt(SENIOR_DEPOSIT) + BigInt(JUNIOR_DEPOSIT);
    const seniorNav = BigInt(senior.accruedNav);
    expect(seniorNav).eq(BigInt(SENIOR_DEPOSIT));
    const juniorNav = totalPoolValue - seniorNav;

    const expectedSeniorPrice = (seniorNav * WAD) / BigInt(SENIOR_DEPOSIT); // == WAD exactly
    const expectedJuniorPrice = (juniorNav * WAD) / BigInt(JUNIOR_DEPOSIT);

    expect(BigInt(senior.epochPrice)).eq(expectedSeniorPrice);
    expect(BigInt(senior.epochPrice)).eq(WAD);
    expect(BigInt(junior.epochPrice)).eq(expectedJuniorPrice);
    expect(BigInt(junior.epochPrice)).eq(1_500_000_000_000_000_000n); // 1.5 * WAD, spelled out
  });
});
