import { INodeContext } from '../set_dev_node';

export const POOLS_PRECOMPILE_ADDRESS = '0x0000000000000000000000000000000000000201';
export const INVESTMENTS_PRECOMPILE_ADDRESS = '0x0000000000000000000000000000000000000200';
export const PERMISSIONS_PRECOMPILE_ADDRESS = '0x0000000000000000000000000000000000000202';

export const POOLS_SELECTORS = {
  create_pool: '1d42be4e',
  borrow: 'd9cf66c5',
  repay: '32a56014',
};

export const INVESTMENTS_SELECTORS = {
  submit_deposit_order: '234d3df8',
  submit_redeem_order: '4cfd9d9d',
  approve_deposit_orders: '0f2af1b2',
  approve_redeem_orders: '7d47a7e1',
  claim_shares: '169acdad',
  claim_assets: 'bc0b21ef',
  cancel_deposit_order: '6cbe3256',
  cancel_redeem_order: '89639142',
};

export const PERMISSIONS_SELECTORS = {
  add_tranche_investor: '5e704920',
  remove_tranche_investor: 'bf0f9c3d',
};

export const NFT_CONTRACT = '0x' + '11'.repeat(20);
export const VAULT_ADDRESS_A = '0x' + '22'.repeat(20);
export const FIVE_PERCENT_APR = '50000000000000000'; // 0.05 * 1e18, FixedU128 inner value

export interface TrancheInput {
  chainId: number;
  vaultAddress: string;
  isSenior: boolean;
  apr: string;
  maxDeposits: string;
}

export function encodeCreatePool(
  context: INodeContext,
  poolId: number,
  borrowerId: string,
  epochLengthSecs: number,
  settlementOffsetSecs: number,
  depositApproval: boolean,
  redeemApproval: boolean,
  collaterals: { nftContract: string, nftTokenId: string }[],
  tranches: TrancheInput[],
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

/// Grants a role via `rwaPermissions.grantPermission`. Pass a root-holding signer for
/// `Role::PoolAdmin`, or a pool-admin-holding signer for `OracleFeeder`/`TrancheInvestor`.
export async function grantPermission(
  context: INodeContext,
  signer: any,
  nonce: () => number,
  poolId: number,
  role: any,
  who: string,
  asSudo: boolean,
) {
  const call = context.polkadotApi.tx.rwaPermissions.grantPermission(poolId, role, who);
  await (asSudo ? context.polkadotApi.tx.sudo.sudo(call) : call).signAndSend(signer, { nonce: nonce() });
  await context.createBlock();
}

export async function currentTimestampSecs(context: INodeContext): Promise<number> {
  const now: any = await context.polkadotApi.query.timestamp.now();
  return Math.floor(now.toNumber() / 1000);
}

/// Repeatedly produces blocks until `predicate()` resolves true, or throws after `maxBlocks`.
/// Block timestamps advance by a fixed `MinimumPeriod` (3s on this chain) once warmed up, so
/// this is used to deterministically reach a pool's settlement window without real sleeps.
export async function advanceBlocksUntil(
  context: INodeContext,
  predicate: () => Promise<boolean>,
  maxBlocks: number = 40,
) {
  for (let i = 0; i < maxBlocks; i++) {
    if (await predicate()) return;
    await context.createBlock();
  }
  throw new Error('advanceBlocksUntil: condition not met within maxBlocks');
}

/// Advances blocks until `poolId`'s settlement window has opened, including the extra block
/// needed for on_initialize's timestamp lag to actually observe it (see
/// feedback_rwa_pools_test_gotchas #9). Submit any NAV earnings for the current epoch *before*
/// calling this — see gotcha #9 for why a late submission permanently misses the window.
export async function advanceToSettlementWindow(
  context: INodeContext,
  poolId: number,
  epochLengthSecs: number,
  settlementOffsetSecs: number,
) {
  const rawPool: any = await context.polkadotApi.query.rwaPools.pools(poolId);
  const pool = rawPool.unwrap().toJSON();
  const windowStartSecs = pool.epoch.epochStartSecs + epochLengthSecs - settlementOffsetSecs;
  await advanceBlocksUntil(context, async () => (await currentTimestampSecs(context)) >= windowStartSecs);
  await context.createBlock();
}

export async function getTranche(context: INodeContext, poolId: number, chainId: number, vaultAddress: string): Promise<any> {
  const rawPool: any = await context.polkadotApi.query.rwaPools.pools(poolId);
  const pool = rawPool.unwrap().toJSON();
  const key = JSON.stringify({ chainId, vaultAddress: vaultAddress.toLowerCase() });
  return pool.tranches[key];
}
