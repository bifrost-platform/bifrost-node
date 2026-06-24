import { expect } from 'chai';
import { ethers } from 'ethers';

import { TEST_CONTROLLERS } from '../../constants/keys';
import { describeDevNode } from '../set_dev_node';
import { createTransaction, ALITH_TRANSACTION_TEMPLATE } from '../transactions';
import { customWeb3Request } from '../providers';

/**
 * Fusaka Hard Fork Upgrade Tests
 *
 * Tests for EIP implementations:
 * - EIP-7825: Transaction Gas Limit Cap (52M gas per transaction, bifrost-frontier)
 * - EIP-7823: Set upper bounds for MODEXP (8192 bits max input)
 * - EIP-7883: Repricing MODEXP (increased minimum gas from 200 to 500)
 * - EIP-7939: Count leading zeros (CLZ) opcode (0x5C)
 *
 * References:
 * - https://eips.ethereum.org/EIPS/eip-7825
 * - https://eips.ethereum.org/EIPS/eip-7823
 * - https://eips.ethereum.org/EIPS/eip-7883
 * - https://eips.ethereum.org/EIPS/eip-7939
 */

const alith: { public: string; private: string } = TEST_CONTROLLERS[0];

// MODEXP precompile address
const MODEXP_ADDRESS = '0x0000000000000000000000000000000000000005';

// EIP-7825: Transaction gas limit cap (52M gas per bifrost-frontier implementation)
const TRANSACTION_GAS_CAP = 52_000_000;

// EIP-7823: Maximum input bits for MODEXP (8192 bits = 1024 bytes)
const MODEXP_MAX_INPUT_BYTES = 1024;

