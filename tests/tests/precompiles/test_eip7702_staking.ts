import { expect } from 'chai';
import { ethers } from 'ethers';

import { Keyring } from '@polkadot/api';

import { TEST_CONTROLLERS } from '../../constants/keys';
import { STAKING_ADDRESS } from '../../constants/staking_contract';

import { customWeb3Request } from '../providers';
import { describeDevNode } from '../set_dev_node';
import { createTransaction } from '../transactions';

const alith = TEST_CONTROLLERS[0];
const baltathar = TEST_CONTROLLERS[1];

// Delegate contract that forwards schedule_candidate_bond_less(uint256) calls
// to the BFC staking precompile (0x0400).
//
// Runtime (43 bytes):
//   - Builds calldata in memory:
//       mem[0..3]  = 0x034c47bc  (schedule_candidate_bond_less selector)
//       mem[4..35] = calldataload(4)  (the `less` arg passed by the caller)
//   - CALLs 0x0400 with that 36-byte calldata
//   - Propagates the revert: if CALL returned 0 (failure) → REVERT, else → STOP
//
// Constructor (12 bytes): CODECOPY runtime into memory, then RETURN it.
const DELEGATE_BYTECODE =
  '0x' +
  // constructor (12 bytes = 0x0c)
  '602b' + // PUSH1 0x2b  (runtime size = 43)
  '600c' + // PUSH1 0x0c  (runtime offset in deployment code = 12)
  '6000' + // PUSH1 0x00  (memory dst)
  '39' +   // CODECOPY
  '602b' + // PUSH1 0x2b  (return size)
  '6000' + // PUSH1 0x00
  'f3' +   // RETURN
  // runtime (43 bytes = 0x2b): offsets are relative to runtime start (byte 0)
  '63034c47bc' + // [0]  PUSH4 0x034c47bc
  '60e0' +       // [5]  PUSH1 0xe0 (224)
  '1b' +         // [7]  SHL  → top 4 bytes = selector, rest = 0
  '6000' +       // [8]  PUSH1 0x00
  '52' +         // [10] MSTORE  → mem[0..3]=selector
  '6004' +       // [11] PUSH1 0x04
  '35' +         // [13] CALLDATALOAD (32 bytes from calldata[4..36])
  '6004' +       // [14] PUSH1 0x04
  '52' +         // [16] MSTORE  → mem[4..35]=less
  '6000' +       // [17] retSize  = 0
  '6000' +       // [19] retOffset = 0
  '6024' +       // [21] argsSize  = 36
  '6000' +       // [23] argsOffset = 0
  '6000' +       // [25] value     = 0
  '610400' +     // [27] PUSH2 0x0400 (precompile)
  '5a' +         // [30] GAS
  'f1' +         // [31] CALL  → stack: [success]
  // Propagate revert:
  '15' +         // [32] ISZERO → stack: [!success]
  '6025' +       // [33] PUSH1 0x25 (= 37, offset of JUMPDEST below)
  '57' +         // [35] JUMPI  → jump to [37] if call failed
  '00' +         // [36] STOP   (success path)
  '5b' +         // [37] JUMPDEST
  '6000' +       // [38] PUSH1 0x00
  '6000' +       // [40] PUSH1 0x00
  'fd';          // [42] REVERT (failure path)

// Forwarder contract: reads a 32-byte ABI-encoded address from calldata[0..32],
// sub-calls it (value=0, no calldata, all remaining gas), and propagates any revert.
//
// Runtime (26 bytes = 0x1a):
//   [0]  PUSH1 0        retSize = 0
//   [2]  PUSH1 0        retOffset = 0
//   [4]  PUSH1 0        argsSize = 0
//   [6]  PUSH1 0        argsOffset = 0
//   [8]  PUSH1 0        value = 0
//   [10] PUSH1 0        calldataOffset for CALLDATALOAD
//   [12] CALLDATALOAD   addr = calldata[0..32]
//   [13] GAS
//   [14] CALL           → stack: [success]
//   [15] ISZERO         → stack: [1 if failed]
//   [16] PUSH1 0x14     → offset of JUMPDEST (= 20)
//   [18] JUMPI          → jump to 20 if CALL failed
//   [19] STOP           (success path)
//   [20] JUMPDEST
//   [21] PUSH1 0
//   [23] PUSH1 0
//   [25] REVERT         (failure path)
//
// Constructor (12 bytes): CODECOPY runtime, RETURN it.
const FORWARDER_BYTECODE =
  '0x' +
  // constructor (12 bytes = 0x0c)
  '601a' + // PUSH1 0x1a  (runtime size = 26)
  '600c' + // PUSH1 0x0c  (runtime offset in deployment code)
  '6000' + // PUSH1 0x00
  '39' +   // CODECOPY
  '601a' + // PUSH1 0x1a  (return size)
  '6000' + // PUSH1 0x00
  'f3' +   // RETURN
  // runtime (26 bytes = 0x1a)
  '6000' + // PUSH1 0 (retSize)
  '6000' + // PUSH1 0 (retOffset)
  '6000' + // PUSH1 0 (argsSize)
  '6000' + // PUSH1 0 (argsOffset)
  '6000' + // PUSH1 0 (value)
  '6000' + // PUSH1 0 (calldataOffset)
  '35' +   // CALLDATALOAD
  '5a' +   // GAS
  'f1' +   // CALL
  '15' +   // ISZERO
  '6014' + // PUSH1 0x14 (offset of JUMPDEST = 20)
  '57' +   // JUMPI
  '00' +   // STOP
  '5b' +   // JUMPDEST
  '6000' + // PUSH1 0
  '6000' + // PUSH1 0
  'fd';    // REVERT

// No-op contract: does nothing and succeeds (single STOP opcode).
// Used to verify that a plain deployed contract is reachable when unblocked
// and completely unreachable when its address is blocked.
//
// Runtime (1 byte): 00 STOP
// Constructor (12 bytes = 0x0c): CODECOPY runtime, RETURN it.
const NOOP_BYTECODE =
  '0x' +
  // constructor (12 bytes)
  '6001' + // PUSH1 0x01  (runtime size = 1)
  '600c' + // PUSH1 0x0c  (runtime offset in deployment code)
  '6000' + // PUSH1 0x00
  '39' +   // CODECOPY
  '6001' + // PUSH1 0x01  (return size)
  '6000' + // PUSH1 0x00
  'f3' +   // RETURN
  // runtime (1 byte)
  '00';    // STOP

