import { expect } from 'chai';
import { ethers } from 'ethers';
import { TransactionReceiptAPI } from 'web3';

import { Keyring } from '@polkadot/api';
import { TypeRegistry } from '@polkadot/types';
import { u8aConcat } from '@polkadot/util';

import { DEMO_ABI, DEMO_BYTE_CODE, DEMO_SOCKET_ABI } from '../../constants/demo_contract';
import { TEST_CONTROLLERS, TEST_RELAYERS } from '../../constants/keys';
import { getExtrinsicResult } from '../extrinsics';
import { describeDevNode, INodeContext } from '../set_dev_node';

// ============================================================
// Constants
// ============================================================

const BFC_ASSET_ID = '0xffffffffffffffffffffffffffffffffffffffff';

// Asset index hashes for BFC
const BFC_ASSET_INDEX_1 = '0x00000001ffffffff00aa36a7469b7b5e119348ad7d61aa8ac38101cdc2b42a22';
const BFC_ASSET_INDEX_2 = '0x00000001000000010000bfc0ffffffffffffffffffffffffffffffffffffffff';
const BFC_ASSET_INDEX_3 = '0x00000001000000030000bfc0c7d85aaeba3b5d36e87794b42506246240148535';

// Asset index hashes for test ERC20 (mapped to deployed contract address at runtime)
const ERC20_ASSET_INDEX_1 = '0x00000004000000030000bfc0ebf923916f4ed9afe9ca1e9df4ed98f0902c03e5';
const ERC20_ASSET_INDEX_2 = '0x000000040000000100aa36a7ffffffffffffffffffffffffffffffffffffffff';

// Max on-flight caps
const BFC_MAX_CAP = '1000000000000000000000000';    // 1,000,000 * 10^18
const ERC20_MAX_CAP = '100000000000000000000';       // 100 * 10^18
const CAP_TOO_LARGE = '100000000000000000000000001';  // MAX_ON_FLIGHT_CAP + 1

// Test oracle addresses
const TEST_ORACLE_1 = '0x0000000000000000000000000000000000100001';
const TEST_ORACLE_2 = '0x0000000000000000000000000000000000100002';

// Socket message for inbound request (Sepolia 11155111 -> Bifrost 49088), status = REQUESTED (1)
// asset_index_hash = ERC20_ASSET_INDEX_2
const INBOUND_SOCKET_MESSAGE = '0x000000000000000000000000000000000000000000000000000000000000002000aa36a7000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005834000000000000000000000000000000000000000000000000000000000000000f00000000000000000000000000000000000000000000000000000000000000010000bfc000000000000000000000000000000000000000000000000000000000030101020000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000e0000000040000000100aa36a7ffffffffffffffffffffffffffffffffffffffff0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000fa2789d80e1f3954aada2d6da1785a9cf6bbae8b000000000000000000000000d52e34b9e819a5b980357d168254ce6ff47c397b0000000000000000000000000000000000000000000000000023867e056e780000000000000000000000000000000000000000000000000000000000000000c000000000000000000000000000000000000000000000000000000000000000e000000000000000000000000055b57a7a0f41d668c584b2246d373b639084eaed000000000000000000000000c96971f6f5a1d20efcd465b1163812a955b414a3000000000000000000000000fa2789d80e1f3954aada2d6da1785a9cf6bbae8b00000000000000000000000000000000000000000000000000038d7ea4c6800000000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000000000000548656c6c6f000000000000000000000000000000000000000000000000000000';

// Socket message for outbound request (Bifrost 49088 -> Sepolia 11155111), status = REQUESTED (1)
// asset_index_hash = ERC20_ASSET_INDEX_1
const OUTBOUND_SOCKET_MESSAGE = '0x00000000000000000000000000000000000000000000000000000000000000200000bfc000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000583500000000000000000000000000000000000000000000000000000000000000cd000000000000000000000000000000000000000000000000000000000000000100aa36a700000000000000000000000000000000000000000000000000000000030203010000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000e000000004000000030000bfc0ebf923916f4ed9afe9ca1e9df4ed98f0902c03e500000000000000000000000000000000000000000000000000000000000000000000000000000000000000008fe69a3387fdc11e6fddd6d455225e682998a19800000000000000000000000069b0731c5972f171a8e58569b188e4d27cf658d60000000000000000000000000000000000000000000000000023867e056e780000000000000000000000000000000000000000000000000000000000000000c000000000000000000000000000000000000000000000000000000000000000e000000000000000000000000055b57a7a0f41d668c584b2246d373b639084eaed000000000000000000000000f55d50af9a18b9875ac1afb87f93273da177ac6d0000000000000000000000008fe69a3387fdc11e6fddd6d455225e682998a19800000000000000000000000000000000000000000000000000038d7ea4c6800000000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000000000000548656c6c6f000000000000000000000000000000000000000000000000000000';

// Test source transaction IDs
const TEST_SRC_TX_ID_1 = '0x1111111111111111111111111111111111111111111111111111111111111111';
const TEST_SRC_TX_ID_2 = '0x2222222222222222222222222222222222222222222222222222222222222222';

// ============================================================
// Helper Functions
// ============================================================

const scaleRegistry = new TypeRegistry();

/**
 * Deploy a simple contract and return its address.
 * Uses DEMO_BYTE_CODE as a stand-in for any ERC20 token address.
 */
async function deployTestContract(context: INodeContext): Promise<string> {
  const deployTx = ((new context.web3.eth.Contract(DEMO_ABI) as any).deploy({
    data: DEMO_BYTE_CODE,
  }));
  const receipt = await sendTx(context, deployTx, null);
  expect(receipt).is.ok;
  expect(receipt?.contractAddress).is.ok;
  return receipt?.contractAddress ?? '';
}

/**
 * Send an EVM transaction via web3 (signed by Dorothy / TEST_CONTROLLERS[3]).
 */
const sendTx = async (context: INodeContext, tx: any, to: string | null): Promise<TransactionReceiptAPI | undefined> => {
  const signedTx = (await context.web3.eth.accounts.signTransaction({
    to,
    from: TEST_CONTROLLERS[3].public,
    data: tx.encodeABI(),
    gasPrice: context.web3.utils.toWei(1000, 'gwei'),
    gas: 3000000,
  }, TEST_CONTROLLERS[3].private)).rawTransaction;

  const txHash = await context.web3.requestManager.send({
    method: 'eth_sendRawTransaction',
    params: [signedTx],
  });
  expect(txHash).is.ok;

  await context.createBlock();
  await context.createBlock();

  const receipt = await context.web3.requestManager.send({
    method: 'eth_getTransactionReceipt',
    params: [txHash],
  });
  expect(receipt).is.ok;
  expect(receipt?.status).equal('0x1');

  return receipt;
};

/**
 * Compute keccak256 hash of hex-encoded bytes.
 */
function computeMsgHash(msgHex: string): string {
  return ethers.keccak256(msgHex);
}

/**
 * Create a signature for on_flight_poll's validate_unsigned.
 *
 * Signed message format:
 *   keccak256("OnFlightPoll") || SCALE_encode((msg: Vec<u8>, msg_hash: H256, src_tx_id: H256))
 *
 * EthereumSignature::verify hashes the message with keccak256 before ecrecover.
 */
function signOnFlightPoll(
  msg: string,
  msgHash: string,
  srcTxId: string,
  privateKey: string,
): string {
  // keccak256("OnFlightPoll") → 32 bytes
  const prefixHash = ethers.getBytes(ethers.keccak256(ethers.toUtf8Bytes('OnFlightPoll')));

  // SCALE encode each element of the tuple (Bytes, H256, H256)
  const msgCodec = scaleRegistry.createType('Bytes', msg);
  const msgHashCodec = scaleRegistry.createType('H256', msgHash);
  const srcTxIdCodec = scaleRegistry.createType('H256', srcTxId);
  const scaleEncoded = u8aConcat(msgCodec.toU8a(), msgHashCodec.toU8a(), srcTxIdCodec.toU8a());

  // Full message = prefix || SCALE-encoded tuple
  const fullMessage = u8aConcat(prefixHash, scaleEncoded);

  // keccak256(fullMessage) → 32-byte digest, then raw ECDSA sign
  const digest = ethers.keccak256(fullMessage);
  const signingKey = new ethers.SigningKey(privateKey);
  const sig = signingKey.sign(digest);

  return sig.serialized; // 65 bytes: r(32) || s(32) || v(1) with v=27|28
}

