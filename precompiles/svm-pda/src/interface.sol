// SPDX-License-Identifier: GPL-3.0-only
pragma solidity >=0.8.0;

/// @dev The SvmPda precompile address.
address constant SVM_PDA_ADDRESS = 0x0000000000000000000000000000000000000900;

/// @dev The SvmPda precompile instance.
ISvmPda constant SVM_PDA_CONTRACT = ISvmPda(SVM_PDA_ADDRESS);

/// @author The Bifrost Team
/// @title Solana program derived address (PDA) derivation
/// @notice Byte-for-byte Agave `Pubkey::find_program_address`, so the hub can predict a Solana
///         spoke's accounts (vault, share mint, multichain adapter) before the spoke creates them.
///         A pure function: no state, no caller or block dependence.
/// @custom:address 0x0000000000000000000000000000000000000900
interface ISvmPda {
    /// @dev Derive the PDA of `seeds` under `programId`:
    ///      for bump in 255 … 1, h = sha256(seeds[0] ‖ … ‖ seeds[n−1] ‖ [bump] ‖ programId ‖
    ///      "ProgramDerivedAddress"); the first h that is not a valid ed25519 point is the PDA.
    ///      Seeds are concatenated as-is (no length prefix), so integers in seeds must be
    ///      fixed-width (e.g. `abi.encodePacked(uint64)`, big-endian).
    ///      Reverts with more than 15 seeds, a seed longer than 32 bytes, or no viable bump.
    ///      Gas: 3,000 + (4,000 + 12 × ceil((Σ seed bytes + 54) / 32)) per bump tried (usually 1–8).
    /// @custom:selector 9d646a92
    /// @param programId The Solana program id — the raw 32-byte ed25519 public key (not a padded
    ///        EVM address).
    /// @param seeds Up to 15 seeds of up to 32 bytes each (an empty seed adds nothing).
    /// @return addr The PDA, 32 bytes.
    /// @return bump The bump seed found (255 … 1).
    function derive_pda(bytes32 programId, bytes[] calldata seeds)
        external
        view
        returns (bytes32 addr, uint8 bump);
}
