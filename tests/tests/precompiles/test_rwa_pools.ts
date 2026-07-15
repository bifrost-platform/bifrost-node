import { expect } from 'chai';
import { numberToHex } from 'web3-utils';

import { Keyring } from '@polkadot/api';

import { TEST_CONTROLLERS } from '../../constants/keys';
import { getExtrinsicResult } from '../extrinsics';
import { describeDevNode, INodeContext } from '../set_dev_node';
import { sendPrecompileTx } from '../transactions';

const SELECTORS = {
  create_pool: '1d42be4e',
  borrow: 'd9cf66c5',
  repay: '32a56014',
};

const PRECOMPILE_ADDRESS = '0x0000000000000000000000000000000000000201';

const NFT_CONTRACT = '0x' + '11'.repeat(20);
const VAULT_ADDRESS_A = '0x' + '22'.repeat(20);
const VAULT_ADDRESS_B = '0x' + '33'.repeat(20);
const FIVE_PERCENT_APR = '50000000000000000'; // 0.05 * 1e18, FixedU128 inner value

interface Tranche {
  chainId: number;
  vaultAddress: string;
  isSenior: boolean;
  apr: string;
  maxDeposits: string;
}

function encodeCreatePool(
  context: INodeContext,
  poolId: number,
  borrowerId: string,
  epochLengthSecs: number,
  settlementOffsetSecs: number,
  depositApproval: boolean,
  redeemApproval: boolean,
  collaterals: { nftContract: string, nftTokenId: string }[],
  tranches: Tranche[],
): string {
  return context.web3.eth.abi.encodeParameters(
    [
      'uint64', 'address', 'uint64', 'uint64', 'bool', 'bool',
      'tuple(address,uint256)[]',
      'tuple(uint64,address,bool,uint256,uint256)[]',
    ],
    [
      poolId,
      borrowerId,
      epochLengthSecs,
      settlementOffsetSecs,
      depositApproval,
      redeemApproval,
      collaterals.map((c) => [c.nftContract, c.nftTokenId]),
      tranches.map((t) => [t.chainId, t.vaultAddress, t.isSenior, t.apr, t.maxDeposits]),
    ],
  );
}

async function grantPoolAdmin(context: INodeContext, sudoSigner: any, nonce: () => number, poolId: number, who: string) {
  await context.polkadotApi.tx.sudo.sudo(
    context.polkadotApi.tx.rwaPermissions.grantPermission(poolId, 'PoolAdmin', who)
  ).signAndSend(sudoSigner, { nonce: nonce() });
  await context.createBlock();
}

describeDevNode('pallet_rwa_pools - set_gateway', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const baltathar = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const charleth: { public: string, private: string } = TEST_CONTROLLERS[2];
  let alithNonce: number;
  let baltatharNonce: number;

  beforeEach(async function () {
    const alithAccount = await context.polkadotApi.query.system.account(alith.address);
    alithNonce = alithAccount.nonce.toNumber();
    const baltatharAccount = await context.polkadotApi.query.system.account(baltathar.address);
    baltatharNonce = baltatharAccount.nonce.toNumber();
  });

  it('should fail to set the gateway address from a non-root origin', async function () {
    await context.polkadotApi.tx.rwaPools.setGateway(charleth.public).signAndSend(baltathar, { nonce: baltatharNonce++ });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'rwaPools', 'setGateway');
    expect(extrinsicResult).eq('BadOrigin');
  });

  it('should successfully set the gateway address via sudo', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.rwaPools.setGateway(charleth.public)
    ).signAndSend(alith, { nonce: alithNonce++ });
    await context.createBlock();

    const rawGateway: any = await context.polkadotApi.query.rwaPools.gatewayAddress();
    expect(rawGateway.toHuman().toLowerCase()).eq(charleth.public.toLowerCase());
  });
});