/**
 * Create a signature for finalize_poll's validate_unsigned.
 *
 * Signed message format:
 *   keccak256("FinalizePoll") || raw_msg_bytes
 */
function signFinalizePoll(msg: string, privateKey: string): string {
  const prefixHash = ethers.getBytes(ethers.keccak256(ethers.toUtf8Bytes('FinalizePoll')));
  const msgBytes = ethers.getBytes(msg);

  const fullMessage = u8aConcat(prefixHash, msgBytes);

  const digest = ethers.keccak256(fullMessage);
  const signingKey = new ethers.SigningKey(privateKey);
  const sig = signingKey.sign(digest);

  return sig.serialized;
}

/**
 * Add an asset via sudo call.
 */
async function addAssetViaSudo(
  context: INodeContext,
  sudo: any,
  assetId: string,
  oracleId: string,
  maxCap: string,
  indexes: string[],
) {
  await context.polkadotApi.tx.sudo.sudo(
    context.polkadotApi.tx.cccpRelayQueue.addAsset(assetId, oracleId, maxCap, indexes)
  ).signAndSend(sudo);
  await context.createBlock();
}

// ============================================================
// Test Suite 1: set_socket
// ============================================================

describeDevNode('pallet_cccp_relay_queue - set_socket', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const sudo = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const nonSudo = keyring.addFromUri(TEST_CONTROLLERS[1].private);
  const socketAddress = TEST_CONTROLLERS[2].public; // arbitrary address

  it('should fail to set socket without root origin', async function () {
    await context.polkadotApi.tx.cccpRelayQueue.setSocket(socketAddress).signAndSend(nonSudo);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'cccpRelayQueue', 'setSocket');
    expect(extrinsicResult).eq('BadOrigin');
  });

  it('should successfully set socket via sudo', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.cccpRelayQueue.setSocket(socketAddress)
    ).signAndSend(sudo);
    await context.createBlock();

    const rawSocket: any = await context.polkadotApi.query.cccpRelayQueue.socket();
    const socket = rawSocket.toJSON();
    expect(socket).is.ok;
    expect(socket.toLowerCase()).eq(socketAddress.toLowerCase());
  });

  it('should fail to set socket with same value', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.cccpRelayQueue.setSocket(socketAddress)
    ).signAndSend(sudo);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'sudo', 'sudo');
    expect(extrinsicResult).is.not.null;
  });

  it('should successfully update socket to a new address', async function () {
    const newSocketAddress = TEST_CONTROLLERS[3].public;
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.cccpRelayQueue.setSocket(newSocketAddress)
    ).signAndSend(sudo);
    await context.createBlock();

    const rawSocket: any = await context.polkadotApi.query.cccpRelayQueue.socket();
    const socket = rawSocket.toJSON();
    expect(socket.toLowerCase()).eq(newSocketAddress.toLowerCase());
  });
});

// ============================================================
// Test Suite 2: add_asset
// ============================================================

describeDevNode('pallet_cccp_relay_queue - add_asset', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const sudo = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  let testErc20Address: string;

  before('deploy test ERC20 contract', async function () {
    testErc20Address = await deployTestContract(context);
    expect(testErc20Address).is.ok;
  });

  it('should fail to add asset - empty asset indexes', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.cccpRelayQueue.addAsset(BFC_ASSET_ID, TEST_ORACLE_1, BFC_MAX_CAP, [])
    ).signAndSend(sudo);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'sudo', 'sudo');
    expect(extrinsicResult).is.not.null;
  });

  it('should fail to add asset - zero max cap', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.cccpRelayQueue.addAsset(BFC_ASSET_ID, TEST_ORACLE_1, '0', [BFC_ASSET_INDEX_1])
    ).signAndSend(sudo);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'sudo', 'sudo');
    expect(extrinsicResult).is.not.null;
  });

  it('should fail to add asset - cap too large', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.cccpRelayQueue.addAsset(BFC_ASSET_ID, TEST_ORACLE_1, CAP_TOO_LARGE, [BFC_ASSET_INDEX_1])
    ).signAndSend(sudo);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'sudo', 'sudo');
    expect(extrinsicResult).is.not.null;
  });

  it('should fail to add asset - duplicate asset indexes', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.cccpRelayQueue.addAsset(
        BFC_ASSET_ID, TEST_ORACLE_1, BFC_MAX_CAP,
        [BFC_ASSET_INDEX_1, BFC_ASSET_INDEX_1] // duplicate
      )
    ).signAndSend(sudo);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'sudo', 'sudo');
    expect(extrinsicResult).is.not.null;
  });

  it('should successfully add BFC asset', async function () {
    await addAssetViaSudo(
      context, sudo, BFC_ASSET_ID, TEST_ORACLE_1, BFC_MAX_CAP,
      [BFC_ASSET_INDEX_1, BFC_ASSET_INDEX_2, BFC_ASSET_INDEX_3]
    );

    // Verify AssetCaps storage
    const rawAssetCap: any = await context.polkadotApi.query.cccpRelayQueue.assetCaps(BFC_ASSET_ID);
    const assetCap = rawAssetCap.toJSON();
    expect(assetCap).is.ok;
    expect(assetCap.maxOnFlightCap).is.ok;

    // Verify AssetIndexes storage
    const rawIndex1: any = await context.polkadotApi.query.cccpRelayQueue.assetIndexes(BFC_ASSET_INDEX_1);
    const index1 = rawIndex1.toJSON();
    expect(index1).is.ok;
    expect(index1.toLowerCase()).eq(BFC_ASSET_ID.toLowerCase());

    const rawIndex2: any = await context.polkadotApi.query.cccpRelayQueue.assetIndexes(BFC_ASSET_INDEX_2);
    const index2 = rawIndex2.toJSON();
    expect(index2).is.ok;

    const rawIndex3: any = await context.polkadotApi.query.cccpRelayQueue.assetIndexes(BFC_ASSET_INDEX_3);
    const index3 = rawIndex3.toJSON();
    expect(index3).is.ok;

    // Verify AssetOracles storage
    const rawOracle: any = await context.polkadotApi.query.cccpRelayQueue.assetOracles(BFC_ASSET_ID);
    const oracle = rawOracle.toJSON();
    expect(oracle).is.ok;
    expect(oracle.toLowerCase()).eq(TEST_ORACLE_1.toLowerCase());
  });

  it('should fail to add asset - asset already exists', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.cccpRelayQueue.addAsset(
        BFC_ASSET_ID, TEST_ORACLE_1, BFC_MAX_CAP,
        ['0x0000000000000000000000000000000000000000000000000000000000000099'] // new index
      )
    ).signAndSend(sudo);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'sudo', 'sudo');
    expect(extrinsicResult).is.not.null;
  });

  it('should fail to add asset - asset index already exists', async function () {
    // Try to register a different asset with an index that BFC already owns
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.cccpRelayQueue.addAsset(
        testErc20Address, TEST_ORACLE_2, ERC20_MAX_CAP,
        [BFC_ASSET_INDEX_1] // already registered to BFC
      )
    ).signAndSend(sudo);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'sudo', 'sudo');
    expect(extrinsicResult).is.not.null;
  });

  it('should successfully add ERC20 asset', async function () {
    await addAssetViaSudo(
      context, sudo, testErc20Address, TEST_ORACLE_2, ERC20_MAX_CAP,
      [ERC20_ASSET_INDEX_1, ERC20_ASSET_INDEX_2]
    );

    // Verify AssetCaps
    const rawAssetCap: any = await context.polkadotApi.query.cccpRelayQueue.assetCaps(testErc20Address);
    const assetCap = rawAssetCap.toJSON();
    expect(assetCap).is.ok;

    // Verify AssetIndexes
    const rawIndex1: any = await context.polkadotApi.query.cccpRelayQueue.assetIndexes(ERC20_ASSET_INDEX_1);
    expect(rawIndex1.toJSON()).is.ok;
    const rawIndex2: any = await context.polkadotApi.query.cccpRelayQueue.assetIndexes(ERC20_ASSET_INDEX_2);
    expect(rawIndex2.toJSON()).is.ok;

    // Verify AssetOracles
    const rawOracle: any = await context.polkadotApi.query.cccpRelayQueue.assetOracles(testErc20Address);
    expect(rawOracle.toJSON()).is.ok;
  });

  it('should fail to add asset - too many asset indexes', async function () {
    const tooManyIndexes = Array.from({ length: 101 }, (_, i) =>
      '0x' + (i + 1000).toString(16).padStart(64, '0')
    );
    const randomAssetId = '0x0000000000000000000000000000000000099999';

    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.cccpRelayQueue.addAsset(randomAssetId, TEST_ORACLE_1, ERC20_MAX_CAP, tooManyIndexes)
    ).signAndSend(sudo);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'sudo', 'sudo');
    expect(extrinsicResult).is.not.null;
  });
});

