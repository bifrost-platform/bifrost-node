import { expect } from 'chai';
import { ethers } from 'ethers';

import { TEST_CONTROLLERS } from '../../constants/keys';
import { STAKING_ADDRESS } from '../../constants/staking_contract';
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

  it('should succeed after the 7702 delegation is removed', async () => {
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
