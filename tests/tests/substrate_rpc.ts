import { ApiPromise } from '@polkadot/api';
import { EventRecord } from '@polkadot/types/interfaces';
import { u8aToHex } from '@polkadot/util';

export async function waitOneBlock(api: ApiPromise, numberOfBlocks: number = 1) {
  return new Promise<void>(async (res) => {
    let count = 0;
    let unsub = await api.derive.chain.subscribeNewHeads(async (header) => {
      count += 1;
      if (count === 1 + numberOfBlocks) {
        unsub();
        res();
      }
    });
  });
}

export async function lookForExtrinsicAndEvents(api: ApiPromise, extrinsicHash: Uint8Array) {
  // We retrieve the block (including the extrinsics)
  const signedBlock = await api.rpc.chain.getBlock();

  // We retrieve the events for that block
  const allRecords: EventRecord[] = (await (
    await api.at(signedBlock.block.header.hash)
  ).query.system.events()) as any;

  const extrinsicIndex = signedBlock.block.extrinsics.findIndex((ext) => {
    return ext.hash.toHex() == u8aToHex(extrinsicHash);
  });
  if (extrinsicIndex < 0) {
    console.log(
      `Extrinsic ${extrinsicHash} is missing in the block ${signedBlock.block.header.hash}`
    );
  }
  const extrinsic = signedBlock.block.extrinsics[extrinsicIndex];

  // We retrieve the events associated with the extrinsic
  const events = allRecords
    .filter(
      ({ phase }) => phase.isApplyExtrinsic && phase.asApplyExtrinsic.toNumber() == extrinsicIndex
    )
    .map(({ event }) => event);
  return { events, extrinsic };
}

export async function tryLookingForEvents(api: ApiPromise, extrinsicHash: Uint8Array): Promise<any> {
  await waitOneBlock(api);
  let { extrinsic, events } = await lookForExtrinsicAndEvents(api, extrinsicHash);
  if (events.length > 0) {
    return {
      extrinsic,
      events,
    };
  } else {
    return await tryLookingForEvents(api, extrinsicHash);
  }
}