// ============================================================
// Test Suite 3: remove_asset
// ============================================================

describeDevNode('pallet_cccp_relay_queue - remove_asset', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const sudo = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  let testErc20Address: string;

  before('setup: deploy ERC20 and add asset', async function () {
    testErc20Address = await deployTestContract(context);
    await addAssetViaSudo(
      context, sudo, testErc20Address, TEST_ORACLE_2, ERC20_MAX_CAP,
      [ERC20_ASSET_INDEX_1, ERC20_ASSET_INDEX_2]
    );
  });

  it('should fail to remove asset - asset does not exist', async function () {
    const nonExistentAsset = '0x0000000000000000000000000000000000000001';
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.cccpRelayQueue.removeAsset(nonExistentAsset)
    ).signAndSend(sudo);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'sudo', 'sudo');
    expect(extrinsicResult).is.not.null;
  });

  it('should successfully remove ERC20 asset', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.cccpRelayQueue.removeAsset(testErc20Address)
    ).signAndSend(sudo);
    await context.createBlock();

    // Verify AssetCaps removed
    const rawAssetCap: any = await context.polkadotApi.query.cccpRelayQueue.assetCaps(testErc20Address);
    expect(rawAssetCap.toJSON()).is.null;

    // Verify AssetIndexes removed
    const rawIndex1: any = await context.polkadotApi.query.cccpRelayQueue.assetIndexes(ERC20_ASSET_INDEX_1);
    expect(rawIndex1.toJSON()).is.null;

    const rawIndex2: any = await context.polkadotApi.query.cccpRelayQueue.assetIndexes(ERC20_ASSET_INDEX_2);
    expect(rawIndex2.toJSON()).is.null;

    // Verify AssetOracles removed
    const rawOracle: any = await context.polkadotApi.query.cccpRelayQueue.assetOracles(testErc20Address);
    expect(rawOracle.toJSON()).is.null;
  });
});

// ============================================================
// Test Suite 4: update_asset
// ============================================================

describeDevNode('pallet_cccp_relay_queue - update_asset', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const sudo = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const NEW_ASSET_INDEX = '0x0000000000000000000000000000000000000000000000000000000000aaaaaa';
  const NEW_ASSET_INDEX_2 = '0x0000000000000000000000000000000000000000000000000000000000bbbbbb';

  before('setup: add BFC asset', async function () {
    await addAssetViaSudo(
      context, sudo, BFC_ASSET_ID, TEST_ORACLE_1, BFC_MAX_CAP,
      [BFC_ASSET_INDEX_1, BFC_ASSET_INDEX_2]
    );
  });

  it('should fail to update asset - empty submission (no fields provided)', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.cccpRelayQueue.updateAsset(BFC_ASSET_ID, null, null, null, null)
    ).signAndSend(sudo);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'sudo', 'sudo');
    expect(extrinsicResult).is.not.null;
  });

  it('should fail to update asset - asset does not exist', async function () {
    const nonExistent = '0x0000000000000000000000000000000000000001';
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.cccpRelayQueue.updateAsset(nonExistent, TEST_ORACLE_2, null, null, null)
    ).signAndSend(sudo);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'sudo', 'sudo');
    expect(extrinsicResult).is.not.null;
  });

  it('should successfully update asset oracle', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.cccpRelayQueue.updateAsset(BFC_ASSET_ID, TEST_ORACLE_2, null, null, null)
    ).signAndSend(sudo);
    await context.createBlock();

    const rawOracle: any = await context.polkadotApi.query.cccpRelayQueue.assetOracles(BFC_ASSET_ID);
    const oracle = rawOracle.toJSON();
    expect(oracle.toLowerCase()).eq(TEST_ORACLE_2.toLowerCase());
  });

  it('should fail to update asset - same value oracle', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.cccpRelayQueue.updateAsset(BFC_ASSET_ID, TEST_ORACLE_2, null, null, null)
    ).signAndSend(sudo);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'sudo', 'sudo');
    expect(extrinsicResult).is.not.null;
  });

  it('should successfully update max on-flight cap', async function () {
    const newCap = '2000000000000000000000000'; // 2M * 10^18
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.cccpRelayQueue.updateAsset(BFC_ASSET_ID, null, newCap, null, null)
    ).signAndSend(sudo);
    await context.createBlock();

    const rawAssetCap: any = await context.polkadotApi.query.cccpRelayQueue.assetCaps(BFC_ASSET_ID);
    const assetCap = rawAssetCap.toJSON();
    expect(assetCap).is.ok;
  });

  it('should fail to update asset - same value max cap', async function () {
    const sameCap = '2000000000000000000000000';
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.cccpRelayQueue.updateAsset(BFC_ASSET_ID, null, sameCap, null, null)
    ).signAndSend(sudo);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'sudo', 'sudo');
    expect(extrinsicResult).is.not.null;
  });

  it('should fail to update asset - max cap zero', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.cccpRelayQueue.updateAsset(BFC_ASSET_ID, null, '0', null, null)
    ).signAndSend(sudo);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'sudo', 'sudo');
    expect(extrinsicResult).is.not.null;
  });

  it('should fail to update asset - cap too large', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.cccpRelayQueue.updateAsset(BFC_ASSET_ID, null, CAP_TOO_LARGE, null, null)
    ).signAndSend(sudo);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'sudo', 'sudo');
    expect(extrinsicResult).is.not.null;
  });

  it('should successfully add new asset indexes', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.cccpRelayQueue.updateAsset(
        BFC_ASSET_ID, null, null,
        [NEW_ASSET_INDEX, NEW_ASSET_INDEX_2], // add
        null // remove
      )
    ).signAndSend(sudo);
    await context.createBlock();

    const rawIndex: any = await context.polkadotApi.query.cccpRelayQueue.assetIndexes(NEW_ASSET_INDEX);
    const index = rawIndex.toJSON();
    expect(index).is.ok;
    expect(index.toLowerCase()).eq(BFC_ASSET_ID.toLowerCase());
  });

  it('should fail to add asset indexes - already exists', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.cccpRelayQueue.updateAsset(
        BFC_ASSET_ID, null, null,
        [NEW_ASSET_INDEX], // already added above
        null
      )
    ).signAndSend(sudo);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'sudo', 'sudo');
    expect(extrinsicResult).is.not.null;
  });

  it('should successfully remove asset indexes', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.cccpRelayQueue.updateAsset(
        BFC_ASSET_ID, null, null,
        null,
        [NEW_ASSET_INDEX, NEW_ASSET_INDEX_2] // remove
      )
    ).signAndSend(sudo);
    await context.createBlock();

    const rawIndex: any = await context.polkadotApi.query.cccpRelayQueue.assetIndexes(NEW_ASSET_INDEX);
    expect(rawIndex.toJSON()).is.null;

    const rawIndex2: any = await context.polkadotApi.query.cccpRelayQueue.assetIndexes(NEW_ASSET_INDEX_2);
    expect(rawIndex2.toJSON()).is.null;
  });

  it('should fail to update asset - conflicting add/remove operations', async function () {
    const conflictIndex = '0x00000000000000000000000000000000000000000000000000000000cccccccc';
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.cccpRelayQueue.updateAsset(
        BFC_ASSET_ID, null, null,
        [conflictIndex], // add
        [conflictIndex]  // remove same index
      )
    ).signAndSend(sudo);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'sudo', 'sudo');
    expect(extrinsicResult).is.not.null;
  });

  it('should fail to update asset - duplicate within add indexes', async function () {
    const dupIndex = '0x00000000000000000000000000000000000000000000000000000000dddddddd';
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.cccpRelayQueue.updateAsset(
        BFC_ASSET_ID, null, null,
        [dupIndex, dupIndex], // duplicate
        null
      )
    ).signAndSend(sudo);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'sudo', 'sudo');
    expect(extrinsicResult).is.not.null;
  });
});