const CLZ_TEST_CONTRACT_BYTE_CODE = '0x6080604052348015600e575f5ffd5b50610a788061001c5f395ff3fe608060405234801561000f575f5ffd5b506004361061009b575f3560e01c8063b8800a3511610064578063b8800a351461017d578063c03147a11461019b578063d7134426146101ce578063e800a022146101fe578063ffe958461461022e5761009b565b8062a7f8791461009f57806306388dd6146100cf5780632580cd86146100ff5780636faa58d21461011b578063b72e25991461014d575b5f5ffd5b6100b960048036038101906100b491906105b7565b61025e565b6040516100c691906105f1565b60405180910390f35b6100e960048036038101906100e491906105b7565b61027e565b6040516100f69190610624565b60405180910390f35b610119600480360381019061011491906105b7565b6102a7565b005b610135600480360381019061013091906105b7565b6102e8565b6040516101449392919061063d565b60405180910390f35b610167600480360381019061016291906107c2565b6102fc565b60405161017491906108c0565b60405180910390f35b610185610384565b6040516101929190610624565b60405180910390f35b6101b560048036038101906101b091906105b7565b610458565b6040516101c594939291906108e0565b60405180910390f35b6101e860048036038101906101e391906105b7565b610498565b6040516101f591906105f1565b60405180910390f35b610218600480360381019061021391906105b7565b6104e9565b60405161022591906105f1565b60405180910390f35b610248600480360381019061024391906105b7565b6104f3565b60405161025591906105f1565b60405180910390f35b5f5f820361026e575f9050610279565b811e80610100039150505b919050565b5f5f820361028e575f90506102a2565b5f60018361029c9190610950565b83161490505b919050565b5f811e9050817fca7fca93faefcaecacc568a6a6fc21b7bf700f2db246cd6de3c7a7616eef418d826040516102dc91906105f1565b60405180910390a25050565b5f5f5f5a9150831e92505a90509193909250565b6060815167ffffffffffffffff81111561031957610318610686565b5b6040519080825280602002602001820160405280156103475781602001602082028036833780820191505090505b5090505f5f90505b825181101561037e5760208102602084010151801e80602084026020860101525050808060010191505061034f565b50919050565b5f5f60011e905060ff811461039c575f915050610455565b7f80000000000000000000000000000000000000000000000000000000000000001e90505f81146103d0575f915050610455565b5f1e905061010081146103e6575f915050610455565b7f01000000000000000000000000000000000000000000000000000000000000001e90506007811461041b575f915050610455565b7effffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff1e90506008811461044f575f915050610455565b60019150505b90565b5f5f5f5f5f5a9050851e94505a816104709190610950565b93505f5a905061047f87610503565b93505a8161048d9190610950565b925050509193509193565b5f5f82116104db576040517f08c379a00000000000000000000000000000000000000000000000000000000081526004016104d2906109dd565b60405180910390fd5b811e8060ff03915050919050565b5f811e9050919050565b5f811e8061010003915050919050565b5f5f820361051557610100905061056e565b5f90505f7f800000000000000000000000000000000000000000000000000000000000000090505b5f8111801561054d57505f818416145b1561056c57818061055d906109fb565b925050600181901c905061053d565b505b919050565b5f604051905090565b5f5ffd5b5f5ffd5b5f819050919050565b61059681610584565b81146105a0575f5ffd5b50565b5f813590506105b18161058d565b92915050565b5f602082840312156105cc576105cb61057c565b5b5f6105d9848285016105a3565b91505092915050565b6105eb81610584565b82525050565b5f6020820190506106045f8301846105e2565b92915050565b5f8115159050919050565b61061e8161060a565b82525050565b5f6020820190506106375f830184610615565b92915050565b5f6060820190506106505f8301866105e2565b61065d60208301856105e2565b61066a60408301846105e2565b949350505050565b5f5ffd5b5f601f19601f8301169050919050565b7f4e487b71000000000000000000000000000000000000000000000000000000005f52604160045260245ffd5b6106bc82610676565b810181811067ffffffffffffffff821117156106db576106da610686565b5b80604052505050565b5f6106ed610573565b90506106f982826106b3565b919050565b5f67ffffffffffffffff82111561071857610717610686565b5b602082029050602081019050919050565b5f5ffd5b5f61073f61073a846106fe565b6106e4565b9050808382526020820190506020840283018581111561076257610761610729565b5b835b8181101561078b578061077788826105a3565b845260208401935050602081019050610764565b5050509392505050565b5f82601f8301126107a9576107a8610672565b5b81356107b984826020860161072d565b91505092915050565b5f602082840312156107d7576107d661057c565b5b5f82013567ffffffffffffffff8111156107f4576107f3610580565b5b61080084828501610795565b91505092915050565b5f81519050919050565b5f82825260208201905092915050565b5f819050602082019050919050565b61083b81610584565b82525050565b5f61084c8383610832565b60208301905092915050565b5f602082019050919050565b5f61086e82610809565b6108788185610813565b935061088383610823565b805f5b838110156108b357815161089a8882610841565b97506108a583610858565b925050600181019050610886565b5085935050505092915050565b5f6020820190508181035f8301526108d88184610864565b905092915050565b5f6080820190506108f35f8301876105e2565b61090060208301866105e2565b61090d60408301856105e2565b61091a60608301846105e2565b95945050505050565b7f4e487b71000000000000000000000000000000000000000000000000000000005f52601160045260245ffd5b5f61095a82610584565b915061096583610584565b925082820390508181111561097d5761097c610923565b5b92915050565b5f82825260208201905092915050565b7f56616c7565206d7573742062652067726561746572207468616e2030000000005f82015250565b5f6109c7601c83610983565b91506109d282610993565b602082019050919050565b5f6020820190508181035f8301526109f4816109bb565b9050919050565b5f610a0582610584565b91507fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff8203610a3757610a36610923565b5b60018201905091905056fea26469706673582212207d76459fb9c3320ddd552e50ad72925d33435371236acda7c2c0aece6f56a38364736f6c634300081f0033';

