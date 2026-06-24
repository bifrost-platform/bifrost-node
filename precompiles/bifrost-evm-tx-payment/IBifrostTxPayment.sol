// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

/**
 * @title IBifrostTxPayment
 * @dev Interface for the Bifrost EVM Transaction Payment precompile
 * @notice Precompile address: 0x0000000000000000000000000000000000000810
 *
 * This precompile allows users to:
 * - Set their preferred ERC20 token for gas fee payment
 * - Query fee estimates in different tokens
 * - Check accepted tokens and their configurations
 */
interface IBifrostTxPayment {
    // ========================================================================
    // Events
    // ========================================================================

    /**
     * @dev Emitted when a user sets or clears their fee token preference
     * @param user The user's address
     * @param token The fee token address (zero address means native token)
     */
    event UserFeeTokenSet(address indexed user, address token);

    // ========================================================================
    // User Functions
    // ========================================================================

    /**
     * @notice Set the caller's preferred fee token
     * @param token The ERC20 token address to use for fee payment
     *
     * Requirements:
     * - Token must be in the accepted tokens list
     * - Token must be enabled
     */
    function setUserFeeToken(address token) external;

    /**
     * @notice Clear the caller's fee token preference (use native BFC)
     */
    function clearUserFeeToken() external;

    // ========================================================================
    // View Functions
    // ========================================================================

    /**
     * @notice Get a user's current fee token preference
     * @param user The user address to query
     * @return The fee token address (zero address means native token)
     */
    function getUserFeeToken(address user) external view returns (address);

    /**
     * @notice Get all users who have set a fee token preference
     * @return users Array of user addresses
     * @return tokens Array of corresponding token addresses
     * @dev WARNING: This iterates over all storage entries and may be expensive with many users
     */
    function getUsersFeeToken() external view returns (address[] memory users, address[] memory tokens);

    /**
     * @notice Estimate the fee amount in a specific token
     * @param token The ERC20 token address
     * @param gasAmount The gas amount to estimate for
     * @return The estimated fee in token units
     */
    function estimateFeeInToken(address token, uint256 gasAmount) external view returns (uint256);

    /**
     * @notice Check if a token is accepted for fee payment
     * @param token The token address to check
     * @return True if the token is accepted and enabled
     */
    function isAcceptedToken(address token) external view returns (bool);

    /**
     * @notice Get the configuration for a fee token
     * @param token The token address to query
     * @return enabled Whether the token is enabled
     * @return oracle The oracle contract address for price feeds
     * @return decimals The token's decimals
     * @return oracleDecimals The oracle's price decimals
     */
    function getTokenConfig(address token)
    external
    view
    returns (
        bool enabled,
        address oracle,
        uint8 decimals,
        uint8 oracleDecimals
    );

    /**
     * @notice Get the current oracle price for a token
     * @param token The token address
     * @return The price (token per native, with oracle decimals)
     */
    function getTokenPrice(address token) external view returns (uint256);

    /**
     * @notice Get the native token fee for a given gas amount
     * @param gasAmount The gas amount
     * @return The fee in native token (wei)
     */
    function getNativeFee(uint256 gasAmount) external view returns (uint256);
}

/**
 * @title BifrostTxPaymentHelper
 * @dev Helper library for interacting with the Bifrost TX Payment precompile
 */
library BifrostTxPaymentHelper {
    /// @dev The precompile address
    IBifrostTxPayment constant TX_PAYMENT = IBifrostTxPayment(0x0000000000000000000000000000000000000810);

    /**
     * @notice Setup a token for fee payment
     * @param token The ERC20 token to use
     */
    function setupFeeToken(IERC20 token) internal {
        // Set as fee token (no approval needed - transfer is used)
        TX_PAYMENT.setUserFeeToken(address(token));
    }

    /**
     * @notice Get fee estimate with fallback
     * @param token The token to estimate for
     * @param gasAmount The gas amount
     * @return tokenFee The fee in token units
     * @return nativeFee The fee in native units
     */
    function estimateFeeWithFallback(address token, uint256 gasAmount)
    internal
    view
    returns (uint256 tokenFee, uint256 nativeFee)
    {
        nativeFee = TX_PAYMENT.getNativeFee(gasAmount);

        if (TX_PAYMENT.isAcceptedToken(token)) {
            tokenFee = TX_PAYMENT.estimateFeeInToken(token, gasAmount);
        } else {
            tokenFee = 0;
        }
    }
}

/**
 * @title IERC20
 * @dev Minimal ERC20 interface for approvals
 */
interface IERC20 {
    function approve(address spender, uint256 amount) external returns (bool);

    function allowance(address owner, address spender) external view returns (uint256);

    function balanceOf(address account) external view returns (uint256);
}

/**
 * @title Example: BifrostTxPaymentUser
 * @dev Example contract showing how to use the Bifrost TX Payment system
 */
contract BifrostTxPaymentExample {
    IBifrostTxPayment constant TX_PAYMENT = IBifrostTxPayment(0x0000000000000000000000000000000000000810);

    IERC20 public immutable paymentToken;

    constructor(address _paymentToken) {
        paymentToken = IERC20(_paymentToken);
    }

    /**
     * @notice One-time setup to enable ERC20 fee payment
     * @dev No approval needed - the system uses transfer from user
     */
    function enableTokenFeePayment() external {
        // Just set as preferred fee token (no approval needed)
        TX_PAYMENT.setUserFeeToken(address(paymentToken));
    }

    /**
     * @notice Check if setup is complete
     */
    function isSetupComplete() external view returns (bool) {
        address currentFeeToken = TX_PAYMENT.getUserFeeToken(address(this));
        return currentFeeToken == address(paymentToken);
    }

    /**
     * @notice Estimate fee for a transaction
     */
    function estimateFee(uint256 gasAmount) external view returns (uint256) {
        return TX_PAYMENT.estimateFeeInToken(address(paymentToken), gasAmount);
    }

    /**
     * @notice Switch back to native token for fees
     */
    function useNativeTokenForFees() external {
        TX_PAYMENT.clearUserFeeToken();
    }
}