// ============================================================
// Test Suite 5: native_currency_oracle
// ============================================================

describeDevNode('pallet_cccp_relay_queue - native_currency_oracle', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const sudo = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const CHAIN_ID_1 = 11155111; // Sepolia
  const CHAIN_ID_2 = 1;       // Ethereum mainnet

  it('should successfully set native currency oracle', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.cccpRelayQueue.setNativeCurrencyOracle(CHAIN_ID_1, TEST_ORACLE_1)
    ).signAndSend(sudo);
    await context.createBlock();

    const rawOracle: any = await context.polkadotApi.query.cccpRelayQueue.nativeCurrencyOracles(CHAIN_ID_1);
    const oracle = rawOracle.toJSON();
    expect(oracle).is.ok;
    expect(oracle.toLowerCase()).eq(TEST_ORACLE_1.toLowerCase());
  });

  it('should fail to set native currency oracle - chain already exists', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.cccpRelayQueue.setNativeCurrencyOracle(CHAIN_ID_1, TEST_ORACLE_2)
    ).signAndSend(sudo);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'sudo', 'sudo');
    expect(extrinsicResult).is.not.null;
  });

  it('should successfully update native currency oracle', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.cccpRelayQueue.updateNativeCurrencyOracle(CHAIN_ID_1, TEST_ORACLE_2)
    ).signAndSend(sudo);
    await context.createBlock();

    const rawOracle: any = await context.polkadotApi.query.cccpRelayQueue.nativeCurrencyOracles(CHAIN_ID_1);
    const oracle = rawOracle.toJSON();
    expect(oracle.toLowerCase()).eq(TEST_ORACLE_2.toLowerCase());
  });

  it('should fail to update native currency oracle - chain does not exist', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.cccpRelayQueue.updateNativeCurrencyOracle(CHAIN_ID_2, TEST_ORACLE_1)
    ).signAndSend(sudo);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'sudo', 'sudo');
    expect(extrinsicResult).is.not.null;
  });

  it('should fail to update native currency oracle - same value', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.cccpRelayQueue.updateNativeCurrencyOracle(CHAIN_ID_1, TEST_ORACLE_2)
    ).signAndSend(sudo);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'sudo', 'sudo');
    expect(extrinsicResult).is.not.null;
  });

  it('should fail to remove native currency oracle - chain does not exist', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.cccpRelayQueue.removeNativeCurrencyOracle(CHAIN_ID_2)
    ).signAndSend(sudo);
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'sudo', 'sudo');
    expect(extrinsicResult).is.not.null;
  });

  it('should successfully remove native currency oracle', async function () {
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.cccpRelayQueue.removeNativeCurrencyOracle(CHAIN_ID_1)
    ).signAndSend(sudo);
    await context.createBlock();

    const rawOracle: any = await context.polkadotApi.query.cccpRelayQueue.nativeCurrencyOracles(CHAIN_ID_1);
    expect(rawOracle.toJSON()).is.null;
  });
});

// ============================================================
// Test Suite 6: on_flight_poll
// ============================================================