// Minimal ERC20-like contract: dispatches on the 4-byte selector.
//   balanceOf(address) → 0x70a08231 → returns uint256(1000)
//   transfer(address,uint256) → 0xa9059cbb → returns bool(true)
//   anything else → REVERT
//
// Stack invariant: DUP1 at [6] leaves one extra copy of the selector below the
// comparison results.  For balanceOf the copy survives the JUMPI into the handler
// (harmless — RETURN ignores remaining stack).  For transfer the copy is consumed
// by the second EQ before the JUMPI, so the handler starts with an empty stack.
// Both handlers therefore omit POP and write their return value directly.
//
// Runtime layout (53 bytes = 0x35):
//   [0]  PUSH1 0 / CALLDATALOAD  → 32-byte word at calldata[0]
//   [3]  PUSH1 0xe0 / SHR        → top 4 bytes = selector
//   [6]  DUP1
//   [7]  PUSH4 0x70a08231 / EQ   → is balanceOf?
//   [13] PUSH1 0x1e / JUMPI      → jump to [30] if yes; stack: [selector]
//   [16] PUSH4 0xa9059cbb / EQ   → consumes remaining copy; is transfer?
//   [22] PUSH1 0x2a / JUMPI      → jump to [42] if yes; stack: []
//   [25] PUSH1 0 / PUSH1 0 / REVERT
//   [30] JUMPDEST (balanceOf)    stack: [selector] (ignored by RETURN)
//        PUSH2 1000 / MSTORE → return 1000
//   [42] JUMPDEST (transfer)     stack: [] (selector consumed by EQ above)
//        PUSH1 1 / MSTORE → return true
//
// Constructor (12 bytes = 0x0c): CODECOPY runtime, RETURN it.
const SIMPLE_ERC20_BYTECODE =
  '0x' +
  // constructor (12 bytes)
  '6035' + // PUSH1 0x35  (runtime size = 53)
  '600c' + // PUSH1 0x0c  (runtime offset)
  '6000' + // PUSH1 0x00
  '39' +   // CODECOPY
  '6035' + // PUSH1 0x35  (return size)
  '6000' + // PUSH1 0x00
  'f3' +   // RETURN
  // runtime (53 bytes) — byte offsets below are relative to runtime start
  '6000' +       // [0]  PUSH1 0  (calldataOffset for CALLDATALOAD)
  '35' +         // [2]  CALLDATALOAD  → 32 bytes at calldata[0]
  '60e0' +       // [3]  PUSH1 0xe0 (224)
  '1c' +         // [5]  SHR           → selector in top stack slot
  '80' +         // [6]  DUP1           → extra copy below for the transfer path
  '6370a08231' + // [7]  PUSH4 0x70a08231 (balanceOf selector)
  '14' +         // [12] EQ
  '601e' +       // [13] PUSH1 0x1e (= 30, balanceOf JUMPDEST)
  '57' +         // [15] JUMPI          → leaves [selector] on stack
  '63a9059cbb' + // [16] PUSH4 0xa9059cbb (transfer selector)
  '14' +         // [21] EQ             → consumes DUP1 copy; leaves [1] or [0]
  '602a' +       // [22] PUSH1 0x2a (= 42, transfer JUMPDEST)
  '57' +         // [24] JUMPI          → leaves [] on stack
  '6000' +       // [25] PUSH1 0  (revert offset)
  '6000' +       // [27] PUSH1 0  (revert size)
  'fd' +         // [29] REVERT
  '5b' +         // [30] JUMPDEST (balanceOf) — stack: [selector], ignored by RETURN
  '6103e8' +     // [31] PUSH2 1000  (0x03e8)
  '6000' +       // [34] PUSH1 0
  '52' +         // [36] MSTORE
  '6020' +       // [37] PUSH1 32
  '6000' +       // [39] PUSH1 0
  'f3' +         // [41] RETURN
  '5b' +         // [42] JUMPDEST (transfer) — stack: [], no selector to pop
  '6001' +       // [43] PUSH1 1  (true)
  '6000' +       // [45] PUSH1 0
  '52' +         // [47] MSTORE
  '6020' +       // [48] PUSH1 32
  '6000' +       // [50] PUSH1 0
  'f3';          // [52] RETURN

// ABI-encode a balanceOf(address) call.
const balanceOfCalldata = (addr: string) =>
  '0x70a08231' + addr.slice(2).toLowerCase().padStart(64, '0');

// ABI-encode a transfer(address,uint256) call.
const transferCalldata = (to: string, amount: bigint) =>
  '0xa9059cbb' +
  to.slice(2).toLowerCase().padStart(64, '0') +
  amount.toString(16).padStart(64, '0');

// Calldata sent to Alith's delegated address:
//   bytes [0..3]  : any 4-byte selector (delegate ignores it, reads less from offset 4)
//   bytes [4..35] : abi-encoded uint256 `less = 1`
const CALL_DATA =
  '0x' +
  '00000000' +                                                               // selector (ignored)
  '0000000000000000000000000000000000000000000000000000000000000001'; // less = 1

