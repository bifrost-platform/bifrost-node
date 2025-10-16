import {
  FrameSupportDispatchPostDispatchInfo, SpRuntimeDispatchError
} from '@polkadot/types/lookup';
import { IEvent } from '@polkadot/types/types';

import { INodeContext } from './set_dev_node';

import type { BlockHash } from '@polkadot/types/interfaces/chain/types';

export async function getExtrinsicResult(
  context: INodeContext,
  pallet: string,
  call: string
): Promise<string | null> {
  const signedBlock = await context.polkadotApi.rpc.chain.getBlock();
  const apiAt = await context.polkadotApi.at(signedBlock.block.header.hash);
  const allEvents = await apiAt.query.system.events();

  const extrinsicIndex = signedBlock.block.extrinsics.findIndex(
    (ext) => pallet == ext.method.section && call === ext.method.method
  );
  if (extrinsicIndex === -1) {
    return null;
  }

  const failedEvent = allEvents.find(
    ({ phase, event }) =>
      phase.isApplyExtrinsic &&
      phase.asApplyExtrinsic.eq(extrinsicIndex) &&
      context.polkadotApi.events.system.ExtrinsicFailed.is(event)
  );
  if (!failedEvent) {
    return null;
  }

  const event: IEvent<[SpRuntimeDispatchError, FrameSupportDispatchPostDispatchInfo]> =
    failedEvent.event as any;
  const [dispatchError, _dispatchInfo] = event.data;
  if (dispatchError.isModule) {
    const decodedError = context.polkadotApi.registry.findMetaError(dispatchError.asModule);
    return decodedError.name;
  }

  return dispatchError.toString();
}

interface IEventParam {
  method: string;
  section: string;
}

export async function isEventTriggered(
  context: INodeContext,
  blockHash: BlockHash,
  params: IEventParam[],
): Promise<boolean> {
  const rawEvents: any = await context.polkadotApi.query.system.events.at(blockHash);
  const events = rawEvents.toHuman();

  let isTriggered: boolean = true;
  for (const param of params) {
    const trigger = events.find((event: any) => event.event.method === param.method && event.event.section === param.section);
    if (!trigger) {
      isTriggered = false;
      break;
    }
  }
  return isTriggered;
}