describeDevNode('pallet_cccp_relay_queue - on_flight_poll', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const sudo = keyring.addFromUri(TEST_CONTROLLERS[0].private);
  const nonRelayer = keyring.addFromUri(TEST_CONTROLLERS[1].private);

  let activeRelayer1: any;
  let activeRelayer2: any;
  let activeRelayer1Key: any; // KeyringPair

  let testErc20Address: string;

  const inboundMsgHash = computeMsgHash(INBOUND_SOCKET_MESSAGE);
  const outboundMsgHash = computeMsgHash(OUTBOUND_SOCKET_MESSAGE);

  before('setup: deploy ERC20 and register assets', async function () {
    // Fetch authorities
    const validators = await context.polkadotApi.query.session.validators();
    const authorities = validators.toJSON() as string[];
    console.log('Active Authorities:', authorities);

    const relayer1Info = TEST_RELAYERS.find(r => authorities.some(a => a.toLowerCase() === r.public.toLowerCase()));
    // Try to find a second one that is different
    const relayer2Info = TEST_RELAYERS.find(r => r !== relayer1Info && authorities.some(a => a.toLowerCase() === r.public.toLowerCase()));

    if (!relayer1Info) {
      // If we can't match any, fallback to first in list but warn
      console.warn('Could not find active relayer 1 in TEST_RELAYERS. Using default.');
      activeRelayer1 = TEST_RELAYERS[0];
    } else {
      activeRelayer1 = relayer1Info;
    }
    activeRelayer1Key = keyring.addFromUri(activeRelayer1.private);

    if (relayer2Info) {
      activeRelayer2 = relayer2Info;
    } else {
      console.warn('Only 1 active relayer found matching known keys. Using default for 2nd (will likely fail signature check if not authority).');
      activeRelayer2 = TEST_RELAYERS[1];
    }

    // Deploy test ERC20 contract
    testErc20Address = await deployTestContract(context);

    // Register BFC asset
    await addAssetViaSudo(
      context, sudo, BFC_ASSET_ID, TEST_ORACLE_1, BFC_MAX_CAP,
      [BFC_ASSET_INDEX_1, BFC_ASSET_INDEX_2, BFC_ASSET_INDEX_3]
    );

    // Register ERC20 asset (required for Fast transfer detection)
    await addAssetViaSudo(
      context, sudo, testErc20Address, TEST_ORACLE_2, ERC20_MAX_CAP,
      [ERC20_ASSET_INDEX_1, ERC20_ASSET_INDEX_2]
    );
  });

  it('should fail on_flight_poll - invalid authority (non-relayer)', async function () {
    const msgHash = inboundMsgHash;
    // Sign with a non-relayer key but claim to be that non-relayer
    const sig = signOnFlightPoll(
      INBOUND_SOCKET_MESSAGE, msgHash, TEST_SRC_TX_ID_1, TEST_CONTROLLERS[1].private
    );

    const submission = {
      authorityId: nonRelayer.address,
      msg: INBOUND_SOCKET_MESSAGE,
      msgHash: msgHash,
      srcTxId: TEST_SRC_TX_ID_1,
    };

    let errorMsg = '';
    await context.polkadotApi.tx.cccpRelayQueue.onFlightPoll(submission, sig).send().catch((err: Error) => {
      errorMsg = err.message;
    });
    await context.createBlock();

    expect(errorMsg).contains('Invalid');
  });

  it('should fail on_flight_poll - invalid signature', async function () {
    const msgHash = inboundMsgHash;
    // Sign with a wrong private key (Relayer 2's key but claim Relayer 1)
    const wrongSig = signOnFlightPoll(
      INBOUND_SOCKET_MESSAGE, msgHash, TEST_SRC_TX_ID_1, activeRelayer2.private
    );

    const submission = {
      authorityId: activeRelayer1Key.address,
      msg: INBOUND_SOCKET_MESSAGE,
      msgHash: msgHash,
      srcTxId: TEST_SRC_TX_ID_1,
    };

    let errorMsg = '';
    await context.polkadotApi.tx.cccpRelayQueue.onFlightPoll(submission, wrongSig).send().catch((err: Error) => {
      errorMsg = err.message;
    });
    await context.createBlock();

    expect(errorMsg).contains('bad signature');
  });

  it('should successfully approve transfer on first vote (inbound)', async function () {
    const msgHash = inboundMsgHash;
    const sig = signOnFlightPoll(
      INBOUND_SOCKET_MESSAGE, msgHash, TEST_SRC_TX_ID_1, activeRelayer1.private
    );

    const submission = {
      authorityId: activeRelayer1Key.address,
      msg: INBOUND_SOCKET_MESSAGE,
      msgHash: msgHash,
      srcTxId: TEST_SRC_TX_ID_1,
    };

    await context.polkadotApi.tx.cccpRelayQueue.onFlightPoll(submission, sig).send();
    await context.createBlock();

    // Should NOT be in PendingTransfers (approved immediately)
    const rawPending: any = await context.polkadotApi.query.cccpRelayQueue.pendingTransfers(msgHash, TEST_SRC_TX_ID_1);
    expect(rawPending.toJSON()).is.null;

    // Should be in OnFlightTransfers
    const rawOnFlight: any = await context.polkadotApi.query.cccpRelayQueue.onFlightTransfers(msgHash);
    const onFlight = rawOnFlight.toJSON();
    expect(onFlight).is.ok;
    expect(onFlight.onFlightVoters.length).gte(1);
    expect(onFlight.srcTxId.toLowerCase()).eq(TEST_SRC_TX_ID_1.toLowerCase());
  });

  it('should fail on_flight_poll - already voted (same authority)', async function () {
    const msgHash = inboundMsgHash;
    const sig = signOnFlightPoll(
      INBOUND_SOCKET_MESSAGE, msgHash, TEST_SRC_TX_ID_1, activeRelayer1.private
    );

    const submission = {
      authorityId: activeRelayer1Key.address,
      msg: INBOUND_SOCKET_MESSAGE,
      msgHash: msgHash,
      srcTxId: TEST_SRC_TX_ID_1,
    };

    // Re-submitting the same vote with identical parameters produces identical signature
    // Transaction pool rejects duplicate unsigned transactions before they reach pallet logic
    let errorMsg = '';
    await context.polkadotApi.tx.cccpRelayQueue.onFlightPoll(submission, sig).send().catch((err: Error) => {
      errorMsg = err.message;
    });

    // Expect transaction pool rejection (error 1013: Transaction Already Imported)
    expect(errorMsg).contains('1013');
  });

  it('should fail on_flight_poll - transfer already on flight', async function () {
    // Check if inbound transfer is on-flight (approved)
    const rawOnFlight: any = await context.polkadotApi.query.cccpRelayQueue.onFlightTransfers(inboundMsgHash);
    if (!rawOnFlight.toJSON()) {
      this.skip(); // Transfer not yet approved, skip this test
      return;
    }

    const sig = signOnFlightPoll(
      INBOUND_SOCKET_MESSAGE, inboundMsgHash, TEST_SRC_TX_ID_2, activeRelayer1.private
    );

    const submission = {
      authorityId: activeRelayer1Key.address,
      msg: INBOUND_SOCKET_MESSAGE,
      msgHash: inboundMsgHash,
      srcTxId: TEST_SRC_TX_ID_2, // different src_tx_id but same msg_hash
    };

    await context.polkadotApi.tx.cccpRelayQueue.onFlightPoll(submission, sig).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'cccpRelayQueue', 'onFlightPoll');
    expect(extrinsicResult).eq('TransferAlreadyOnFlight');
  });

  it('should successfully approve outbound transfer on first vote (single validator)', async function () {
    const msgHash = outboundMsgHash;
    const sig = signOnFlightPoll(
      OUTBOUND_SOCKET_MESSAGE, msgHash, TEST_SRC_TX_ID_2, activeRelayer1.private
    );

    const submission = {
      authorityId: activeRelayer1Key.address,
      msg: OUTBOUND_SOCKET_MESSAGE,
      msgHash: msgHash,
      srcTxId: TEST_SRC_TX_ID_2,
    };

    await context.polkadotApi.tx.cccpRelayQueue.onFlightPoll(submission, sig).send();
    await context.createBlock();

    // With single validator, majority=1, so first vote immediately approves
    // Transfer skips PendingTransfers and goes directly to OnFlightTransfers
    const rawOnFlight: any = await context.polkadotApi.query.cccpRelayQueue.onFlightTransfers(msgHash);
    const onFlight = rawOnFlight.toJSON();
    expect(onFlight).is.ok;
    expect(onFlight.onFlightVoters).is.ok;
    expect(onFlight.onFlightVoters.length).gte(1);
    expect(onFlight.srcTxId.toLowerCase()).eq(TEST_SRC_TX_ID_2.toLowerCase());
  });

  it('should fail on_flight_poll - different src_tx_id for already approved message', async function () {
    // Verify outbound transfer is already on-flight (approved in previous test)
    const msgHash = outboundMsgHash;
    const rawOnFlight: any = await context.polkadotApi.query.cccpRelayQueue.onFlightTransfers(msgHash);
    if (!rawOnFlight.toJSON()) {
      this.skip(); // Transfer not yet approved, skip this test
      return;
    }

    // Try to submit same message with different src_tx_id
    // This should fail because only one transfer per msg_hash is allowed
    const differentSrcTxId = '0x3333333333333333333333333333333333333333333333333333333333333333';
    const sig = signOnFlightPoll(
      OUTBOUND_SOCKET_MESSAGE, msgHash, differentSrcTxId, activeRelayer1.private
    );

    const submission = {
      authorityId: activeRelayer1Key.address,
      msg: OUTBOUND_SOCKET_MESSAGE,
      msgHash: msgHash,
      srcTxId: differentSrcTxId,
    };

    await context.polkadotApi.tx.cccpRelayQueue.onFlightPoll(submission, sig).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'cccpRelayQueue', 'onFlightPoll');
    expect(extrinsicResult).eq('TransferAlreadyOnFlight');
  });
});

// ============================================================
// Test Suite 7: finalize_poll
// ============================================================