describeDevNode('EIP-7702 EOA guard on schedule_candidate_bond_less', (context) => {
  let delegateAddress: string;
  let chainId: bigint;

  before('deploy delegate contract and record chain id', async () => {
    chainId = await context.web3.eth.getChainId();

    // Deploy the delegate contract using the same Legacy signing path all other tests use.
    const rawDeploy = await createTransaction(context, {
      from: alith.public,
      privateKey: alith.private,
      data: DELEGATE_BYTECODE,
      gas: 200_000,
      gasPrice: 1_000_000_000_000,
      value: '0x0',
    });
    const { txResults } = await context.createBlock({ transactions: [rawDeploy] });

    const deployHash = txResults[0] as string;
    if (!deployHash?.startsWith('0x')) {
      throw new Error(`deploy rejected by pool: ${JSON.stringify(txResults[0])}`);
    }

    const receipt = await context.web3.eth.getTransactionReceipt(deployHash);
    expect(receipt.status).to.equal(1n, 'deploy tx status');
    delegateAddress = receipt.contractAddress!;
    expect(delegateAddress).to.be.a('string');
  });

  it('should revert when a 7702-delegated EOA calls schedule_candidate_bond_less', async () => {
    const alithWallet = new ethers.Wallet(alith.private);
    const baltatharWallet = new ethers.Wallet(baltathar.private);

    const alithNonce = Number(
      await context.web3.eth.getTransactionCount(alith.public, 'pending'),
    );
    const baltatharNonce = Number(
      await context.web3.eth.getTransactionCount(baltathar.public, 'pending'),
    );

    // Alith authorizes her EOA to delegate execution to the deployed contract.
    // After this authorization is applied on-chain, Alith's account code becomes
    // 0xef0100 || delegateAddress (23 bytes), making account_code_metadata().size > 0.
    const auth = await alithWallet.authorizeSync({
      address: delegateAddress,
      chainId,
      nonce: alithNonce,
    });

    // Baltathar sends the EIP-7702 type-4 transaction:
    //   1. Alith's authorization is applied  (sets Alith's code to the delegation prefix)
    //   2. `to: alith.public` is then called with CALL_DATA
    //      → the delegate code runs in Alith's context
    //      → delegate calls schedule_candidate_bond_less on the precompile
    //      → ensure_caller_is_eoa sees Alith (the msg.sender) has code → reverts
    const rawTx = await baltatharWallet.signTransaction({
      type: 4,
      chainId,
      nonce: baltatharNonce,
      to: alith.public,
      data: CALL_DATA,
      gasLimit: 300_000n,
      maxFeePerGas: 1_000_000_000_000n,
      maxPriorityFeePerGas: 0n,
      value: 0n,
      authorizationList: [auth],
    });

    const { txResults } = await context.createBlock({ transactions: [rawTx] });
    const txHash = txResults[0] as string;
    expect(txHash, 'type-4 tx should be accepted into the pool').to.match(/^0x/);

    // Verify Alith's code was set to the 23-byte EIP-7702 delegation prefix (0xef0100 || addr).
    const alithCode = await context.web3.eth.getCode(alith.public);
    expect(alithCode, 'Alith should have delegation code set').to.not.equal('0x');

    const receipt = await context.web3.eth.getTransactionReceipt(txHash);
    expect(receipt.status).to.equal(0n, 'tx should revert (EOA guard triggered)');
  });

  it('should succeed after the 7702 delegation is removed (plain EOA direct call)', async () => {
    const alithWallet = new ethers.Wallet(alith.private);
    const baltatharWallet = new ethers.Wallet(baltathar.private);

    // EIP-7702 increments the authorizing account's nonce when the authorization is applied,
    // so Alith's nonce is now one higher than it was when we sent the first authorization.
    const alithNonce = Number(
      await context.web3.eth.getTransactionCount(alith.public, 'pending'),
    );
    const baltatharNonce = Number(
      await context.web3.eth.getTransactionCount(baltathar.public, 'pending'),
    );

    // Setting the authorization address to the zero address clears the account's code,
    // restoring Alith to a plain EOA.
    const clearAuth = await alithWallet.authorizeSync({
      address: '0x0000000000000000000000000000000000000000',
      chainId,
      nonce: alithNonce,
    });

    // Baltathar sends the clearing tx (Alith's authorization nonce != Baltathar's tx nonce,
    // so there is no nonce conflict).
    const rawClear = await baltatharWallet.signTransaction({
      type: 4,
      chainId,
      nonce: baltatharNonce,
      to: baltathar.public, // no-op call; we only need the authorization to be applied
      data: '0x',
      gasLimit: 100_000n,
      maxFeePerGas: 1_000_000_000_000n,
      maxPriorityFeePerGas: 0n,
      value: 0n,
      authorizationList: [clearAuth],
    });

    const { txResults: clearResults } = await context.createBlock({ transactions: [rawClear] });
    expect(clearResults[0] as string, 'clear tx should be accepted').to.match(/^0x/);

    // Verify Alith is a plain EOA again
    const alithCode = await context.web3.eth.getCode(alith.public);
    expect(alithCode).to.equal('0x', 'Alith should have no code after clearing the delegation');

    // Alith calls schedule_candidate_bond_less(1) directly as a plain EOA.
    // CandidateInfo is keyed by the controller (Alith), so this is a valid candidate call.
    const rawBondLess = await createTransaction(context, {
      from: alith.public,
      privateKey: alith.private,
      to: STAKING_ADDRESS,
      // schedule_candidate_bond_less(uint256) selector 0x034c47bc, less = 1
      data: '0x034c47bc' + '0000000000000000000000000000000000000000000000000000000000000001',
      gas: 200_000,
      gasPrice: 1_000_000_000_000,
      value: '0x0',
    });

    const { txResults } = await context.createBlock({ transactions: [rawBondLess] });
    const txHash = txResults[0] as string;
    expect(txHash, 'bond less tx should be accepted').to.match(/^0x/);

    const receipt = await context.web3.eth.getTransactionReceipt(txHash);
    expect(receipt.status).to.equal(1n, 'schedule_candidate_bond_less should succeed for plain EOA');
  });
});

