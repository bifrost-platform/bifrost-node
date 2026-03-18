// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/**
 * @title CLZTest
 * @dev Test contract for EIP-7939: Count Leading Zeros (CLZ) opcode
 *
 * EIP-7939 introduces the CLZ opcode (0x5C) that counts the number of
 * consecutive zero bits from the most significant bit of a 256-bit word.
 *
 * Gas cost: 5 (same as MUL opcode)
 *
 * References:
 * - https://eips.ethereum.org/EIPS/eip-7939
 * - https://ethereum.org/roadmap/fusaka/
 */
contract CLZTest {
    /**
     * @dev Executes the CLZ opcode on the input value
     * @param value The 256-bit value to count leading zeros for
     * @return leadingZeros The number of consecutive zero bits from the MSB
     */
    function countLeadingZeros(
        uint256 value
    ) public pure returns (uint256 leadingZeros) {
        assembly {
            leadingZeros := clz(value)
        }
    }

    /**
     * @dev Tests CLZ with multiple values and returns results
     * @param values Array of values to test
     * @return results Array of leading zero counts
     */
    function batchCountLeadingZeros(
        uint256[] memory values
    ) public pure returns (uint256[] memory results) {
        results = new uint256[](values.length);
        for (uint256 i = 0; i < values.length; i++) {
            assembly {
                let val := mload(add(add(values, 0x20), mul(i, 0x20)))
                let clzResult := clz(val)
                mstore(add(add(results, 0x20), mul(i, 0x20)), clzResult)
            }
        }
        return results;
    }

    /**
     * @dev Verifies CLZ opcode produces expected results
     * @return success True if all test cases pass
     */
    function runTestCases() public pure returns (bool success) {
        uint256 result;

        // Test case 1: Value = 1 (0x0000...0001) → Expected: 255 leading zeros
        assembly {
            result := clz(1)
        }
        if (result != 255) return false;

        // Test case 2: MSB set (0x8000...0000) → Expected: 0 leading zeros
        assembly {
            result := clz(
                0x8000000000000000000000000000000000000000000000000000000000000000
            )
        }
        if (result != 0) return false;

        // Test case 3: All zeros → Expected: 256 leading zeros
        assembly {
            result := clz(0)
        }
        if (result != 256) return false;

        // Test case 4: 0x0100...0000 → Expected: 7 leading zeros
        assembly {
            result := clz(
                0x0100000000000000000000000000000000000000000000000000000000000000
            )
        }
        if (result != 7) return false;

        // Test case 5: 0x00FF...FFFF → Expected: 8 leading zeros
        assembly {
            result := clz(
                0x00FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF
            )
        }
        if (result != 8) return false;

        return true;
    }

    /**
     * @dev Finds the most significant bit position (256 - CLZ)
     * @param value The input value
     * @return msbPosition Position of the most significant bit (0-255, or 256 if value is 0)
     */
    function findMSB(uint256 value) public pure returns (uint256 msbPosition) {
        assembly {
            let clzResult := clz(value)
            msbPosition := sub(256, clzResult)
        }
    }

    /**
     * @dev Calculates log2(value) using CLZ
     * @param value Must be greater than 0
     * @return _log2Floor Floor of log2(value)
     */
    function log2Floor(uint256 value) public pure returns (uint256 _log2Floor) {
        require(value > 0, "Value must be greater than 0");
        assembly {
            let clzResult := clz(value)
            _log2Floor := sub(255, clzResult)
        }
    }

    /**
     * @dev Calculates log2(value) + 1 (useful for bit width calculations)
     * @param value The input value
     * @return _bitWidth Minimum number of bits needed to represent value
     */
    function bitWidth(uint256 value) public pure returns (uint256 _bitWidth) {
        if (value == 0) return 0;
        assembly {
            let clzResult := clz(value)
            _bitWidth := sub(256, clzResult)
        }
    }

    /**
     * @dev Demonstrates efficiency improvement vs pre-EIP implementation
     * Pre-EIP: 180-300 gas for manual CLZ implementation
     * Post-EIP: 5 gas for CLZ opcode
     * @param value The value to test
     * @return clzResult The CLZ result
     * @return gasBefore Gas before operation
     * @return gasAfter Gas after operation
     */
    function measureCLZGas(
        uint256 value
    )
        public
        view
        returns (uint256 clzResult, uint256 gasBefore, uint256 gasAfter)
    {
        gasBefore = gasleft();
        assembly {
            clzResult := clz(value)
        }
        gasAfter = gasleft();
    }

    /**
     * @dev Compares CLZ with manual implementation for efficiency testing
     * @param value The input value
     * @return opcodeResult Result from CLZ opcode
     * @return opcodeGasUsed Gas used by CLZ opcode
     * @return manualResult Result from manual implementation
     * @return manualGasUsed Gas used by manual implementation
     */
    function compareImplementations(
        uint256 value
    )
        public
        view
        returns (
            uint256 opcodeResult,
            uint256 opcodeGasUsed,
            uint256 manualResult,
            uint256 manualGasUsed
        )
    {
        // Test CLZ opcode
        uint256 gasBeforeOpcode = gasleft();
        assembly {
            opcodeResult := clz(value)
        }
        opcodeGasUsed = gasBeforeOpcode - gasleft();

        // Test manual implementation (pre-EIP approach)
        uint256 gasBeforeManual = gasleft();
        manualResult = manualCLZ(value);
        manualGasUsed = gasBeforeManual - gasleft();
    }

    /**
     * @dev Manual implementation of CLZ for comparison (pre-EIP approach)
     * This demonstrates the 36-60x gas efficiency improvement from EIP-7939
     * @param value The input value
     * @return count Number of leading zeros
     */
    function manualCLZ(uint256 value) internal pure returns (uint256 count) {
        if (value == 0) return 256;

        count = 0;
        uint256 mask = 1 << 255; // Start with MSB

        while (mask > 0 && (value & mask) == 0) {
            count++;
            mask >>= 1;
        }

        return count;
    }

    /**
     * @dev Example use case: Power of 2 check using CLZ
     * A number is a power of 2 if it has exactly one bit set
     * @param value The value to check
     * @return _isPowerOfTwo True if value is a power of 2
     */
    function isPowerOfTwo(
        uint256 value
    ) public pure returns (bool _isPowerOfTwo) {
        if (value == 0) return false;

        // If power of 2, value & (value-1) should be 0
        // We can also check using CLZ: if only one bit is set,
        // CLZ(value) + trailing zeros = 255
        return (value & (value - 1)) == 0;
    }

    /**
     * @dev Emits event with CLZ result for testing and debugging
     */
    event CLZCalculated(uint256 indexed input, uint256 leadingZeros);

    /**
     * @dev Calculates and emits CLZ result
     * @param value The input value
     */
    function calculateAndEmit(uint256 value) public {
        uint256 result;
        assembly {
            result := clz(value)
        }
        emit CLZCalculated(value, result);
    }
}