const VALID_DEMO_SOCKET_BYTE_CODE = '6080604052348015600e575f5ffd5b506106728061001c5f395ff3fe608060405234801561000f575f5ffd5b5060043610610029575f3560e01c80638dac22041461002d575b5f5ffd5b6100476004803603810190610042919061034e565b61005d565b604051610054919061049d565b60405180910390f35b6100656102e0565b62aa36a760e01b825f01602081019061007e919061050c565b7bffffffffffffffffffffffffffffffffffffffffffffffffffffffff19161480156100c757506158348260200160208101906100bb9190610574565b67ffffffffffffffff16145b80156100f75750600f8260400160208101906100e391906105e4565b6fffffffffffffffffffffffffffffffff16145b15610170576101046102e0565b6005815f01515f6020811061011c5761011b61060f565b5b602002019060ff16908160ff16815250507f08637ad6dc50350a1f576e333f67b778a7ac42ad8bec51cf2e89cefab3576b485f1b816020018181525050636979bc00816040018181525050809150506102db565b61bfc060e01b825f016020810190610188919061050c565b7bffffffffffffffffffffffffffffffffffffffffffffffffffffffff19161480156101d157506158358260200160208101906101c59190610574565b67ffffffffffffffff16145b8015610201575060cd8260400160208101906101ed91906105e4565b6fffffffffffffffffffffffffffffffff16145b156102ce5761020e6102e0565b6007815f01515f602081106102265761022561060f565b5b602002019060ff16908160ff16815250506002815f01516001602081106102505761024f61060f565b5b602002019060ff16908160ff16815250506002815f015160036020811061027a5761027961060f565b5b602002019060ff16908160ff16815250507fe93f9a200a83bfe0f927464d6527d05c4321b1e0730f5d546a9e512de034f2025f1b81602001818152505063697ae7d3816040018181525050809150506102db565b6102d66102e0565b809150505b919050565b60405180606001604052806102f3610305565b81526020015f81526020015f81525090565b604051806104000160405280602090602082028036833780820191505090505090565b5f5ffd5b5f5ffd5b5f606082840312156103455761034461032c565b5b81905092915050565b5f6060828403121561036357610362610328565b5b5f61037084828501610330565b91505092915050565b5f60209050919050565b5f81905092915050565b5f819050919050565b5f60ff82169050919050565b6103ab81610396565b82525050565b5f6103bc83836103a2565b60208301905092915050565b5f602082019050919050565b6103dd81610379565b6103e78184610383565b92506103f28261038d565b805f5b8381101561042257815161040987826103b1565b9650610414836103c8565b9250506001810190506103f5565b505050505050565b5f819050919050565b61043c8161042a565b82525050565b5f819050919050565b61045481610442565b82525050565b61044082015f82015161046f5f8501826103d4565b506020820151610483610400850182610433565b50604082015161049761042085018261044b565b50505050565b5f610440820190506104b15f83018461045a565b92915050565b5f7fffffffff0000000000000000000000000000000000000000000000000000000082169050919050565b6104eb816104b7565b81146104f5575f5ffd5b50565b5f81359050610506816104e2565b92915050565b5f6020828403121561052157610520610328565b5b5f61052e848285016104f8565b91505092915050565b5f67ffffffffffffffff82169050919050565b61055381610537565b811461055d575f5ffd5b50565b5f8135905061056e8161054a565b92915050565b5f6020828403121561058957610588610328565b5b5f61059684828501610560565b91505092915050565b5f6fffffffffffffffffffffffffffffffff82169050919050565b6105c38161059f565b81146105cd575f5ffd5b50565b5f813590506105de816105ba565b92915050565b5f602082840312156105f9576105f8610328565b5b5f610606848285016105d0565b91505092915050565b7f4e487b71000000000000000000000000000000000000000000000000000000005f52603260045260245ffdfea264697066735822122078d07a285a72752011239daa4203515a710d26b4ec66248dcaa663ed94e37d9d64736f6c634300081f0033';

// Socket messages with COMMITTED status (status = 7)
const INBOUND_COMMITTED_MESSAGE = '0x000000000000000000000000000000000000000000000000000000000000002000aa36a7000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005834000000000000000000000000000000000000000000000000000000000000000f00000000000000000000000000000000000000000000000000000000000000070000bfc000000000000000000000000000000000000000000000000000000000030101020000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000e0000000040000000100aa36a7ffffffffffffffffffffffffffffffffffffffff0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000fa2789d80e1f3954aada2d6da1785a9cf6bbae8b000000000000000000000000d52e34b9e819a5b980357d168254ce6ff47c397b0000000000000000000000000000000000000000000000000023867e056e780000000000000000000000000000000000000000000000000000000000000000c000000000000000000000000000000000000000000000000000000000000000e000000000000000000000000055b57a7a0f41d668c584b2246d373b639084eaed000000000000000000000000c96971f6f5a1d20efcd465b1163812a955b414a3000000000000000000000000fa2789d80e1f3954aada2d6da1785a9cf6bbae8b00000000000000000000000000000000000000000000000000038d7ea4c6800000000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000000000000548656c6c6f000000000000000000000000000000000000000000000000000000';

const OUTBOUND_COMMITTED_MESSAGE = '0x00000000000000000000000000000000000000000000000000000000000000200000bfc000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000583500000000000000000000000000000000000000000000000000000000000000cd000000000000000000000000000000000000000000000000000000000000000700aa36a700000000000000000000000000000000000000000000000000000000030203010000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000e000000004000000030000bfc0ebf923916f4ed9afe9ca1e9df4ed98f0902c03e500000000000000000000000000000000000000000000000000000000000000000000000000000000000000008fe69a3387fdc11e6fddd6d455225e682998a19800000000000000000000000069b0731c5972f171a8e58569b188e4d27cf658d60000000000000000000000000000000000000000000000000023867e056e780000000000000000000000000000000000000000000000000000000000000000c000000000000000000000000000000000000000000000000000000000000000e000000000000000000000000055b57a7a0f41d668c584b2246d373b639084eaed000000000000000000000000f55d50af9a18b9875ac1afb87f93273da177ac6d0000000000000000000000008fe69a3387fdc11e6fddd6d455225e682998a19800000000000000000000000000000000000000000000000000038d7ea4c6800000000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000000000000548656c6c6f000000000000000000000000000000000000000000000000000000';

// Message that was never put on-flight (for testing TransferNotOnFlight error)
const NOT_ON_FLIGHT_MESSAGE = '0x00000000000000000000000000000000000000000000000000000000000000200000bfc000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000583500000000000000000000000000000000000000000000000000000000000000ca000000000000000000000000000000000000000000000000000000000000000700aa36a700000000000000000000000000000000000000000000000000000000030203010000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000e000000014000000030000bfc0fe1b0377ddf6ff4e685d4e21f6ba0249de1c60c6000000000000000000000000000000000000000000000000000000000000000000000000000000000000000055b57a7a0f41d668c584b2246d373b639084eaed00000000000000000000000055b57a7a0f41d668c584b2246d373b639084eaed0000000000000000000000000000000000000000000000000000000008f0d18000000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000000000000';