// Tests the precompile-guard fix in precompiles.rs.
// The scenario: an EOA gets EIP-7702 delegation code, then gets blocked.
// After that, any EVM call targeting that blocked account should be reverted
// on-chain by the BifrostPrecompiles guard — covering both top-level txs and
// internal sub-calls (EOA → contract → blocked_account).
describeDevNode('EIP-7702 blocked account on-chain revert', (context) => {
  const alith = TEST_CONTROLLERS[0];
  const baltathar = TEST_CONTROLLERS[1];
  const charleth = TEST_CONTROLLERS[2];

  let delegateAddress: string;
  let chainId: bigint;

  before('deploy delegate contract, delegate Charleth, and block Charleth', async () => {
    chainId = await context.web3.eth.getChainId();

    const rawDeploy = await createTransaction(context, {
      from: alith.public,
      privateKey: alith.private,
      data: DELEGATE_BYTECODE,
      gas: 200_000,
      gasPrice: 1_000_000_000_000,
      value: '0x0',
    });
    const { txResults } = await context.createBlock({ transactions: [rawDeploy] });
    const deployHash = txResults[0] as string;
    if (!deployHash?.startsWith('0x')) {
      throw new Error(`deploy rejected: ${JSON.stringify(txResults[0])}`);
    }
    const receipt = await context.web3.eth.getTransactionReceipt(deployHash);
    expect(receipt.status).to.equal(1n, 'deploy tx status');
    delegateAddress = receipt.contractAddress!;

    // Apply EIP-7702 delegation to Charleth while she is NOT yet blocked.
    const charlethWallet = new ethers.Wallet(charleth.private);
    const baltatharWallet = new ethers.Wallet(baltathar.private);

    const charlethNonce = Number(
      await context.web3.eth.getTransactionCount(charleth.public, 'pending'),
    );
    const baltatharNonce = Number(
      await context.web3.eth.getTransactionCount(baltathar.public, 'pending'),
    );

    const auth = await charlethWallet.authorizeSync({
      address: delegateAddress,
      chainId,
      nonce: charlethNonce,
    });

    const rawSetup = await baltatharWallet.signTransaction({
      type: 4,
      chainId,
      nonce: baltatharNonce,
      to: baltathar.public,
      data: '0x',
      gasLimit: 100_000n,
      maxFeePerGas: 1_000_000_000_000n,
      maxPriorityFeePerGas: 0n,
      value: 0n,
      authorizationList: [auth],
    });

    const { txResults: setupResults } = await context.createBlock({ transactions: [rawSetup] });
    expect(setupResults[0] as string, 'delegation setup tx should be accepted').to.match(/^0x/);

    const charlethCode = await context.web3.eth.getCode(charleth.public);
    expect(charlethCode, 'Charleth should have delegation code after authorization').to.not.equal('0x');

    // Block Charleth via sudo (Alith is the sudo key in dev mode).
    const keyring = new Keyring({ type: 'ethereum' });
    const sudoKey = keyring.addFromUri(alith.private);
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.bfcUtility.addBlockedAccount(charleth.public)
    ).signAndSend(sudoKey);
    await context.createBlock();
  });

  it('should revert on-chain when calling a blocked+delegated account', async () => {
    // The precompile guard intercepts the CALL to Charleth's address because
    // is_blocked_account returns true → is_precompile returns Answer{true} →
    // execute returns Err(Revert "blocked account").  The pool accepts the tx
    // (no longer rejected at check_self_contained level), but the EVM reverts it.
    const charlethCode = await context.web3.eth.getCode(charleth.public);
    expect(charlethCode, 'Charleth should still have delegation code').to.not.equal('0x');

    const baltatharWallet = new ethers.Wallet(baltathar.private);
    const baltatharNonce = Number(
      await context.web3.eth.getTransactionCount(baltathar.public, 'pending'),
    );
    const rawCallTx = await baltatharWallet.signTransaction({
      type: 2,
      chainId,
      nonce: baltatharNonce,
      to: charleth.public,
      data: '0x',
      gasLimit: 50_000n,
      maxFeePerGas: 1_000_000_000_000n,
      maxPriorityFeePerGas: 0n,
      value: 0n,
    });

    const { txResults } = await context.createBlock({ transactions: [rawCallTx] });
    const txHash = txResults[0] as string;
    expect(txHash, 'tx targeting blocked account should be accepted by the pool').to.match(/^0x/);

    const receipt = await context.web3.eth.getTransactionReceipt(txHash);
    expect(receipt.status).to.equal(0n, 'tx should revert on-chain (precompile guard)');
  });

  it('should succeed after the account is unblocked and delegation cleared', async () => {
    const charlethWallet = new ethers.Wallet(charleth.private);
    const baltatharWallet = new ethers.Wallet(baltathar.private);
    const keyring = new Keyring({ type: 'ethereum' });
    const sudoKey = keyring.addFromUri(alith.private);

    // Unblock Charleth so the precompile guard no longer fires.
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.bfcUtility.removeBlockedAccount(charleth.public)
    ).signAndSend(sudoKey);
    await context.createBlock();

    // Clear the EIP-7702 delegation so the subsequent call is a plain ETH transfer
    // (not routed through the delegate bytecode which would fail for staking reasons).
    const charlethNonce = Number(
      await context.web3.eth.getTransactionCount(charleth.public, 'pending'),
    );
    let baltatharNonce = Number(
      await context.web3.eth.getTransactionCount(baltathar.public, 'pending'),
    );

    const clearAuth = await charlethWallet.authorizeSync({
      address: '0x0000000000000000000000000000000000000000',
      chainId,
      nonce: charlethNonce,
    });

    const rawClear = await baltatharWallet.signTransaction({
      type: 4,
      chainId,
      nonce: baltatharNonce,
      to: baltathar.public,
      data: '0x',
      gasLimit: 100_000n,
      maxFeePerGas: 1_000_000_000_000n,
      maxPriorityFeePerGas: 0n,
      value: 0n,
      authorizationList: [clearAuth],
    });

    const { txResults: clearResults } = await context.createBlock({ transactions: [rawClear] });
    expect(clearResults[0] as string, 'clear tx should be accepted').to.match(/^0x/);

    const charlethCode = await context.web3.eth.getCode(charleth.public);
    expect(charlethCode).to.equal('0x', 'Charleth should have no code after clearing delegation');

    // A plain type-2 tx to Charleth (now an unblocked plain EOA) should succeed.
    baltatharNonce = Number(
      await context.web3.eth.getTransactionCount(baltathar.public, 'pending'),
    );
    const rawCallTx = await baltatharWallet.signTransaction({
      type: 2,
      chainId,
      nonce: baltatharNonce,
      to: charleth.public,
      data: '0x',
      gasLimit: 50_000n,
      maxFeePerGas: 1_000_000_000_000n,
      maxPriorityFeePerGas: 0n,
      value: 0n,
    });

    const { txResults } = await context.createBlock({ transactions: [rawCallTx] });
    const txHash = txResults[0] as string;
    expect(txHash, 'tx to unblocked plain EOA should be accepted').to.match(/^0x/);

    const receipt = await context.web3.eth.getTransactionReceipt(txHash);
    expect(receipt.status).to.equal(1n, 'tx to unblocked plain EOA should succeed');
  });
});

