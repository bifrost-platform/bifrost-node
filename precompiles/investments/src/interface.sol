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
     * @notice Submit a pending invest order for epoch settlement.
     * @dev Accumulates the USDC amount into PendingInvestOrders storage.
     *      Emits InvestOrderSubmitted on the substrate side.
     * @param pool_id       The pool ID
     * @param chain_id      EVM chain ID of the chain where the vault is deployed
     * @param vault_address ERC-7540 vault contract address on that chain
     * @param investor      Investor address on the external chain
     * @param amount        USDC amount to invest (18-decimal U256)
     */
    function submitInvestOrder(
        uint64 pool_id,
        uint64 chain_id,
        address vault_address,
        address investor,
        uint256 amount
    ) external;

    /**
     * @notice Submit a pending redeem order for epoch settlement.
     * @dev Accumulates the tranche token amount into PendingRedeemOrders storage.
     *      Emits RedeemOrderSubmitted on the substrate side.
     * @param pool_id       The pool ID
     * @param chain_id      EVM chain ID of the chain where the vault is deployed
     * @param vault_address ERC-7540 vault contract address on that chain
     * @param investor      Investor address on the external chain
     * @param amount        Tranche token amount to redeem (18-decimal U256)
     */
    function submitRedeemOrder(
        uint64 pool_id,
        uint64 chain_id,
        address vault_address,
        address investor,
        uint256 amount
    ) external;
}