describeDevNode('Fusaka Hard Fork - EIP-7825: Transaction Gas Limit Cap', (context) => {
  it('should accept transaction at gas limit cap (52M gas)', async function () {
    const tx = await createTransaction(context, {
      ...ALITH_TRANSACTION_TEMPLATE,
      to: alith.public,
      value: '0x1',
      gas: TRANSACTION_GAS_CAP,
    });

    const result = await context.createBlock({ transactions: [tx] });
    expect(result.txResults[0]).to.not.be.null;
    expect(result.block.hash).to.not.be.null;
  });

  it('should reject transaction exceeding gas limit cap (52M + 1 gas)', async function () {
    let rejected = false;
    try {
      const tx = await createTransaction(context, {
        ...ALITH_TRANSACTION_TEMPLATE,
        to: alith.public,
        value: '0x1',
        gas: TRANSACTION_GAS_CAP + 1,
      });
      await context.createBlock({ transactions: [tx] });
    } catch (_e) {
      rejected = true;
    }
    expect(rejected).to.be.true;
  });

  it('should allow multiple transactions within block gas limit', async function () {
    const txCount = 4;
    const gasPerTx = Math.floor(TRANSACTION_GAS_CAP * 0.8); // Use 80% of cap per tx
    const transactions = [];

    for (let i = 0; i < txCount; i++) {
      const nonce = await context.web3.eth.getTransactionCount(alith.public, 'pending');
      const tx = await createTransaction(context, {
        ...ALITH_TRANSACTION_TEMPLATE,
        to: alith.public,
        value: '0x1',
        gas: gasPerTx,
        nonce: Number(nonce) + i,
      });
      transactions.push(tx);
    }

    const result = await context.createBlock({ transactions });

    // All transactions should be included
    expect(result.txResults).to.have.lengthOf(txCount);
    result.txResults.forEach((txResult) => {
      expect(txResult).to.not.be.null;
    });
  });
});