describeDevNode('pallet_cccp_relay_queue - finalize_poll', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const sudo = keyring.addFromUri(TEST_CONTROLLERS[0].private);

  let activeRelayer1: any;
  let activeRelayer1Key: any;
  let testErc20Address: string;
  let socketContractAddress: string;

  const inboundMsgHash = computeMsgHash(INBOUND_SOCKET_MESSAGE);
  const outboundMsgHash = computeMsgHash(OUTBOUND_SOCKET_MESSAGE);

  before('setup: deploy contracts, register assets, and set socket', async function () {
    // Fetch authorities
    const validators = await context.polkadotApi.query.session.validators();
    const authorities = validators.toJSON() as string[];

    const relayer1Info = TEST_RELAYERS.find(r => authorities.some(a => a.toLowerCase() === r.public.toLowerCase()));
    if (!relayer1Info) {
      activeRelayer1 = TEST_RELAYERS[0];
    } else {
      activeRelayer1 = relayer1Info;
    }
    activeRelayer1Key = keyring.addFromUri(activeRelayer1.private);

    // Deploy test ERC20 contract
    testErc20Address = await deployTestContract(context);

    // Deploy Socket contract with valid request info
    const deployTx = ((new context.web3.eth.Contract(DEMO_SOCKET_ABI) as any).deploy({
      data: VALID_DEMO_SOCKET_BYTE_CODE,
    }));
    const receipt = await sendTx(context, deployTx, null);
    expect(receipt).is.ok;
    expect(receipt?.contractAddress).is.ok;
    socketContractAddress = receipt?.contractAddress ?? '';

    // Set Socket contract address
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.cccpRelayQueue.setSocket(socketContractAddress)
    ).signAndSend(sudo);
    await context.createBlock();

    // Register BFC asset
    await addAssetViaSudo(
      context, sudo, BFC_ASSET_ID, TEST_ORACLE_1, BFC_MAX_CAP,
      [BFC_ASSET_INDEX_1, BFC_ASSET_INDEX_2, BFC_ASSET_INDEX_3]
    );

    // Register ERC20 asset
    await addAssetViaSudo(
      context, sudo, testErc20Address, TEST_ORACLE_2, ERC20_MAX_CAP,
      [ERC20_ASSET_INDEX_1, ERC20_ASSET_INDEX_2]
    );

    // Put inbound transfer on-flight for finalization tests
    const sig = signOnFlightPoll(
      INBOUND_SOCKET_MESSAGE, inboundMsgHash, TEST_SRC_TX_ID_1, activeRelayer1.private
    );
    const submission = {
      authorityId: activeRelayer1Key.address,
      msg: INBOUND_SOCKET_MESSAGE,
      msgHash: inboundMsgHash,
      srcTxId: TEST_SRC_TX_ID_1,
    };
    await context.polkadotApi.tx.cccpRelayQueue.onFlightPoll(submission, sig).send();
    await context.createBlock();

    // Put outbound transfer on-flight for finalization tests
    const outboundSig = signOnFlightPoll(
      OUTBOUND_SOCKET_MESSAGE, outboundMsgHash, TEST_SRC_TX_ID_2, activeRelayer1.private
    );
    const outboundSubmission = {
      authorityId: activeRelayer1Key.address,
      msg: OUTBOUND_SOCKET_MESSAGE,
      msgHash: outboundMsgHash,
      srcTxId: TEST_SRC_TX_ID_2,
    };
    await context.polkadotApi.tx.cccpRelayQueue.onFlightPoll(outboundSubmission, outboundSig).send();
    await context.createBlock();
  });

  it('should fail finalize_poll - transfer not on flight', async function () {
    // Use a message that was never approved via on_flight_poll
    const sig = signFinalizePoll(NOT_ON_FLIGHT_MESSAGE, activeRelayer1.private);

    await context.polkadotApi.tx.cccpRelayQueue.finalizePoll({
      authorityId: activeRelayer1Key.address,
      msg: NOT_ON_FLIGHT_MESSAGE,
    }, sig).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'cccpRelayQueue', 'finalizePoll');
    expect(extrinsicResult).eq('TransferNotOnFlight');
  });

  it('should successfully finalize outbound transfer on first vote (single validator)', async function () {
    const sig = signFinalizePoll(OUTBOUND_COMMITTED_MESSAGE, activeRelayer1.private);
    const submission = {
      authorityId: activeRelayer1Key.address,
      msg: OUTBOUND_COMMITTED_MESSAGE,
    };

    await context.polkadotApi.tx.cccpRelayQueue.finalizePoll(submission, sig).send();
    await context.createBlock();

    // Verify moved to FinalizedTransfers
    const rawFinalized: any = await context.polkadotApi.query.cccpRelayQueue.finalizedTransfers(outboundMsgHash);
    const finalized = rawFinalized.toJSON();
    expect(finalized).is.ok;
    expect(finalized.srcTxId.toLowerCase()).eq(TEST_SRC_TX_ID_2.toLowerCase());

    // Verify removed from OnFlightTransfers
    const rawOnFlightAfter: any = await context.polkadotApi.query.cccpRelayQueue.onFlightTransfers(outboundMsgHash);
    expect(rawOnFlightAfter.toJSON()).is.null;
  });

  it('should fail finalize_poll - transfer already finalized (duplicate transaction)', async function () {
    const sig = signFinalizePoll(OUTBOUND_COMMITTED_MESSAGE, activeRelayer1.private);
    const submission = {
      authorityId: activeRelayer1Key.address,
      msg: OUTBOUND_COMMITTED_MESSAGE,
    };

    // Try to submit same finalization again
    let errorMsg = '';
    await context.polkadotApi.tx.cccpRelayQueue.finalizePoll(submission, sig).send().catch((err: Error) => {
      errorMsg = err.message;
    });

    // Expect transaction pool rejection (duplicate transaction with identical signature)
    expect(errorMsg).contains('1013');
  });

  it('should fail finalize_poll - invalid authority (non-relayer)', async function () {
    // Create new on-flight transfer
    const newSrcTxId = '0x3333333333333333333333333333333333333333333333333333333333333333';
    const sig1 = signOnFlightPoll(INBOUND_SOCKET_MESSAGE, inboundMsgHash, newSrcTxId, activeRelayer1.private);
    await context.polkadotApi.tx.cccpRelayQueue.onFlightPoll({
      authorityId: activeRelayer1Key.address,
      msg: INBOUND_SOCKET_MESSAGE,
      msgHash: inboundMsgHash,
      srcTxId: newSrcTxId,
    }, sig1).send();
    await context.createBlock();

    // Try to finalize with non-relayer
    const nonRelayer = keyring.addFromUri(TEST_CONTROLLERS[1].private);
    const sig2 = signFinalizePoll(INBOUND_COMMITTED_MESSAGE, TEST_CONTROLLERS[1].private);

    let errorMsg = '';
    await context.polkadotApi.tx.cccpRelayQueue.finalizePoll({
      authorityId: nonRelayer.address,
      msg: INBOUND_COMMITTED_MESSAGE,
    }, sig2).send().catch((err: Error) => {
      errorMsg = err.message;
    });

    expect(errorMsg).contains('Invalid');
  });

  it('should fail finalize_poll - invalid signature', async function () {
    // Create new on-flight transfer
    const newSrcTxId = '0x4444444444444444444444444444444444444444444444444444444444444444';
    const sig1 = signOnFlightPoll(OUTBOUND_SOCKET_MESSAGE, outboundMsgHash, newSrcTxId, activeRelayer1.private);
    await context.polkadotApi.tx.cccpRelayQueue.onFlightPoll({
      authorityId: activeRelayer1Key.address,
      msg: OUTBOUND_SOCKET_MESSAGE,
      msgHash: outboundMsgHash,
      srcTxId: newSrcTxId,
    }, sig1).send();
    await context.createBlock();

    // Sign with wrong key but claim to be activeRelayer1
    const wrongSig = signFinalizePoll(OUTBOUND_COMMITTED_MESSAGE, TEST_CONTROLLERS[1].private);

    let errorMsg = '';
    await context.polkadotApi.tx.cccpRelayQueue.finalizePoll({
      authorityId: activeRelayer1Key.address,
      msg: OUTBOUND_COMMITTED_MESSAGE,
    }, wrongSig).send().catch((err: Error) => {
      errorMsg = err.message;
    });

    expect(errorMsg).contains('bad signature');
  });
});

// ============================================================
// Test Suite 8: Standard On-Flights
// ============================================================

// Standard transfer socket message (Bifrost 49088 -> Sepolia 11155111)
// This message represents a STANDARD outbound transfer (not Fast)
// asset_index_hash = 0x00000014000000030000bfc0fe1b0377ddf6ff4e685d4e21f6ba0249de1c60c6
const STANDARD_OUTBOUND_MESSAGE = '0x00000000000000000000000000000000000000000000000000000000000000200000bfc000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000583500000000000000000000000000000000000000000000000000000000000000ca000000000000000000000000000000000000000000000000000000000000000100aa36a700000000000000000000000000000000000000000000000000000000030203010000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000e000000014000000030000bfc0fe1b0377ddf6ff4e685d4e21f6ba0249de1c60c6000000000000000000000000000000000000000000000000000000000000000000000000000000000000000055b57a7a0f41d668c584b2246d373b639084eaed00000000000000000000000055b57a7a0f41d668c584b2246d373b639084eaed0000000000000000000000000000000000000000000000000000000008f0d18000000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000000000000';

// Asset index for standard transfer test
const STANDARD_ASSET_INDEX = '0x00000014000000030000bfc0fe1b0377ddf6ff4e685d4e21f6ba0249de1c60c6';

// Test source transaction IDs for standard transfers
const STANDARD_SRC_TX_ID_1 = '0x5555555555555555555555555555555555555555555555555555555555555555';
const STANDARD_SRC_TX_ID_2 = '0x6666666666666666666666666666666666666666666666666666666666666666';

