import '@polkadot/api-augment';

import { ApiPromise } from '@polkadot/api';
import { BlockHash } from '@polkadot/types/interfaces';

export async function createAndFinalizeBlock(
  api: ApiPromise,
  parentHash?: BlockHash,
  finalize: boolean = true
): Promise<{
  duration: number;
  hash: BlockHash;
}> {
  const startTime: number = Date.now();
  let hash: any = undefined;
  try {
    if (parentHash == undefined) {
      hash = (await api.rpc.engine.createBlock(true, finalize)).toJSON()['hash'];
    } else {
      hash = (await api.rpc.engine.createBlock(true, finalize, parentHash)).toJSON()['hash'];
    }
  } catch (err) {
    console.error('ERROR DURING BLOCK CREATION AND FINALIZATION', err);
  }

  return {
    duration: Date.now() - startTime,
    hash,
  };
}