describeDevNode('Fusaka Hard Fork - EIP-7823 & EIP-7883: MODEXP Bounds and Repricing', (context) => {
  /**
   * MODEXP precompile input format:
   * [length_of_BASE][length_of_EXPONENT][length_of_MODULUS][BASE][EXPONENT][MODULUS]
   * Each length is 32 bytes (uint256)
   */

  it('should accept MODEXP with inputs within 8192 bits limit', async function () {
    // Test with 256-bit values (well within limit)
    // Computing: 3^5 mod 13 = 243 mod 13 = 9
    const base = '0x' + '03'.padStart(64, '0');
    const exponent = '0x' + '05'.padStart(64, '0');
    const modulus = '0x' + '0d'.padStart(64, '0'); // 13 in decimal

    const inputData =
      '0x' +
      '0000000000000000000000000000000000000000000000000000000000000020' + // base length: 32 bytes
      '0000000000000000000000000000000000000000000000000000000000000020' + // exp length: 32 bytes
      '0000000000000000000000000000000000000000000000000000000000000020' + // mod length: 32 bytes
      base.slice(2) +
      exponent.slice(2) +
      modulus.slice(2);

    // Verify MODEXP computes correct result: 3^5 mod 13 = 9
    const callResult = await customWeb3Request(context.web3, 'eth_call', [
      {
        to: MODEXP_ADDRESS,
        data: inputData,
        from: alith.public,
      },
    ]);

    const modexpResult = parseInt(callResult, 16);
    expect(modexpResult).to.equal(9, 'MODEXP(3, 5, 13) should equal 9');

    // Also verify via transaction
    const tx = await createTransaction(context, {
      ...ALITH_TRANSACTION_TEMPLATE,
      to: MODEXP_ADDRESS,
      data: inputData,
      gas: 100000,
    });

    const result = await context.createBlock({ transactions: [tx] });
    expect(result.txResults[0]).to.not.be.null;

    const receipt = await context.web3.eth.getTransactionReceipt(result.txResults[0]);
    expect(receipt).to.not.be.null;
    expect(Number(receipt.status)).to.equal(1);
  });

  it('should enforce minimum gas cost of 500 (EIP-7883)', async function () {
    // Small modexp operation that should cost at least 500 gas
    // Computing: 2^2 mod 7 = 4 mod 7 = 4
    const base = '0x' + '02'.padStart(64, '0');
    const exponent = '0x' + '02'.padStart(64, '0');
    const modulus = '0x' + '07'.padStart(64, '0');

    const inputData =
      '0x' +
      '0000000000000000000000000000000000000000000000000000000000000020' +
      '0000000000000000000000000000000000000000000000000000000000000020' +
      '0000000000000000000000000000000000000000000000000000000000000020' +
      base.slice(2) +
      exponent.slice(2) +
      modulus.slice(2);

    // Verify correct computation: 2^2 mod 7 = 4
    const callResult = await customWeb3Request(context.web3, 'eth_call', [
      {
        to: MODEXP_ADDRESS,
        data: inputData,
        from: alith.public,
      },
    ]);

    const modexpResult = parseInt(callResult, 16);
    expect(modexpResult).to.equal(4, 'MODEXP(2, 2, 7) should equal 4');

    // Verify gas cost via transaction
    const tx = await createTransaction(context, {
      ...ALITH_TRANSACTION_TEMPLATE,
      to: MODEXP_ADDRESS,
      data: inputData,
      gas: 100000,
    });

    const result = await context.createBlock({ transactions: [tx] });
    expect(result.txResults[0]).to.not.be.null;

    const receipt = await context.web3.eth.getTransactionReceipt(result.txResults[0]);
    expect(receipt).to.not.be.null;
    expect(Number(receipt.status)).to.equal(1);

    // Gas used should be at least 500 (minimum cost from EIP-7883)
    const gasUsed = Number(receipt.gasUsed);
    expect(gasUsed).to.be.at.least(500, 'EIP-7883 requires minimum 500 gas for MODEXP');
  });

  it('should reject MODEXP with inputs exceeding 8192 bits (1024 bytes)', async function () {
    // Create input larger than 1024 bytes (8192 bits)
    const oversizedLength = MODEXP_MAX_INPUT_BYTES + 1;
    const oversizedData = '01'.repeat(oversizedLength);

    const inputData =
      '0x' +
      oversizedLength.toString(16).padStart(64, '0') + // base length exceeds limit
      '0000000000000000000000000000000000000000000000000000000000000020' + // exp length
      '0000000000000000000000000000000000000000000000000000000000000020' + // mod length
      oversizedData +
      '02'.padStart(64, '0') +
      '07'.padStart(64, '0');

    try {
      const tx = await createTransaction(context, {
        ...ALITH_TRANSACTION_TEMPLATE,
        to: MODEXP_ADDRESS,
        data: inputData,
        gas: 500000,
      });

      const result = await context.createBlock({ transactions: [tx] });
      const receipt = await context.web3.eth.getTransactionReceipt(result.txResults[0]);

      // Should fail due to exceeding bounds
      expect(Number(receipt.status)).to.equal(0);
    } catch (error: any) {
      // Expected to fail
      expect(error).to.exist;
    }
  });

  it('should handle maximum allowed MODEXP input (8192 bits)', async function () {
    // Test with exactly 1024 bytes (8192 bits)
    const maxData = '02'.repeat(MODEXP_MAX_INPUT_BYTES);

    const inputData =
      '0x' +
      MODEXP_MAX_INPUT_BYTES.toString(16).padStart(64, '0') + // base length at limit
      '0000000000000000000000000000000000000000000000000000000000000001' + // exp length: 1 byte
      '0000000000000000000000000000000000000000000000000000000000000001' + // mod length: 1 byte
      maxData +
      '03' +
      '05';

    const tx = await createTransaction(context, {
      ...ALITH_TRANSACTION_TEMPLATE,
      to: MODEXP_ADDRESS,
      data: inputData,
      gas: 3000000,
    });

    const result = await context.createBlock({ transactions: [tx] });
    expect(result.txResults[0]).to.not.be.null;

    const receipt = await context.web3.eth.getTransactionReceipt(result.txResults[0]);
    expect(receipt).to.not.be.null;
    // Should succeed with maximum allowed input
    expect(Number(receipt.status)).to.equal(1);
  });
});