describeDevNode('precompile_rwa_pools - create_pool', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const poolAdmin: { public: string, private: string } = TEST_CONTROLLERS[1];
  const borrower: { public: string, private: string } = TEST_CONTROLLERS[2];
  const outsider: { public: string, private: string } = TEST_CONTROLLERS[3];
  let alithNonce: number;

  beforeEach(async function () {
    const alithAccount = await context.polkadotApi.query.system.account(alith.address);
    alithNonce = alithAccount.nonce.toNumber();
  });

  it('should fail to create a pool without the PoolAdmin role', async function () {
    const data = encodeCreatePool(
      context, 1, borrower.public, 86_400, 3_600, true, true,
      [{ nftContract: NFT_CONTRACT, nftTokenId: '1' }],
      [{ chainId: 1, vaultAddress: VAULT_ADDRESS_A, isSenior: true, apr: FIVE_PERCENT_APR, maxDeposits: '0' }],
    );
    const block = await sendPrecompileTx(
      context, PRECOMPILE_ADDRESS, SELECTORS, outsider.public, outsider.private, 'create_pool', [data],
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(false);
  });

  it('should successfully create a pool', async function () {
    await grantPoolAdmin(context, alith, () => alithNonce++, 1, poolAdmin.public);

    const data = encodeCreatePool(
      context, 1, borrower.public, 86_400, 3_600, true, true,
      [{ nftContract: NFT_CONTRACT, nftTokenId: '1' }],
      [{ chainId: 1, vaultAddress: VAULT_ADDRESS_A, isSenior: true, apr: FIVE_PERCENT_APR, maxDeposits: '0' }],
    );
    const block = await sendPrecompileTx(
      context, PRECOMPILE_ADDRESS, SELECTORS, poolAdmin.public, poolAdmin.private, 'create_pool', [data],
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(true);

    const rawPool: any = await context.polkadotApi.query.rwaPools.pools(1);
    expect(rawPool.isSome).eq(true);
    const pool = rawPool.unwrap().toHuman();
    expect(pool.depositSettlement).eq('Approval');
    expect(pool.redeemSettlement).eq('Approval');
    expect(pool.epoch.currentEpoch).eq('0');
    expect(pool.collaterals).to.have.lengthOf(1);

    const rawTranchePool: any = await context.polkadotApi.query.rwaPools.tranches({ chainId: 1, vaultAddress: VAULT_ADDRESS_A });
    expect(rawTranchePool.unwrap().toNumber()).eq(1);

    const rawCollateralPool: any = await context.polkadotApi.query.rwaPools.collaterals({ nftContract: NFT_CONTRACT, nftTokenId: '1' });
    expect(rawCollateralPool.unwrap().toNumber()).eq(1);

    const rawBorrower: any = await context.polkadotApi.query.rwaPermissions.borrowers(1);
    expect(rawBorrower.unwrap().toHuman()).eq(borrower.public);
  });

  it('should fail to create a pool with a duplicate pool ID', async function () {
    const data = encodeCreatePool(
      context, 1, borrower.public, 86_400, 3_600, true, true,
      [{ nftContract: '0x' + '44'.repeat(20), nftTokenId: '1' }],
      [{ chainId: 2, vaultAddress: VAULT_ADDRESS_B, isSenior: true, apr: FIVE_PERCENT_APR, maxDeposits: '0' }],
    );
    const block = await sendPrecompileTx(
      context, PRECOMPILE_ADDRESS, SELECTORS, poolAdmin.public, poolAdmin.private, 'create_pool', [data],
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(false);
  });

  it('should fail to create a pool with an invalid settlement offset', async function () {
    await grantPoolAdmin(context, alith, () => alithNonce++, 2, poolAdmin.public);

    const data = encodeCreatePool(
      context, 2, borrower.public, 3_600, 3_600, true, true,
      [{ nftContract: '0x' + '55'.repeat(20), nftTokenId: '1' }],
      [{ chainId: 3, vaultAddress: '0x' + '66'.repeat(20), isSenior: true, apr: FIVE_PERCENT_APR, maxDeposits: '0' }],
    );
    const block = await sendPrecompileTx(
      context, PRECOMPILE_ADDRESS, SELECTORS, poolAdmin.public, poolAdmin.private, 'create_pool', [data],
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(false);
  });

  it('should fail to create a pool with two senior tranches', async function () {
    await grantPoolAdmin(context, alith, () => alithNonce++, 3, poolAdmin.public);

    const data = encodeCreatePool(
      context, 3, borrower.public, 86_400, 3_600, true, true,
      [{ nftContract: '0x' + '77'.repeat(20), nftTokenId: '1' }],
      [
        { chainId: 4, vaultAddress: '0x' + '88'.repeat(20), isSenior: true, apr: FIVE_PERCENT_APR, maxDeposits: '0' },
        { chainId: 4, vaultAddress: '0x' + '99'.repeat(20), isSenior: true, apr: FIVE_PERCENT_APR, maxDeposits: '0' },
      ],
    );
    const block = await sendPrecompileTx(
      context, PRECOMPILE_ADDRESS, SELECTORS, poolAdmin.public, poolAdmin.private, 'create_pool', [data],
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(false);
  });
});

describeDevNode('pallet_rwa_pools - add_vault', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const poolAdmin = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const poolAdminPublic: { public: string, private: string } = TEST_CONTROLLERS[1];
  const borrower: { public: string, private: string } = TEST_CONTROLLERS[2];
  const outsider = keyring.addFromUri(TEST_CONTROLLERS[3].private);
  let alithNonce: number;
  let poolAdminNonce: number;

  before('should grant PoolAdmin and create a pool with a senior tranche', async function () {
    const alithAccount = await context.polkadotApi.query.system.account(alith.address);
    alithNonce = alithAccount.nonce.toNumber();

    await grantPoolAdmin(context, alith, () => alithNonce++, 1, poolAdminPublic.public);
    // Pool ID 2 gets a PoolAdmin grant but is never created, to test PoolNotFound below.
    await grantPoolAdmin(context, alith, () => alithNonce++, 2, poolAdminPublic.public);

    const data = encodeCreatePool(
      context, 1, borrower.public, 86_400, 3_600, true, true,
      [{ nftContract: NFT_CONTRACT, nftTokenId: '1' }],
      [{ chainId: 1, vaultAddress: VAULT_ADDRESS_A, isSenior: true, apr: FIVE_PERCENT_APR, maxDeposits: '0' }],
    );
    const block = await sendPrecompileTx(
      context, PRECOMPILE_ADDRESS, SELECTORS, poolAdminPublic.public, poolAdminPublic.private, 'create_pool', [data],
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(true);
  });

  beforeEach(async function () {
    const poolAdminAccount = await context.polkadotApi.query.system.account(poolAdmin.address);
    poolAdminNonce = poolAdminAccount.nonce.toNumber();
  });

  it('should fail to add a vault when the caller is not the pool admin', async function () {
    await context.polkadotApi.tx.rwaPools.addVault(1, {
      trancheType: 'Junior',
      trancheId: { chainId: 5, vaultAddress: VAULT_ADDRESS_B },
      maxDeposits: null,
    }).signAndSend(outsider, { nonce: (await context.polkadotApi.query.system.account(outsider.address)).nonce.toNumber() });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'rwaPools', 'addVault');
    expect(extrinsicResult).eq('Unauthorized');
  });

  it('should fail to add a vault to a pool that does not exist', async function () {
    await context.polkadotApi.tx.rwaPools.addVault(2, {
      trancheType: 'Junior',
      trancheId: { chainId: 5, vaultAddress: VAULT_ADDRESS_B },
      maxDeposits: null,
    }).signAndSend(poolAdmin, { nonce: poolAdminNonce++ });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'rwaPools', 'addVault');
    expect(extrinsicResult).eq('PoolNotFound');
  });

  it('should successfully add a junior vault to an existing pool', async function () {
    await context.polkadotApi.tx.rwaPools.addVault(1, {
      trancheType: 'Junior',
      trancheId: { chainId: 5, vaultAddress: VAULT_ADDRESS_B },
      maxDeposits: null,
    }).signAndSend(poolAdmin, { nonce: poolAdminNonce++ });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'rwaPools', 'addVault');
    expect(extrinsicResult).eq(null);

    const rawTranchePool: any = await context.polkadotApi.query.rwaPools.tranches({ chainId: 5, vaultAddress: VAULT_ADDRESS_B });
    expect(rawTranchePool.unwrap().toNumber()).eq(1);
  });

  it('should fail to add a vault with a tranche ID that already exists', async function () {
    await context.polkadotApi.tx.rwaPools.addVault(1, {
      trancheType: 'Junior',
      trancheId: { chainId: 5, vaultAddress: VAULT_ADDRESS_B },
      maxDeposits: null,
    }).signAndSend(poolAdmin, { nonce: poolAdminNonce++ });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'rwaPools', 'addVault');
    expect(extrinsicResult).eq('TrancheAlreadyExists');
  });

  it('should fail to add a second junior tranche to the same pool', async function () {
    await context.polkadotApi.tx.rwaPools.addVault(1, {
      trancheType: 'Junior',
      trancheId: { chainId: 6, vaultAddress: '0x' + 'aa'.repeat(20) },
      maxDeposits: null,
    }).signAndSend(poolAdmin, { nonce: poolAdminNonce++ });
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'rwaPools', 'addVault');
    expect(extrinsicResult).eq('DuplicateTrancheType');
  });
});