// Tests that the precompile guard fires for INTERNAL sub-calls, not just top-level txs.
// Scenario: EOA → forwarder contract → blocked_account.
// The forwarder makes a sub-call to the blocked account; the guard reverts it;
// the forwarder propagates the revert; the outer tx reverts.
describeDevNode('EIP-7702 blocked account sub-call revert', (context) => {
  const alith = TEST_CONTROLLERS[0];
  const baltathar = TEST_CONTROLLERS[1];
  const charleth = TEST_CONTROLLERS[2];

  let forwarderAddress: string;
  let delegateAddress: string;
  let chainId: bigint;

  before('deploy forwarder + delegate, delegate Charleth, block Charleth', async () => {
    chainId = await context.web3.eth.getChainId();

    // Deploy the forwarder contract (calls a target address read from calldata).
    const rawForwarderDeploy = await createTransaction(context, {
      from: alith.public,
      privateKey: alith.private,
      data: FORWARDER_BYTECODE,
      gas: 200_000,
      gasPrice: 1_000_000_000_000,
      value: '0x0',
    });
    const { txResults: fwdResults } = await context.createBlock({ transactions: [rawForwarderDeploy] });
    const fwdHash = fwdResults[0] as string;
    if (!fwdHash?.startsWith('0x')) throw new Error(`forwarder deploy rejected: ${fwdResults[0]}`);
    const fwdReceipt = await context.web3.eth.getTransactionReceipt(fwdHash);
    expect(fwdReceipt.status).to.equal(1n, 'forwarder deploy status');
    forwarderAddress = fwdReceipt.contractAddress!;

    // Deploy the delegate contract (same one used in the other suites).
    const rawDelegateDeploy = await createTransaction(context, {
      from: alith.public,
      privateKey: alith.private,
      data: DELEGATE_BYTECODE,
      gas: 200_000,
      gasPrice: 1_000_000_000_000,
      value: '0x0',
    });
    const { txResults: delResults } = await context.createBlock({ transactions: [rawDelegateDeploy] });
    const delHash = delResults[0] as string;
    if (!delHash?.startsWith('0x')) throw new Error(`delegate deploy rejected: ${delResults[0]}`);
    const delReceipt = await context.web3.eth.getTransactionReceipt(delHash);
    expect(delReceipt.status).to.equal(1n, 'delegate deploy status');
    delegateAddress = delReceipt.contractAddress!;

    // Apply EIP-7702 delegation to Charleth.
    const charlethWallet = new ethers.Wallet(charleth.private);
    const baltatharWallet = new ethers.Wallet(baltathar.private);

    const charlethNonce = Number(
      await context.web3.eth.getTransactionCount(charleth.public, 'pending'),
    );
    const baltatharNonce = Number(
      await context.web3.eth.getTransactionCount(baltathar.public, 'pending'),
    );

    const auth = await charlethWallet.authorizeSync({
      address: delegateAddress,
      chainId,
      nonce: charlethNonce,
    });
    const rawSetup = await baltatharWallet.signTransaction({
      type: 4,
      chainId,
      nonce: baltatharNonce,
      to: baltathar.public,
      data: '0x',
      gasLimit: 100_000n,
      maxFeePerGas: 1_000_000_000_000n,
      maxPriorityFeePerGas: 0n,
      value: 0n,
      authorizationList: [auth],
    });
    const { txResults: setupResults } = await context.createBlock({ transactions: [rawSetup] });
    expect(setupResults[0] as string, 'delegation setup should be accepted').to.match(/^0x/);

    const charlethCode = await context.web3.eth.getCode(charleth.public);
    expect(charlethCode, 'Charleth should have delegation code').to.not.equal('0x');

    // Block Charleth.
    const keyring = new Keyring({ type: 'ethereum' });
    const sudoKey = keyring.addFromUri(alith.private);
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.bfcUtility.addBlockedAccount(charleth.public)
    ).signAndSend(sudoKey);
    await context.createBlock();
  });

  it('should revert the outer tx when a sub-call targets a blocked account', async () => {
    // Baltathar calls the forwarder, which sub-calls Charleth.
    // The precompile guard fires on the internal CALL opcode (code_address = Charleth)
    // and reverts the sub-call; the forwarder propagates the revert.
    const baltatharWallet = new ethers.Wallet(baltathar.private);
    const baltatharNonce = Number(
      await context.web3.eth.getTransactionCount(baltathar.public, 'pending'),
    );

    // Pass Charleth's address ABI-encoded (left-padded to 32 bytes).
    const calldata = ethers.zeroPadValue(charleth.public, 32);

    const rawTx = await baltatharWallet.signTransaction({
      type: 2,
      chainId,
      nonce: baltatharNonce,
      to: forwarderAddress,
      data: calldata,
      gasLimit: 200_000n,
      maxFeePerGas: 1_000_000_000_000n,
      maxPriorityFeePerGas: 0n,
      value: 0n,
    });

    const { txResults } = await context.createBlock({ transactions: [rawTx] });
    const txHash = txResults[0] as string;
    expect(txHash, 'tx should be accepted by the pool').to.match(/^0x/);

    const receipt = await context.web3.eth.getTransactionReceipt(txHash);
    expect(receipt.status).to.equal(0n, 'outer tx should revert (sub-call to blocked account reverted)');
  });

  it('should succeed when the sub-call target is unblocked and has no code', async () => {
    const charlethWallet = new ethers.Wallet(charleth.private);
    const baltatharWallet = new ethers.Wallet(baltathar.private);
    const keyring = new Keyring({ type: 'ethereum' });
    const sudoKey = keyring.addFromUri(alith.private);

    // Unblock Charleth.
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.bfcUtility.removeBlockedAccount(charleth.public)
    ).signAndSend(sudoKey);
    await context.createBlock();

    // Clear Charleth's delegation so the forwarder sub-call becomes a plain ETH transfer
    // (which succeeds) rather than invoking delegation code (which would fail).
    const charlethNonce = Number(
      await context.web3.eth.getTransactionCount(charleth.public, 'pending'),
    );
    let baltatharNonce = Number(
      await context.web3.eth.getTransactionCount(baltathar.public, 'pending'),
    );

    const clearAuth = await charlethWallet.authorizeSync({
      address: '0x0000000000000000000000000000000000000000',
      chainId,
      nonce: charlethNonce,
    });
    const rawClear = await baltatharWallet.signTransaction({
      type: 4,
      chainId,
      nonce: baltatharNonce,
      to: baltathar.public,
      data: '0x',
      gasLimit: 100_000n,
      maxFeePerGas: 1_000_000_000_000n,
      maxPriorityFeePerGas: 0n,
      value: 0n,
      authorizationList: [clearAuth],
    });
    const { txResults: clearResults } = await context.createBlock({ transactions: [rawClear] });
    expect(clearResults[0] as string, 'clear tx should be accepted').to.match(/^0x/);

    const charlethCode = await context.web3.eth.getCode(charleth.public);
    expect(charlethCode).to.equal('0x', 'Charleth should be a plain EOA after clearing delegation');

    // Forwarder sub-calls Charleth (now an unblocked plain EOA): no guard, no code → success.
    baltatharNonce = Number(
      await context.web3.eth.getTransactionCount(baltathar.public, 'pending'),
    );
    const calldata = ethers.zeroPadValue(charleth.public, 32);

    const rawTx = await baltatharWallet.signTransaction({
      type: 2,
      chainId,
      nonce: baltatharNonce,
      to: forwarderAddress,
      data: calldata,
      gasLimit: 200_000n,
      maxFeePerGas: 1_000_000_000_000n,
      maxPriorityFeePerGas: 0n,
      value: 0n,
    });

    const { txResults } = await context.createBlock({ transactions: [rawTx] });
    const txHash = txResults[0] as string;
    expect(txHash, 'tx should be accepted').to.match(/^0x/);

    const receipt = await context.web3.eth.getTransactionReceipt(txHash);
    expect(receipt.status).to.equal(1n, 'outer tx should succeed (sub-call to plain EOA)');
  });
});