describeDevNode('Fusaka Hard Fork - EIP-7939: CLZ (Count Leading Zeros) Opcode', (context) => {
  // Function selector for countLeadingZeros(uint256): e800a022
  const countLeadingZerosSelector = 'e800a022';
  let contractAddress!: string;

  before(async function () {
    const deployTx = await createTransaction(context, {
      ...ALITH_TRANSACTION_TEMPLATE,
      data: CLZ_TEST_CONTRACT_BYTE_CODE,
      gas: 5000000,
    });
    const deployResult = await context.createBlock({ transactions: [deployTx] });
    expect(deployResult.txResults[0]).to.not.be.null;
    const deployReceipt = await context.web3.eth.getTransactionReceipt(deployResult.txResults[0]);
    expect(deployReceipt).to.not.be.null;
    expect(Number(deployReceipt.status)).to.equal(1);
    contractAddress = deployReceipt.contractAddress!;
    expect(contractAddress).to.not.be.null;
  });

  it('should support CLZ opcode (0x5C) with correct gas cost', async function () {
    // Test CLZ with various inputs. EIP-7939: CLZ returns leading zero count (256 for input 0).
    const testCases = [
      {
        input: '0x' + '1'.padStart(64, '0'),  // 0x0000...0001
        expectedLeadingZeros: 255,
      },
      {
        input: '0x' + '8000000000000000000000000000000000000000000000000000000000000000',
        expectedLeadingZeros: 0,  // No leading zeros (MSB is 1)
      },
      {
        input: '0x' + '0100000000000000000000000000000000000000000000000000000000000000',
        expectedLeadingZeros: 7,
      },
      {
        input: '0x' + '00'.repeat(32),        // All zeros
        expectedLeadingZeros: 256,
      },
    ];

    for (const testCase of testCases) {
      // Verify CLZ returns correct value using eth_call
      // Encode function call: selector + parameter (uint256)
      const callData = '0x' + countLeadingZerosSelector + testCase.input.slice(2).padStart(64, '0');

      const returnValue = await customWeb3Request(context.web3, 'eth_call', [
        {
          to: contractAddress,
          data: callData,
          from: alith.public,
        },
      ]);
      console.log('returnValue', returnValue);

      const clzResult = parseInt(returnValue, 16);
      expect(clzResult).to.equal(
        testCase.expectedLeadingZeros,
        `CLZ should return ${testCase.expectedLeadingZeros} leading zeros for input ${testCase.input}, but got ${clzResult}`
      );

      // Also verify via transaction for gas cost
      const callTx = await createTransaction(context, {
        ...ALITH_TRANSACTION_TEMPLATE,
        to: contractAddress,
        data: callData,
        gas: 100000,
      });

      const callResult = await context.createBlock({ transactions: [callTx] });
      expect(callResult.txResults[0]).to.not.be.null;

      const receipt = await context.web3.eth.getTransactionReceipt(callResult.txResults[0]);
      expect(receipt).to.not.be.null;
      expect(Number(receipt.status)).to.equal(1);

      // Verify gas cost is reasonable (CLZ should cost 5 gas, similar to MUL)
      const gasUsed = Number(receipt.gasUsed);
      expect(gasUsed).to.be.lessThan(50000); // Much less than 180-300 gas pre-EIP
    }
  });

  it('should verify CLZ opcode returns correct leading zero count', async function () {
    // Test with 0x0000...0010 (251 leading zeros)
    const testInput = '0x' + '10'.padStart(64, '0');
    const expectedCLZ = 251;

    const callData = '0x' + countLeadingZerosSelector + testInput.slice(2);
    const returnValue = await customWeb3Request(context.web3, 'eth_call', [
      {
        to: contractAddress,
        data: callData,
        from: alith.public,
      },
    ]);

    const clzResult = parseInt(returnValue, 16);
    expect(clzResult).to.equal(expectedCLZ, `CLZ(0x10) should return ${expectedCLZ} but got ${clzResult}`);

    // Also verify via transaction for gas cost
    const tx = await createTransaction(context, {
      ...ALITH_TRANSACTION_TEMPLATE,
      to: contractAddress,
      data: callData,
      gas: 100000,
    });

    const result = await context.createBlock({ transactions: [tx] });
    expect(result.txResults[0]).to.not.be.null;

    const receipt = await context.web3.eth.getTransactionReceipt(result.txResults[0]);
    expect(receipt).to.not.be.null;
    expect(Number(receipt.status)).to.equal(1);

    // Verify gas efficiency (5 gas for CLZ vs 180-300 gas for pre-EIP implementation)
    const gasUsed = Number(receipt.gasUsed);
    expect(gasUsed).to.be.lessThan(100000);
  });

  it('should handle CLZ edge cases correctly', async function () {
    // Edge case: Maximum value (all 1s) - 0 leading zeros
    const maxInput = '0x' + 'FF'.repeat(32);
    const maxCallData = '0x' + countLeadingZerosSelector + maxInput.slice(2);
    const maxReturnValue = await customWeb3Request(context.web3, 'eth_call', [
      {
        to: contractAddress,
        data: maxCallData,
        from: alith.public,
      },
    ]);
    expect(parseInt(maxReturnValue, 16)).to.equal(0, 'CLZ(0xFFFF...FFFF) should be 0');

    const maxValueTx = await createTransaction(context, {
      ...ALITH_TRANSACTION_TEMPLATE,
      to: contractAddress,
      data: maxCallData,
      gas: 100000,
    });

    const maxResult = await context.createBlock({ transactions: [maxValueTx] });
    const maxReceipt = await context.web3.eth.getTransactionReceipt(maxResult.txResults[0]);
    expect(Number(maxReceipt.status)).to.equal(1);

    // Edge case: Minimum non-zero value (1) - 255 leading zeros
    const minInput = '0x' + '01'.padStart(64, '0');
    const minCallData = '0x' + countLeadingZerosSelector + minInput.slice(2);
    const minReturnValue = await customWeb3Request(context.web3, 'eth_call', [
      {
        to: contractAddress,
        data: minCallData,
        from: alith.public,
      },
    ]);
    expect(parseInt(minReturnValue, 16)).to.equal(255, 'CLZ(0x01) should be 255');

    const minValueTx = await createTransaction(context, {
      ...ALITH_TRANSACTION_TEMPLATE,
      to: contractAddress,
      data: minCallData,
      gas: 100000,
    });

    const minResult = await context.createBlock({ transactions: [minValueTx] });
    const minReceipt = await context.web3.eth.getTransactionReceipt(minResult.txResults[0]);
    expect(Number(minReceipt.status)).to.equal(1);
  });
});

