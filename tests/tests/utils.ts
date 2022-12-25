import { INodeContext } from './set_dev_node';

export const sleep = (ms: number) => new Promise((resolve) => {
  setTimeout(resolve, ms);
});

export async function jumpToRound(context: INodeContext, round: Number): Promise<string | null> {
  let lastBlockHash = null;

  while (true) {
    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentRound = rawCurrentRound.currentRoundIndex.toNumber();
    if (currentRound == round) {
      return lastBlockHash;
    }
    lastBlockHash = (await context.createBlock()).block.hash.toString();
  }
}

export async function jumpToSession(context: INodeContext, session: Number): Promise<string | null> {
  let lastBlockHash = null;

  while (true) {
    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentSession = rawCurrentRound.currentSessionIndex.toNumber();
    if (currentSession == session) {
      return lastBlockHash;
    }
    lastBlockHash = (await context.createBlock()).block.hash.toString();
  }
}

export async function jumpToOneBlockBeforeSession(context: INodeContext): Promise<string | null> {
  let lastBlockHash = null;

  while (true) {
    const rawCurrentRound: any = await context.polkadotApi.query.bfcStaking.round();
    const currentBlock = rawCurrentRound.currentBlock.toNumber();
    const sessionFirstBlock = rawCurrentRound.firstSessionBlock.toNumber();
    const sessionLength = rawCurrentRound.sessionLength.toNumber();
    const oneBlockBeforeSession = sessionFirstBlock + sessionLength - 1;

    if (currentBlock === oneBlockBeforeSession) {
      return lastBlockHash;
    }
    lastBlockHash = (await context.createBlock()).block.hash.toString();
  }
}

export async function jumpToLaunch(context: INodeContext) {
  const rawLaunchPeriod: any = context.polkadotApi.consts.democracy.launchPeriod;
  const launchPeriod = rawLaunchPeriod.toNumber();

  while (true) {
    const rawCurrentBlock: any = await context.polkadotApi.query.system.number();
    const currentBlock = rawCurrentBlock.toNumber();
    if (currentBlock % launchPeriod === 0) {
      break;
    }
    await context.createBlock();
  }
}

export async function endVote(context: INodeContext, referendumIndex: number) {
  const rawReferendumInfo: any = await context.polkadotApi.query.democracy.referendumInfoOf(referendumIndex);
  const referendumInfo = rawReferendumInfo.unwrap().toJSON();
  const blockBeforeEnd = referendumInfo.ongoing.end - 1;

  while (true) {
    const rawCurrentBlock: any = await context.polkadotApi.query.system.number();
    const currentBlock = rawCurrentBlock.toNumber();
    if (currentBlock === blockBeforeEnd) {
      break;
    }
    await context.createBlock();
  }
}