// Tests the check_self_contained signer check in self_contained_call.rs.
// A blocked account must not be able to submit transactions — the pool must reject
// any EVM tx whose signer is a blocked account, before it is ever included in a block.
describeDevNode('blocked account signer pool rejection', (context) => {
  const alith = TEST_CONTROLLERS[0];
  const charleth = TEST_CONTROLLERS[2];

  let chainId: bigint;

  before('block Charleth', async () => {
    chainId = await context.web3.eth.getChainId();
    const keyring = new Keyring({ type: 'ethereum' });
    const sudoKey = keyring.addFromUri(alith.private);
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.bfcUtility.addBlockedAccount(charleth.public)
    ).signAndSend(sudoKey);
    await context.createBlock();
  });

  it('should reject from pool a tx signed by a blocked account', async () => {
    const charlethWallet = new ethers.Wallet(charleth.private);
    const charlethNonce = Number(
      await context.web3.eth.getTransactionCount(charleth.public, 'pending'),
    );
    const rawTx = await charlethWallet.signTransaction({
      type: 2,
      chainId,
      nonce: charlethNonce,
      to: charleth.public,
      data: '0x',
      gasLimit: 21_000n,
      maxFeePerGas: 1_000_000_000_000n,
      maxPriorityFeePerGas: 0n,
      value: 0n,
    });

    let txResult: any;
    try {
      txResult = await customWeb3Request(context.web3, 'eth_sendRawTransaction', [rawTx]);
    } catch (err: any) {
      txResult = err;
    }
    const isAccepted = typeof txResult === 'string' && /^0x[0-9a-f]{64}$/i.test(txResult);
    expect(isAccepted, 'tx signed by a blocked account should be rejected by the pool').to.be.false;
  });

  it('should accept a tx from the account once it is unblocked', async () => {
    const keyring = new Keyring({ type: 'ethereum' });
    const sudoKey = keyring.addFromUri(alith.private);
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.bfcUtility.removeBlockedAccount(charleth.public)
    ).signAndSend(sudoKey);
    await context.createBlock();

    const charlethWallet = new ethers.Wallet(charleth.private);
    const charlethNonce = Number(
      await context.web3.eth.getTransactionCount(charleth.public, 'pending'),
    );
    const rawTx = await charlethWallet.signTransaction({
      type: 2,
      chainId,
      nonce: charlethNonce,
      to: charleth.public,
      data: '0x',
      gasLimit: 21_000n,
      maxFeePerGas: 1_000_000_000_000n,
      maxPriorityFeePerGas: 0n,
      value: 0n,
    });

    let txResult: any;
    try {
      txResult = await customWeb3Request(context.web3, 'eth_sendRawTransaction', [rawTx]);
    } catch (err: any) {
      txResult = err;
    }
    const isAccepted = typeof txResult === 'string' && /^0x[0-9a-f]{64}$/i.test(txResult);
    expect(isAccepted, 'tx from unblocked account should be accepted by the pool').to.be.true;

    // Mine the block and verify on-chain execution also succeeded.
    await context.createBlock();
    const receipt = await context.web3.eth.getTransactionReceipt(txResult as string);
    expect(receipt.status).to.equal(1n, 'tx from unblocked account should succeed on-chain');
  });
});