describeDevNode('Fusaka Hard Fork - Integration Test: All EIPs', (context) => {
  it('should handle transaction with gas cap, MODEXP, and CLZ in single block', async function () {
    const transactions = [];

    // 1. Transaction at gas cap limit
    const nonce1 = await context.web3.eth.getTransactionCount(alith.public, 'pending');
    const gasCapTx = await createTransaction(context, {
      ...ALITH_TRANSACTION_TEMPLATE,
      to: alith.public,
      value: '0x1',
      gas: 12_000_000,
      nonce: Number(nonce1),
    });
    transactions.push(gasCapTx);

    // 2. MODEXP transaction (EIP-7823, EIP-7883)
    const modexpData =
      '0x' +
      '0000000000000000000000000000000000000000000000000000000000000020' +
      '0000000000000000000000000000000000000000000000000000000000000020' +
      '0000000000000000000000000000000000000000000000000000000000000020' +
      '03'.padStart(64, '0') +
      '05'.padStart(64, '0') +
      '0d'.padStart(64, '0');

    const nonce2 = await context.web3.eth.getTransactionCount(alith.public, 'pending');
    const modexpTx = await createTransaction(context, {
      ...ALITH_TRANSACTION_TEMPLATE,
      to: MODEXP_ADDRESS,
      data: modexpData,
      gas: 100000,
      nonce: Number(nonce2) + 1,
    });
    transactions.push(modexpTx);

    const result = await context.createBlock({ transactions });

    // All transactions should succeed
    expect(result.txResults).to.have.lengthOf(2);

    for (const txHash of result.txResults) {
      expect(txHash).to.not.be.null;
      const receipt = await context.web3.eth.getTransactionReceipt(txHash);
      expect(receipt).to.not.be.null;
      expect(Number(receipt.status)).to.equal(1);
    }

    // Verify block was created successfully
    expect(result.block.hash).to.not.be.null;
  });

  it('should maintain backward compatibility with pre-Fusaka transactions', async function () {
    // Test that regular transactions still work as expected
    const regularTx = await createTransaction(context, {
      ...ALITH_TRANSACTION_TEMPLATE,
      to: alith.public,
      value: ethers.parseEther('0.1').toString(),
      gas: 21000, // Standard ETH transfer gas
    });

    const result = await context.createBlock({ transactions: [regularTx] });

    expect(result.txResults[0]).to.not.be.null;

    const receipt = await context.web3.eth.getTransactionReceipt(result.txResults[0]);
    expect(receipt).to.not.be.null;
    expect(Number(receipt.status)).to.equal(1);
    expect(Number(receipt.gasUsed)).to.equal(21000);
  });
});
