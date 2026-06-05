// SPDX-License-Identifier: GPL-3.0-only
pragma solidity >=0.8.0;

/**
 * @title Pools Precompile Interface
 * @notice Manages RWA pool creation on the Hub chain and relays borrow/repay
 *         messages that arrive from an external EVM (Spoke) chain via CCCP.
 * Address: 0x0000000000000000000000000000000000000201
 */
interface Pools {
    struct CollateralInput {
        address nft_contract;
        uint256 nft_token_id;
    }

    /// @param chain_id      EVM chain ID where the vault is deployed
    /// @param vault_address ERC-7540 vault contract address on the Spoke chain
    /// @param is_senior     true = Senior tranche (fixed APR), false = Junior tranche
    /// @param apr           Annual percentage rate as a FixedU128 inner value (1e18 = 100%)
    /// @param max_deposits  Maximum total deposits; 0 = uncapped
    struct TrancheInput {
        uint64 chain_id;
        address vault_address;
        bool is_senior;
        uint256 apr;
        uint256 max_deposits;
    }

    event PoolCreated(
        uint64 pool_id,
        address borrower_id,
        uint64 epoch_length_secs,
        uint64 settlement_offset_secs
    );
    event Borrowed(
        uint64 pool_id,
        uint64 chain_id,
        address vault_address,
        address borrower,
        uint256 amount
    );
    event Repaid(
        uint64 pool_id,
        uint64 chain_id,
        address vault_address,
        address borrower,
        uint256 amount
    );

    /**
     * @notice Create a new RWA pool on the Hub and deploy its vaults on the Spoke chain.
     * @dev Caller must hold the PoolAdmin role for `pool_id` (granted by sudo in advance).
     *      Reverts if the pool ID is already taken, collaterals list is empty,
     *      tranches list is empty, or settlement_offset is out of range.
     *      Emits PoolCreated on success.
     * @param pool_id                     Hub pool ID
     * @param borrower_id                 Institution's EVM address
     * @param epoch_length_secs           Epoch duration in seconds
     * @param settlement_offset_secs      Seconds before epoch end when the settlement window opens
     * @param deposit_settlement_approval true = Approval mode, false = Automatic
     * @param redeem_settlement_approval  true = Approval mode, false = Automatic
     * @param collaterals                 Array of collateral NFT (contract, tokenId) pairs
     * @param tranches                    Array of tranche configurations
     */
    function create_pool(
        uint64 pool_id,
        address borrower_id,
        uint64 epoch_length_secs,
        uint64 settlement_offset_secs,
        bool deposit_settlement_approval,
        bool redeem_settlement_approval,
        CollateralInput[] calldata collaterals,
        TrancheInput[] calldata tranches
    ) external;

    /**
     * @notice Draw funds from a tranche treasury.
     * @dev Only callable by the Gateway contract.
     *      Increments the tranche's `borrowed` counter on the Hub chain.
     *      Reverts if treasury liquidity (invested − borrowed) < amount.
     *      Emits Borrowed on success.
     * @param pool_id       The pool ID
     * @param chain_id      EVM chain ID of the chain where the vault is deployed
     * @param vault_address ERC-7540 vault contract address on that chain
     * @param borrower      EVM address of the institution initiating the borrow
     * @param amount        USDC amount to borrow (18-decimal U256)
     */
    function borrow(
        uint64 pool_id,
        uint64 chain_id,
        address vault_address,
        address borrower,
        uint256 amount
    ) external;

    /**
     * @notice Return funds to a tranche treasury.
     * @dev Only callable by the Gateway contract.
     *      Decrements the tranche's `borrowed` counter on the Hub chain.
     *      Saturates at zero — over-repayment does not revert.
     *      Emits Repaid on success.
     * @param pool_id       The pool ID
     * @param chain_id      EVM chain ID of the chain where the vault is deployed
     * @param vault_address ERC-7540 vault contract address on that chain
     * @param borrower      EVM address of the institution initiating the repay
     * @param amount        USDC amount being repaid (18-decimal U256)
     */
    function repay(
        uint64 pool_id,
        uint64 chain_id,
        address vault_address,
        address borrower,
        uint256 amount
    ) external;
}
