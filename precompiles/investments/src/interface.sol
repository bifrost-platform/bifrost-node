// SPDX-License-Identifier: GPL-3.0-only
pragma solidity >=0.8.0;

/**
 * @title Investments Precompile Interface
 * @notice Called exclusively by the Gateway contract. Dispatches invest/redeem
 *         order operations to the pallet-investments Substrate pallet.
 * Address: 0x0000000000000000000000000000000000000200
 */
interface Investments {
    event DepositOrderSubmitted(
        uint64 pool_id,
        uint64 chain_id,
        address vault_address,
        address investor_id,
        uint256 amount
    );
    event RedeemOrderSubmitted(
        uint64 pool_id,
        uint64 chain_id,
        address vault_address,
        address investor_id,
        uint256 amount
    );
    event DepositOrderApproved(
        uint64 pool_id,
        uint64 chain_id,
        address vault_address,
        address investor_id
    );
    event RedeemOrderApproved(
        uint64 pool_id,
        uint64 chain_id,
        address vault_address,
        address investor_id
    );
    event SharesClaimed(
        uint64 pool_id,
        uint64 chain_id,
        address vault_address,
        address investor_id,
        uint256 shares_to_mint
    );
    event AssetsClaimed(
        uint64 pool_id,
        uint64 chain_id,
        address vault_address,
        address investor_id,
        uint256 payout
    );

    /**
     * @notice Submit a pending deposit order for epoch settlement.
     * @dev Accumulates the USDC amount into PendingDepositOrders storage.
     *      Emits DepositOrderSubmitted on the substrate side.
     * @param pool_id       The pool ID
     * @param chain_id      EVM chain ID of the chain where the vault is deployed
     * @param vault_address ERC-7540 vault contract address on that chain
     * @param investor_id   Investor address on the external chain
     * @param amount        USDC amount to deposit (18-decimal U256)
     */
    function submitDepositOrder(
        uint64 pool_id,
        uint64 chain_id,
        address vault_address,
        address investor_id,
        uint256 amount
    ) external;

    /**
     * @notice Submit a pending redeem order for epoch settlement.
     * @dev Accumulates the tranche token amount into PendingRedeemOrders storage.
     *      Emits RedeemOrderSubmitted on the substrate side.
     * @param pool_id       The pool ID
     * @param chain_id      EVM chain ID of the chain where the vault is deployed
     * @param vault_address ERC-7540 vault contract address on that chain
     * @param investor_id   Investor address on the external chain
     * @param amount        Tranche token amount to redeem (18-decimal U256)
     */
    function submitRedeemOrder(
        uint64 pool_id,
        uint64 chain_id,
        address vault_address,
        address investor_id,
        uint256 amount
    ) external;

    /**
     * @notice Approve a selected set of investors' pending deposit orders.
     * @dev Settlement window must be open. Converts USDC amounts to tokens-to-mint
     *      at the locked epoch price and moves entries to ApprovedDepositOrders.
     *      Emits DepositOrderConfirmed per investor.
     * @param pool_id       The pool ID
     * @param chain_id      EVM chain ID of the chain where the vault is deployed
     * @param vault_address ERC-7540 vault contract address on that chain
     * @param investor_ids  Investor addresses to approve (max 100)
     */
    function approveDepositOrders(
        uint64 pool_id,
        uint64 chain_id,
        address vault_address,
        address[] calldata investor_ids
    ) external;

    /**
     * @notice Approve a selected set of investors' pending redeem orders.
     * @dev Settlement window must be open. Converts token amounts to USDC-to-distribute
     *      at the locked epoch price and moves entries to ApprovedRedeemOrders.
     *      Emits RedeemOrderConfirmed per investor.
     * @param pool_id       The pool ID
     * @param chain_id      EVM chain ID of the chain where the vault is deployed
     * @param vault_address ERC-7540 vault contract address on that chain
     * @param investor_ids  Investor addresses to approve (max 100)
     */
    function approveRedeemOrders(
        uint64 pool_id,
        uint64 chain_id,
        address vault_address,
        address[] calldata investor_ids
    ) external;

    /**
     * @notice Automatic mode: claim settled deposit shares for an investor.
     * @dev Moves the investor's entry from ClaimableDepositOrders to
     *      ApprovedDepositOrders. Called by the Gateway when a
     *      requestTrancheClaim() message for a deposit arrives from the spoke chain.
     *      Emits DepositClaimed on the substrate side.
     * @param pool_id       The pool ID
     * @param chain_id      EVM chain ID of the chain where the vault is deployed
     * @param vault_address ERC-7540 vault contract address on that chain
     * @param investor_id   Investor address on the external chain
     */
    function claimShares(
        uint64 pool_id,
        uint64 chain_id,
        address vault_address,
        address investor_id
    ) external;

    /**
     * @notice Automatic mode: claim settled redemption assets for an investor.
     * @dev Moves the investor's entry from ClaimableRedeemOrders to
     *      ApprovedRedeemOrders. Called by the Gateway when a
     *      requestTrancheClaim() message for a redemption arrives from the spoke chain.
     *      Emits RedeemClaimed on the substrate side.
     * @param pool_id       The pool ID
     * @param chain_id      EVM chain ID of the chain where the vault is deployed
     * @param vault_address ERC-7540 vault contract address on that chain
     * @param investor_id   Investor address on the external chain
     */
    function claimAssets(
        uint64 pool_id,
        uint64 chain_id,
        address vault_address,
        address investor_id
    ) external;
}