describeDevNode('precompile_rwa_pools - borrow / repay', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const alith = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const poolAdmin: { public: string, private: string } = TEST_CONTROLLERS[1];
  const borrower: { public: string, private: string } = TEST_CONTROLLERS[2];
  const gateway: { public: string, private: string } = TEST_CONTROLLERS[3];
  const outsider: { public: string, private: string } = TEST_CONTROLLERS[4];
  const otherBorrower: { public: string, private: string } = TEST_CONTROLLERS[5];
  let alithNonce: number;

  before('should set the gateway and create a pool', async function () {
    const alithAccount = await context.polkadotApi.query.system.account(alith.address);
    alithNonce = alithAccount.nonce.toNumber();

    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.rwaPools.setGateway(gateway.public)
    ).signAndSend(alith, { nonce: alithNonce++ });
    await context.createBlock();

    await grantPoolAdmin(context, alith, () => alithNonce++, 1, poolAdmin.public);

    const data = encodeCreatePool(
      context, 1, borrower.public, 86_400, 3_600, true, true,
      [{ nftContract: NFT_CONTRACT, nftTokenId: '1' }],
      [{ chainId: 1, vaultAddress: VAULT_ADDRESS_A, isSenior: true, apr: FIVE_PERCENT_APR, maxDeposits: '0' }],
    );
    const block = await sendPrecompileTx(
      context, PRECOMPILE_ADDRESS, SELECTORS, poolAdmin.public, poolAdmin.private, 'create_pool', [data],
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(true);
  });

  async function getTranche(): Promise<{ reserve: bigint, borrowed: bigint }> {
    const rawPool: any = await context.polkadotApi.query.rwaPools.pools(1);
    const pool = rawPool.unwrap().toJSON();
    const key = JSON.stringify({ chainId: 1, vaultAddress: VAULT_ADDRESS_A.toLowerCase() });
    const tranche = pool.tranches[key];
    return { reserve: BigInt(tranche.reserve), borrowed: BigInt(tranche.borrowed) };
  }

  it('should fail to repay from a non-gateway caller', async function () {
    const block = await sendPrecompileTx(
      context, PRECOMPILE_ADDRESS, SELECTORS, outsider.public, outsider.private, 'repay',
      [numberToHex(1), numberToHex(1), VAULT_ADDRESS_A, borrower.public, numberToHex(1_000)],
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(false);
  });

  it('should fail to borrow when the treasury reserve is empty', async function () {
    const block = await sendPrecompileTx(
      context, PRECOMPILE_ADDRESS, SELECTORS, gateway.public, gateway.private, 'borrow',
      [numberToHex(1), numberToHex(1), VAULT_ADDRESS_A, borrower.public, numberToHex(100)],
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(false);
  });

  it('should successfully repay (funding the treasury reserve)', async function () {
    const block = await sendPrecompileTx(
      context, PRECOMPILE_ADDRESS, SELECTORS, gateway.public, gateway.private, 'repay',
      [numberToHex(1), numberToHex(1), VAULT_ADDRESS_A, borrower.public, numberToHex(1_000)],
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(true);

    const tranche = await getTranche();
    expect(tranche.reserve).eq(1_000n);
    expect(tranche.borrowed).eq(0n);
  });

  it('should fail to repay/borrow for an unauthorized borrower address', async function () {
    const block = await sendPrecompileTx(
      context, PRECOMPILE_ADDRESS, SELECTORS, gateway.public, gateway.private, 'borrow',
      [numberToHex(1), numberToHex(1), VAULT_ADDRESS_A, otherBorrower.public, numberToHex(100)],
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(false);
  });

  it('should fail to borrow a zero amount', async function () {
    const block = await sendPrecompileTx(
      context, PRECOMPILE_ADDRESS, SELECTORS, gateway.public, gateway.private, 'borrow',
      [numberToHex(1), numberToHex(1), VAULT_ADDRESS_A, borrower.public, numberToHex(0)],
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(false);
  });

  it('should successfully borrow against the funded reserve', async function () {
    const block = await sendPrecompileTx(
      context, PRECOMPILE_ADDRESS, SELECTORS, gateway.public, gateway.private, 'borrow',
      [numberToHex(1), numberToHex(1), VAULT_ADDRESS_A, borrower.public, numberToHex(400)],
    );
    const receipt = await context.web3.eth.getTransactionReceipt(block.txResults[0]);
    expect(Boolean(receipt.status)).eq(true);

    const tranche = await getTranche();
    expect(tranche.reserve).eq(600n);
    expect(tranche.borrowed).eq(400n);
  });
});
