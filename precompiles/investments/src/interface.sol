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
        address borrower,
        address investor_id,
        uint64 epoch_id,
        uint256 shares_to_mint
    );
    event RedeemOrderApproved(
        uint64 pool_id,
        uint64 chain_id,
        address vault_address,
        address borrower,
        address investor_id,
        uint64 epoch_id,
        uint256 payout_amount
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
     * @dev Only callable by the Gateway contract.
     *      Accumulates the USDC amount into PendingDepositOrders storage.
     *      Emits DepositOrderSubmitted on success.
     * @param pool_id       The pool ID
     * @param chain_id      EVM chain ID of the chain where the vault is deployed
     * @param vault_address ERC-7540 vault contract address on that chain
     * @param investor_id   Investor address on the external chain
     * @param amount        USDC amount to deposit (18-decimal U256)
     */
    function submit_deposit_order(
        uint64 pool_id,
        uint64 chain_id,
        address vault_address,
        address investor_id,
        uint256 amount
    ) external;

    /**
     * @notice Submit a pending redeem order for epoch settlement.
     * @dev Only callable by the Gateway contract.
     *      Accumulates the tranche token amount into PendingRedeemOrders storage.
     *      Emits RedeemOrderSubmitted on success.
     * @param pool_id       The pool ID
     * @param chain_id      EVM chain ID of the chain where the vault is deployed
     * @param vault_address ERC-7540 vault contract address on that chain
     * @param investor_id   Investor address on the external chain
     * @param amount        Tranche token amount to redeem (18-decimal U256)
     */
    function submit_redeem_order(
        uint64 pool_id,
        uint64 chain_id,
        address vault_address,
        address investor_id,
        uint256 amount
    ) external;

    /**
     * @notice Approve a selected set of investors' pending deposit orders (Approval mode).
     * @dev Only callable by the Gateway contract.
     *      Settlement window must be open. Converts USDC amounts to tokens-to-mint
     *      at the locked epoch price and moves entries to ApprovedDepositOrders.
     *      Emits DepositOrderApproved per investor.
     * @param pool_id       The pool ID
     * @param chain_id      EVM chain ID of the chain where the vault is deployed
     * @param vault_address ERC-7540 vault contract address on that chain
     * @param borrower      EVM address of the institution approving the orders
     * @param investor_ids  Investor addresses to approve (max 100, parallel with epoch_ids)
     * @param epoch_ids     Epoch IDs of the pending orders to approve (parallel with investor_ids)
     */
    function approve_deposit_orders(
        uint64 pool_id,
        uint64 chain_id,
        address vault_address,
        address borrower,
        address[] calldata investor_ids,
        uint64[] calldata epoch_ids
    ) external;

    /**
     * @notice Approve a selected set of investors' pending redeem orders (Approval mode).
     * @dev Only callable by the Gateway contract.
     *      Settlement window must be open. Converts token amounts to USDC-to-distribute
     *      at the locked epoch price and moves entries to ApprovedRedeemOrders.
     *      Emits RedeemOrderApproved per approved (investor, epoch) pair.
     * @param pool_id       The pool ID
     * @param chain_id      EVM chain ID of the chain where the vault is deployed
     * @param vault_address ERC-7540 vault contract address on that chain
     * @param borrower      EVM address of the institution approving the orders
     * @param investor_ids  Investor addresses to approve (max 100, parallel with epoch_ids)
     * @param epoch_ids     Epoch IDs of the pending orders to approve (parallel with investor_ids)
     */
    function approve_redeem_orders(
        uint64 pool_id,
        uint64 chain_id,
        address vault_address,
        address borrower,
        address[] calldata investor_ids,
        uint64[] calldata epoch_ids
    ) external;

    /**
     * @notice Automatic mode: claim settled deposit shares for an investor.
     * @dev Only callable by the Gateway contract.
     *      Moves the investor's entry from ClaimableDepositOrders to
     *      ApprovedDepositOrders so the Gateway can send the mint instruction.
     *      Emits SharesClaimed on success.
     * @param pool_id       The pool ID
     * @param chain_id      EVM chain ID of the chain where the vault is deployed
     * @param vault_address ERC-7540 vault contract address on that chain
     * @param investor_id   Investor address on the external chain
     */
    function claim_shares(
        uint64 pool_id,
        uint64 chain_id,
        address vault_address,
        address investor_id
    ) external;

    /**
     * @notice Automatic mode: claim settled redemption assets for an investor.
     * @dev Only callable by the Gateway contract.
     *      Moves the investor's entry from ClaimableRedeemOrders to
     *      ApprovedRedeemOrders so the Gateway can send the payout instruction.
     *      Emits AssetsClaimed on success.
     * @param pool_id       The pool ID
     * @param chain_id      EVM chain ID of the chain where the vault is deployed
     * @param vault_address ERC-7540 vault contract address on that chain
     * @param investor_id   Investor address on the external chain
     */
    function claim_assets(
        uint64 pool_id,
        uint64 chain_id,
        address vault_address,
        address investor_id
    ) external;
}
