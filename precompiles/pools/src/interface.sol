// SPDX-License-Identifier: GPL-3.0-only
pragma solidity >=0.8.0;

/**
 * @title Pools Precompile Interface
 * @notice Called by the CCCP receiver contract when a borrow or repay message
 *         arrives on Bifrost from an external EVM (Spoke) chain.
 * Address: 0x0000000000000000000000000000000000000201
 */
interface Pools {
    event Borrowed(
        uint64 pool_id,
        uint64 chain_id,
        address vault_address,
        uint256 amount
    );
    event Repaid(
        uint64 pool_id,
        uint64 chain_id,
        address vault_address,
        uint256 amount
    );

    /**
     * @notice Draw funds from a tranche treasury.
     * @dev Increments the tranche's `borrowed` counter on the Hub chain.
     *      Reverts if treasury liquidity (invested − borrowed) < amount.
     *      Emits Borrowed on the substrate side.
     * @param pool_id       The pool ID
     * @param chain_id      EVM chain ID of the chain where the vault is deployed
     * @param vault_address ERC-7540 vault contract address on that chain
     * @param amount        USDC amount to borrow (18-decimal U256)
     */
    function borrow(
        uint64 pool_id,
        uint64 chain_id,
        address vault_address,
        uint256 amount
    ) external;

    /**
     * @notice Return funds to a tranche treasury.
     * @dev Decrements the tranche's `borrowed` counter on the Hub chain.
     *      Saturates at zero — over-repayment does not revert.
     *      Emits Repaid on the substrate side.
     * @param pool_id       The pool ID
     * @param chain_id      EVM chain ID of the chain where the vault is deployed
     * @param vault_address ERC-7540 vault contract address on that chain
     * @param amount        USDC amount being repaid (18-decimal U256)
     */
    function repay(
        uint64 pool_id,
        uint64 chain_id,
        address vault_address,
        uint256 amount
    ) external;
}