// Tests that blocking a deployed contract address makes it completely uncallable
// via a direct top-level transaction.
describeDevNode('blocked contract address (direct call)', (context) => {
  const alith = TEST_CONTROLLERS[0];
  const baltathar = TEST_CONTROLLERS[1];

  let contractAddress: string;
  let chainId: bigint;

  before('deploy noop contract and block its address', async () => {
    chainId = await context.web3.eth.getChainId();

    const rawDeploy = await createTransaction(context, {
      from: alith.public,
      privateKey: alith.private,
      data: NOOP_BYTECODE,
      gas: 100_000,
      gasPrice: 1_000_000_000_000,
      value: '0x0',
    });
    const { txResults } = await context.createBlock({ transactions: [rawDeploy] });
    const deployHash = txResults[0] as string;
    if (!deployHash?.startsWith('0x')) throw new Error(`deploy rejected: ${txResults[0]}`);
    const deployReceipt = await context.web3.eth.getTransactionReceipt(deployHash);
    expect(deployReceipt.status).to.equal(1n, 'deploy status');
    contractAddress = deployReceipt.contractAddress!;

    // Sanity-check: contract is callable before blocking.
    const baltatharWallet = new ethers.Wallet(baltathar.private);
    let nonce = Number(await context.web3.eth.getTransactionCount(baltathar.public, 'pending'));
    const rawSanity = await baltatharWallet.signTransaction({
      type: 2, chainId, nonce,
      to: contractAddress, data: '0x',
      gasLimit: 50_000n, maxFeePerGas: 1_000_000_000_000n, maxPriorityFeePerGas: 0n, value: 0n,
    });
    const { txResults: sanityResults } = await context.createBlock({ transactions: [rawSanity] });
    const sanityReceipt = await context.web3.eth.getTransactionReceipt(sanityResults[0] as string);
    expect(sanityReceipt.status).to.equal(1n, 'contract should be callable before blocking');

    // Block the contract address.
    const keyring = new Keyring({ type: 'ethereum' });
    const sudoKey = keyring.addFromUri(alith.private);
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.bfcUtility.addBlockedAccount(contractAddress)
    ).signAndSend(sudoKey);
    await context.createBlock();
  });

  it('should revert a direct call to a blocked contract', async () => {
    const baltatharWallet = new ethers.Wallet(baltathar.private);
    const nonce = Number(await context.web3.eth.getTransactionCount(baltathar.public, 'pending'));
    const rawTx = await baltatharWallet.signTransaction({
      type: 2, chainId, nonce,
      to: contractAddress, data: '0x',
      gasLimit: 50_000n, maxFeePerGas: 1_000_000_000_000n, maxPriorityFeePerGas: 0n, value: 0n,
    });

    const { txResults } = await context.createBlock({ transactions: [rawTx] });
    const txHash = txResults[0] as string;
    expect(txHash, 'tx should be accepted by the pool').to.match(/^0x/);

    const receipt = await context.web3.eth.getTransactionReceipt(txHash);
    expect(receipt.status).to.equal(0n, 'call to blocked contract should revert');
  });

  it('should succeed after the contract is unblocked', async () => {
    const keyring = new Keyring({ type: 'ethereum' });
    const sudoKey = keyring.addFromUri(alith.private);
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.bfcUtility.removeBlockedAccount(contractAddress)
    ).signAndSend(sudoKey);
    await context.createBlock();

    const baltatharWallet = new ethers.Wallet(baltathar.private);
    const nonce = Number(await context.web3.eth.getTransactionCount(baltathar.public, 'pending'));
    const rawTx = await baltatharWallet.signTransaction({
      type: 2, chainId, nonce,
      to: contractAddress, data: '0x',
      gasLimit: 50_000n, maxFeePerGas: 1_000_000_000_000n, maxPriorityFeePerGas: 0n, value: 0n,
    });

    const { txResults } = await context.createBlock({ transactions: [rawTx] });
    const txHash = txResults[0] as string;
    expect(txHash, 'tx should be accepted').to.match(/^0x/);

    const receipt = await context.web3.eth.getTransactionReceipt(txHash);
    expect(receipt.status).to.equal(1n, 'call to unblocked contract should succeed');
  });
});

// Tests that blocking a deployed contract address makes it completely uncallable
// even via an internal sub-call (EOA → forwarder → blocked contract).
describeDevNode('blocked contract address (sub-call)', (context) => {
  const alith = TEST_CONTROLLERS[0];
  const baltathar = TEST_CONTROLLERS[1];

  let contractAddress: string;
  let forwarderAddress: string;
  let chainId: bigint;

  before('deploy noop + forwarder contracts and block noop', async () => {
    chainId = await context.web3.eth.getChainId();

    // Deploy noop contract.
    const rawNoopDeploy = await createTransaction(context, {
      from: alith.public, privateKey: alith.private,
      data: NOOP_BYTECODE, gas: 100_000, gasPrice: 1_000_000_000_000, value: '0x0',
    });
    const { txResults: noopResults } = await context.createBlock({ transactions: [rawNoopDeploy] });
    const noopHash = noopResults[0] as string;
    if (!noopHash?.startsWith('0x')) throw new Error(`noop deploy rejected: ${noopResults[0]}`);
    const noopReceipt = await context.web3.eth.getTransactionReceipt(noopHash);
    expect(noopReceipt.status).to.equal(1n, 'noop deploy status');
    contractAddress = noopReceipt.contractAddress!;

    // Deploy forwarder contract.
    const rawFwdDeploy = await createTransaction(context, {
      from: alith.public, privateKey: alith.private,
      data: FORWARDER_BYTECODE, gas: 200_000, gasPrice: 1_000_000_000_000, value: '0x0',
    });
    const { txResults: fwdResults } = await context.createBlock({ transactions: [rawFwdDeploy] });
    const fwdHash = fwdResults[0] as string;
    if (!fwdHash?.startsWith('0x')) throw new Error(`forwarder deploy rejected: ${fwdResults[0]}`);
    const fwdReceipt = await context.web3.eth.getTransactionReceipt(fwdHash);
    expect(fwdReceipt.status).to.equal(1n, 'forwarder deploy status');
    forwarderAddress = fwdReceipt.contractAddress!;

    // Block the noop contract address.
    const keyring = new Keyring({ type: 'ethereum' });
    const sudoKey = keyring.addFromUri(alith.private);
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.bfcUtility.addBlockedAccount(contractAddress)
    ).signAndSend(sudoKey);
    await context.createBlock();
  });

  it('should revert the outer tx when a sub-call targets a blocked contract', async () => {
    const baltatharWallet = new ethers.Wallet(baltathar.private);
    const nonce = Number(await context.web3.eth.getTransactionCount(baltathar.public, 'pending'));
    const calldata = ethers.zeroPadValue(contractAddress, 32);

    const rawTx = await baltatharWallet.signTransaction({
      type: 2, chainId, nonce,
      to: forwarderAddress, data: calldata,
      gasLimit: 200_000n, maxFeePerGas: 1_000_000_000_000n, maxPriorityFeePerGas: 0n, value: 0n,
    });

    const { txResults } = await context.createBlock({ transactions: [rawTx] });
    const txHash = txResults[0] as string;
    expect(txHash, 'tx should be accepted by the pool').to.match(/^0x/);

    const receipt = await context.web3.eth.getTransactionReceipt(txHash);
    expect(receipt.status).to.equal(0n, 'outer tx should revert (sub-call to blocked contract reverted)');
  });

  it('should succeed when the sub-call target is unblocked', async () => {
    const keyring = new Keyring({ type: 'ethereum' });
    const sudoKey = keyring.addFromUri(alith.private);
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.bfcUtility.removeBlockedAccount(contractAddress)
    ).signAndSend(sudoKey);
    await context.createBlock();

    const baltatharWallet = new ethers.Wallet(baltathar.private);
    const nonce = Number(await context.web3.eth.getTransactionCount(baltathar.public, 'pending'));
    const calldata = ethers.zeroPadValue(contractAddress, 32);

    const rawTx = await baltatharWallet.signTransaction({
      type: 2, chainId, nonce,
      to: forwarderAddress, data: calldata,
      gasLimit: 200_000n, maxFeePerGas: 1_000_000_000_000n, maxPriorityFeePerGas: 0n, value: 0n,
    });

    const { txResults } = await context.createBlock({ transactions: [rawTx] });
    const txHash = txResults[0] as string;
    expect(txHash, 'tx should be accepted').to.match(/^0x/);

    const receipt = await context.web3.eth.getTransactionReceipt(txHash);
    expect(receipt.status).to.equal(1n, 'outer tx should succeed (sub-call to unblocked contract)');
  });
});

