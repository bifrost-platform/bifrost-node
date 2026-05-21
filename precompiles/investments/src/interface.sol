// SPDX-License-Identifier: GPL-3.0-only
pragma solidity >=0.8.0;

/**
 * @title Investments Precompile Interface
 * @notice Called by the CCCP receiver contract when a requestDeposit or requestRedeem
 *         message arrives on Bifrost from an external EVM chain.
 * Address: 0x0000000000000000000000000000000000000200
 */
interface Investments {
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
     * @notice Execute confirmed redeem orders for a tranche during the settlement window.
     * @dev Called by the borrower after depositing `usdc_amount` to the Spoke Treasury.
     *      Drains ConfirmedRedeemOrders and emits RedeemOrdersExecuted so the off-chain
     *      bot can distribute USDC to each investor proportionally.
     * @param pool_id       The pool ID
     * @param chain_id      EVM chain ID of the chain where the vault is deployed
     * @param vault_address ERC-7540 vault contract address on that chain
     * @param usdc_amount   USDC deposited to the Spoke Treasury to cover redemptions
     */
    function executeRedeemOrders(
        uint64 pool_id,
        uint64 chain_id,
        address vault_address,
        uint256 usdc_amount
    ) external;
}
