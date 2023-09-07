import { ChildProcess, spawn } from 'child_process';
import { ethers } from 'ethers';
import tcpPortUsed from 'tcp-port-used';
import { setTimeout } from 'timers/promises';
import { HttpProvider } from 'web3-core';
import { JsonRpcResponse } from 'web3-core-helpers';

import { ApiPromise } from '@polkadot/api';

import { DEBUG_MODE, SPAWNING_TIME } from '../constants/config';
import { createAndFinalizeBlock } from './blocks';
import {
  customWeb3Request, EnhancedWeb3, provideEthersApi, providePolkadotApi,
  provideWeb3Api
} from './providers';
import { sleep } from './utils';

import type { BlockHash } from '@polkadot/types/interfaces/chain/types';

const BINARY_PATH = '../target/release/bifrost-node';

export interface IBlockCreation {
  parentHash?: BlockHash;
  finalize?: boolean;
  transactions?: string[];
}

export interface INodeContext {
  createWeb3: (protocol?: 'ws' | 'http') => Promise<EnhancedWeb3>;
  createEthers: () => Promise<ethers.providers.JsonRpcProvider>;
  createPolkadotApi: () => Promise<ApiPromise>;

  createBlock: (options?: IBlockCreation) => Promise<{
    txResults: JsonRpcResponse[];
    block: {
      duration: number;
      hash: BlockHash;
    };
  }>;

  web3: EnhancedWeb3;
  ethers: ethers.providers.JsonRpcProvider;
  polkadotApi: ApiPromise;
  rpcPort: number;
  ethTransactionType?: EthTransactionType;
}

interface IInternalNodeContext extends INodeContext {
  _polkadotApis: ApiPromise[];
  _web3Providers: HttpProvider[];
}

type EthTransactionType = 'Legacy' | 'EIP2930' | 'EIP1559';

export function describeDevNode(
  title: string,
  cb: (context: INodeContext) => void,
  multi: boolean = false,
  ethTransactionType: EthTransactionType = 'Legacy',
) {
  describe(title, function () {
    // Set timeout to 5000 for all tests.
    this.timeout(20000);

    // The context is initialized empty to allow passing a reference
    // and to be filled once the node information is retrieved
    let context: IInternalNodeContext = { ethTransactionType } as IInternalNodeContext;

    // The currently running node for this describe
    let node: ChildProcess | null;

    // Making sure the node has started
    before('starting dev node', async function () {
      this.timeout(SPAWNING_TIME);
      const init = !DEBUG_MODE
        ? multi ? await startMultiDevNode() : await startSingleDevNode()
        : {
          runningNode: null,
          p2pPort: 30333,
          rpcPort: 9933,
          wsPort: 9945,
        };

      node = init.runningNode;
      context.rpcPort = init.rpcPort;

      // Context is given prior to this assignement, so doing
      // context = init.context will fail because it replace the variable;

      context._polkadotApis = [];
      context._web3Providers = [];
      node = init.runningNode;

      context.createWeb3 = async (protocol: 'ws' | 'http' = 'http') => {
        const provider =
          protocol == 'ws'
            ? await provideWeb3Api(init.wsPort, 'ws')
            : await provideWeb3Api(init.rpcPort, 'http');
        context._web3Providers.push((provider as any)._provider);
        return provider;
      };
      context.createEthers = async () => provideEthersApi(init.rpcPort);

      context.createPolkadotApi = async () => {
        const apiPromise = await providePolkadotApi(init.rpcPort);
        // We keep track of the polkadotApis to close them at the end of the test
        context._polkadotApis.push(apiPromise);
        await apiPromise.isReady;
        if (multi) {
          await setTimeout(2000);
        } else {
          await setTimeout(500);
        }
        // Necessary hack to allow polkadotApi to finish its internal metadata loading
        // apiPromise.isReady unfortunately doesn't wait for those properly
        return apiPromise;
      };

      context.polkadotApi = await context.createPolkadotApi();
      context.web3 = await context.createWeb3();
      context.ethers = await context.createEthers();

      context.createBlock = async <T>(options: IBlockCreation = {}) => {
        let { parentHash, finalize, transactions = [] } = options;

        let txResults = await Promise.all(
          transactions.map((t) => customWeb3Request(context.web3, 'eth_sendRawTransaction', [t]))
        );
        const block = await createAndFinalizeBlock(context.polkadotApi, parentHash, finalize);
        return {
          txResults,
          block,
        };
      };
    });

    after(async function () {
      await Promise.all(context._web3Providers.map((p) => p.disconnect()));
      await Promise.all(context._polkadotApis.map((p) => p.disconnect()));

      if (node) {
        node.kill();
        node = null;
      }
    });

    cb(context);
  });
}

export async function startMultiDevNode() {
  await startSingleDevNode(9934, true);
  return await startSingleDevNode(9933, true);
}

export async function startSingleDevNode(overrideRpcPort?: number | null, multi: boolean = false,) {
  if (multi) {
    await sleep(3000);
  } else {
    await sleep(500);
  }

  let { p2pPort, rpcPort, wsPort } = await findAvailablePorts();
  if (overrideRpcPort) {
    rpcPort = overrideRpcPort;
  }

  const cmd = BINARY_PATH;
  let args = [
    `--dev`,
    `--sealing`,
    `--port=${p2pPort}`,
    `--rpc-port=${rpcPort}`,
    `--ethapi=debug,trace,txpool`
  ];
  console.debug(`starting dev node: --port=${p2pPort} --rpc-port=${rpcPort} --ws-port=${wsPort}`);

  let runningNode: ChildProcess | null = null;

  const onProcessExit = function () {
    runningNode && runningNode.kill();
  };
  const onProcessInterrupt = function () {
    process.exit(2);
  };

  process.once('exit', onProcessExit);
  process.once('SIGINT', onProcessInterrupt);
  runningNode = spawn(cmd, args);

  runningNode.once('exit', () => {
    process.removeListener('exit', onProcessExit);
    process.removeListener('SIGINT', onProcessInterrupt);
    console.debug(`exiting dev node: --port=${p2pPort} --rpc-port=${rpcPort} --ws-port=${wsPort}`);
  });

  runningNode.on('error', (err) => {
    if ((err as any).errno == 'ENOENT') {
      console.error(
        `\x1b[31mMissing node binary ` +
        `(${BINARY_PATH}).\nPlease compile the node project\x1b[0m`
      );
    } else {
      console.error(err);
    }
    process.exit(1);
  });

  return { p2pPort, rpcPort, wsPort, runningNode };
}

export async function findAvailablePorts() {
  const availablePorts = await Promise.all(
    [null, null, null].map(async (_, index) => {
      let selectedPort = 0;
      let port = 1024 + index * 20000 + (process.pid % 20000);
      let endingPort = 65535;
      while (!selectedPort && port < endingPort) {
        const inUse = await tcpPortUsed.check(port, '127.0.0.1');
        if (!inUse) {
          selectedPort = port;
        }
        port++;
      }
      if (!selectedPort) {
        throw new Error(`No available port`);
      }
      return selectedPort;
    })
  );

  return {
    p2pPort: availablePorts[0],
    rpcPort: availablePorts[1],
    wsPort: availablePorts[2],
  };
}