// Tests that blocking an ERC20 token contract address freezes all token operations.
// Uses a minimal ERC20-like contract with real balanceOf / transfer selectors so
// the test is representative of how token contracts are actually called.
describeDevNode('blocked ERC20 token contract', (context) => {
  const alith = TEST_CONTROLLERS[0];
  const baltathar = TEST_CONTROLLERS[1];

  let tokenAddress: string;
  let chainId: bigint;

  before('deploy ERC20 token, verify it works, then block its address', async () => {
    chainId = await context.web3.eth.getChainId();

    const rawDeploy = await createTransaction(context, {
      from: alith.public,
      privateKey: alith.private,
      data: SIMPLE_ERC20_BYTECODE,
      gas: 200_000,
      gasPrice: 1_000_000_000_000,
      value: '0x0',
    });
    const { txResults } = await context.createBlock({ transactions: [rawDeploy] });
    const deployHash = txResults[0] as string;
    if (!deployHash?.startsWith('0x')) throw new Error(`deploy rejected: ${txResults[0]}`);
    const deployReceipt = await context.web3.eth.getTransactionReceipt(deployHash);
    expect(deployReceipt.status).to.equal(1n, 'deploy status');
    tokenAddress = deployReceipt.contractAddress!;

    // Sanity-check via eth_call: balanceOf should return 1000 before blocking.
    const returnData = await context.web3.eth.call({
      to: tokenAddress,
      data: balanceOfCalldata(alith.public),
    });
    expect(BigInt(returnData)).to.equal(1000n, 'balanceOf should return 1000 before blocking');

    // Block the token contract address.
    const keyring = new Keyring({ type: 'ethereum' });
    const sudoKey = keyring.addFromUri(alith.private);
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.bfcUtility.addBlockedAccount(tokenAddress)
    ).signAndSend(sudoKey);
    await context.createBlock();
  });

  it('should revert balanceOf on a blocked ERC20 contract', async () => {
    const baltatharWallet = new ethers.Wallet(baltathar.private);
    const nonce = Number(
      await context.web3.eth.getTransactionCount(baltathar.public, 'pending'),
    );
    const rawTx = await baltatharWallet.signTransaction({
      type: 2,
      chainId,
      nonce,
      to: tokenAddress,
      data: balanceOfCalldata(alith.public),
      gasLimit: 50_000n,
      maxFeePerGas: 1_000_000_000_000n,
      maxPriorityFeePerGas: 0n,
      value: 0n,
    });

    const { txResults } = await context.createBlock({ transactions: [rawTx] });
    const txHash = txResults[0] as string;
    expect(txHash, 'tx should be accepted by the pool').to.match(/^0x/);

    const receipt = await context.web3.eth.getTransactionReceipt(txHash);
    expect(receipt.status).to.equal(0n, 'balanceOf on blocked token should revert');
  });

  it('should revert transfer on a blocked ERC20 contract', async () => {
    const baltatharWallet = new ethers.Wallet(baltathar.private);
    const nonce = Number(
      await context.web3.eth.getTransactionCount(baltathar.public, 'pending'),
    );
    const rawTx = await baltatharWallet.signTransaction({
      type: 2,
      chainId,
      nonce,
      to: tokenAddress,
      data: transferCalldata(baltathar.public, 100n),
      gasLimit: 50_000n,
      maxFeePerGas: 1_000_000_000_000n,
      maxPriorityFeePerGas: 0n,
      value: 0n,
    });

    const { txResults } = await context.createBlock({ transactions: [rawTx] });
    const txHash = txResults[0] as string;
    expect(txHash, 'tx should be accepted by the pool').to.match(/^0x/);

    const receipt = await context.web3.eth.getTransactionReceipt(txHash);
    expect(receipt.status).to.equal(0n, 'transfer on blocked token should revert');
  });

  it('should restore balanceOf and transfer after unblocking', async () => {
    const keyring = new Keyring({ type: 'ethereum' });
    const sudoKey = keyring.addFromUri(alith.private);
    await context.polkadotApi.tx.sudo.sudo(
      context.polkadotApi.tx.bfcUtility.removeBlockedAccount(tokenAddress)
    ).signAndSend(sudoKey);
    await context.createBlock();

    // Verify balanceOf via eth_call returns the expected value again.
    const returnData = await context.web3.eth.call({
      to: tokenAddress,
      data: balanceOfCalldata(alith.public),
    });
    expect(BigInt(returnData)).to.equal(1000n, 'balanceOf should return 1000 after unblocking');

    // Verify transfer succeeds as a real transaction.
    const baltatharWallet = new ethers.Wallet(baltathar.private);
    const nonce = Number(
      await context.web3.eth.getTransactionCount(baltathar.public, 'pending'),
    );
    const rawTx = await baltatharWallet.signTransaction({
      type: 2,
      chainId,
      nonce,
      to: tokenAddress,
      data: transferCalldata(baltathar.public, 100n),
      gasLimit: 50_000n,
      maxFeePerGas: 1_000_000_000_000n,
      maxPriorityFeePerGas: 0n,
      value: 0n,
    });

    const { txResults } = await context.createBlock({ transactions: [rawTx] });
    const txHash = txResults[0] as string;
    expect(txHash, 'tx should be accepted').to.match(/^0x/);

    const receipt = await context.web3.eth.getTransactionReceipt(txHash);
    expect(receipt.status).to.equal(1n, 'transfer should succeed after unblocking');
  });
});