describeDevNode('pallet_cccp_relay_queue - standard_on_flights', (context) => {
  const keyring = new Keyring({ type: 'ethereum' });
  const sudo = keyring.addFromUri(TEST_CONTROLLERS[0].private);

  let activeRelayer1: any;
  let activeRelayer1Key: any;
  let testAssetAddress: string;

  const standardMsgHash = computeMsgHash(STANDARD_OUTBOUND_MESSAGE);

  before('setup: deploy asset and register for standard transfers', async function () {
    // Fetch authorities
    const validators = await context.polkadotApi.query.session.validators();
    const authorities = validators.toJSON() as string[];

    const relayer1Info = TEST_RELAYERS.find(r => authorities.some(a => a.toLowerCase() === r.public.toLowerCase()));
    if (!relayer1Info) {
      activeRelayer1 = TEST_RELAYERS[0];
    } else {
      activeRelayer1 = relayer1Info;
    }
    activeRelayer1Key = keyring.addFromUri(activeRelayer1.private);

    // Deploy test asset contract
    testAssetAddress = await deployTestContract(context);

    // Register asset with STANDARD_ASSET_INDEX
    // Transfer amount in message is 0x8f0d180 = 150000000 (150M base units)
    // Set cap lower than transfer amount to force Standard transfer
    const lowCap = '100000000'; // 100M base units - less than transfer amount
    await addAssetViaSudo(
      context, sudo, testAssetAddress, TEST_ORACLE_1, lowCap,
      [STANDARD_ASSET_INDEX]
    );
  });

  it('should successfully approve standard transfer on first vote (single validator)', async function () {
    const sig = signOnFlightPoll(
      STANDARD_OUTBOUND_MESSAGE, standardMsgHash, STANDARD_SRC_TX_ID_1, activeRelayer1.private
    );

    const submission = {
      authorityId: activeRelayer1Key.address,
      msg: STANDARD_OUTBOUND_MESSAGE,
      msgHash: standardMsgHash,
      srcTxId: STANDARD_SRC_TX_ID_1,
    };

    await context.polkadotApi.tx.cccpRelayQueue.onFlightPoll(submission, sig).send();
    await context.createBlock();

    // Verify moved to OnFlightTransfers (single validator = immediate approval)
    const rawOnFlight: any = await context.polkadotApi.query.cccpRelayQueue.onFlightTransfers(standardMsgHash);
    const onFlight = rawOnFlight.toJSON();
    expect(onFlight).is.ok;
    expect(onFlight.srcTxId.toLowerCase()).eq(STANDARD_SRC_TX_ID_1.toLowerCase());
    expect(onFlight.onFlightVoters.length).gte(1);

    // Verify transfer option is Standard (enum returns string "Standard")
    expect(onFlight.option).eq('Standard');
  });

  it('should verify standard transfer does not lock on-flight cap', async function () {
    // Get asset cap after standard transfer
    const rawAssetCap: any = await context.polkadotApi.query.cccpRelayQueue.assetCaps(testAssetAddress);
    const assetCap = rawAssetCap.toJSON();
    expect(assetCap).is.ok;

    // For Standard transfers, onFlightCap should remain 0 (no cap locking)
    // Standard transfers don't affect the on-flight cap
    expect(BigInt(assetCap.onFlightCap)).eq(BigInt(0));
  });

  it('should verify standard transfer data integrity', async function () {
    // Read the on-flight transfer and verify all fields match the socket message
    const rawOnFlight: any = await context.polkadotApi.query.cccpRelayQueue.onFlightTransfers(standardMsgHash);
    const onFlight = rawOnFlight.toJSON();
    expect(onFlight).is.ok;

    // Verify key fields from socket message
    // src_chain_id = 0x0000bfc0 = 49088 (Bifrost)
    expect(onFlight.srcChainId).eq(49088);
    // dst_chain_id = 0x00aa36a7 = 11155111 (Sepolia)
    expect(onFlight.dstChainId).eq(11155111);
    // nonce = 0xca = 202 (stored as sequence_id in the transfer info)
    expect(BigInt(onFlight.sequenceId)).eq(BigInt(202));
    // asset_index_hash
    expect(onFlight.assetIndexHash.toLowerCase()).eq(STANDARD_ASSET_INDEX.toLowerCase());
    // amount = 0x8f0d180 = 150000000
    expect(BigInt(onFlight.amount)).eq(BigInt(150000000));
  });

  it('should handle multiple standard transfers with different nonces', async function () {
    // Create a new standard transfer with different nonce to create unique message
    const newNonce = '0xcd'; // Different from 0xca in original message
    const modifiedMessage = STANDARD_OUTBOUND_MESSAGE.replace(
      '00000000000000000000000000000000000000000000000000000000000000ca',
      '00000000000000000000000000000000000000000000000000000000000000' + newNonce.substring(2)
    );
    const modifiedMsgHash = computeMsgHash(modifiedMessage);

    const sig = signOnFlightPoll(
      modifiedMessage, modifiedMsgHash, STANDARD_SRC_TX_ID_2, activeRelayer1.private
    );

    const submission = {
      authorityId: activeRelayer1Key.address,
      msg: modifiedMessage,
      msgHash: modifiedMsgHash,
      srcTxId: STANDARD_SRC_TX_ID_2,
    };

    await context.polkadotApi.tx.cccpRelayQueue.onFlightPoll(submission, sig).send();
    await context.createBlock();

    // Verify second transfer is also approved
    const rawOnFlight: any = await context.polkadotApi.query.cccpRelayQueue.onFlightTransfers(modifiedMsgHash);
    const onFlight = rawOnFlight.toJSON();
    expect(onFlight).is.ok;
    expect(onFlight.srcTxId.toLowerCase()).eq(STANDARD_SRC_TX_ID_2.toLowerCase());
    expect(onFlight.option).eq('Standard'); // Standard transfer
  });

  it('should fail to create duplicate standard transfer with same msg_hash', async function () {
    // Verify first transfer is still on-flight
    const rawOnFlight: any = await context.polkadotApi.query.cccpRelayQueue.onFlightTransfers(standardMsgHash);
    if (!rawOnFlight.toJSON()) {
      this.skip(); // Transfer already finalized, skip this test
      return;
    }

    // Try to submit same message with different src_tx_id (should fail - TransferAlreadyOnFlight)
    const differentSrcTxId = '0x9999999999999999999999999999999999999999999999999999999999999999';
    const sig = signOnFlightPoll(
      STANDARD_OUTBOUND_MESSAGE, standardMsgHash, differentSrcTxId, activeRelayer1.private
    );

    await context.polkadotApi.tx.cccpRelayQueue.onFlightPoll({
      authorityId: activeRelayer1Key.address,
      msg: STANDARD_OUTBOUND_MESSAGE,
      msgHash: standardMsgHash,
      srcTxId: differentSrcTxId,
    }, sig).send();
    await context.createBlock();

    const extrinsicResult = await getExtrinsicResult(context, 'cccpRelayQueue', 'onFlightPoll');
    expect(extrinsicResult).eq('TransferAlreadyOnFlight');
  });

  it('should prevent double voting on standard transfer', async function () {
    // Create new transfer for this test
    const newNonce = '0xce';
    const newMessage = STANDARD_OUTBOUND_MESSAGE.replace(
      '00000000000000000000000000000000000000000000000000000000000000ca',
      '00000000000000000000000000000000000000000000000000000000000000' + newNonce.substring(2)
    );
    const newMsgHash = computeMsgHash(newMessage);
    const newSrcTxId = '0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';

    const sig1 = signOnFlightPoll(newMessage, newMsgHash, newSrcTxId, activeRelayer1.private);

    await context.polkadotApi.tx.cccpRelayQueue.onFlightPoll({
      authorityId: activeRelayer1Key.address,
      msg: newMessage,
      msgHash: newMsgHash,
      srcTxId: newSrcTxId,
    }, sig1).send();
    await context.createBlock();

    // Try to vote again with same relayer (should be rejected by transaction pool)
    const sig2 = signOnFlightPoll(newMessage, newMsgHash, newSrcTxId, activeRelayer1.private);

    let errorMsg = '';
    await context.polkadotApi.tx.cccpRelayQueue.onFlightPoll({
      authorityId: activeRelayer1Key.address,
      msg: newMessage,
      msgHash: newMsgHash,
      srcTxId: newSrcTxId,
    }, sig2).send().catch((err: Error) => {
      errorMsg = err.message;
    });

    // Expect transaction pool rejection (error 1013: Transaction Already Imported)
    expect(errorMsg).contains('1013');
  });
});
