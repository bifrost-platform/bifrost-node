use pallet_evm_precompile_blake2::Blake2F;
use pallet_evm_precompile_bn128::{Bn128Add, Bn128Mul, Bn128Pairing};
use pallet_evm_precompile_modexp::Modexp;
use pallet_evm_precompile_simple::{ECRecover, Identity, Ripemd160, Sha256};

use precompile_permissioned_authority::PermissionedAuthorityPrecompile;
use precompile_relay_manager::RelayManagerPrecompile;
use precompile_svm_pda::SvmPdaPrecompile;
use precompile_tranche_investments_v2::TrancheInvestmentsV2Precompile;
use precompile_tranche_permissions::TranchePermissionsUniversalPrecompile;
use precompile_tranche_system::TrancheSystemUniversalPrecompile;
use precompile_tranche_tx_registry_v3::TrancheTxRegistryV3Precompile;

use precompile_utils::precompile_set::*;

type EthereumPrecompilesChecks = (AcceptDelegateCall, CallableByContract, CallableByPrecompile);
type BifrostPrecompilesChecks = (CallableByContract, CallableByPrecompile);
// TranchePermissions calls out to the Hub-chain Orchestrator contract
// (Orchestrator.sendWhitelist) as a subcall when propagating a TrancheInvestor
// grant/revoke to a Spoke chain — needs `SubcallWithMaxNesting` or
// `handle.call` is rejected before it ever reaches Orchestrator.
type TranchePermissionsPrecompilesChecks =
	(CallableByContract, CallableByPrecompile, SubcallWithMaxNesting<0>);

/// The Ethereum precompiles, the authority/relay-manager precompiles the CCCP relayer reads
/// (`authority_address`/`relayer_manager_address` in its config), and the **latest** OmniFi
/// tranche-* precompiles only — the ones multichain-v3 contracts call: tranche-system and
/// tranche-permissions universal (0x0600/0x0602), investments v2 (0x0301) and tx-registry v3
/// (0x0703). Each keeps the address it has on the public Bifrost chains (see
/// `runtime/dev/src/precompiles.rs`); the v1 0x0200 block, v2 tx-registry and custom-flows are
/// not on this chain.
#[precompile_utils::precompile_name_from_address]
pub type BifrostPrecompilesAt<R> = (
	// Ethereum precompiles:
	// We allow DELEGATECALL to stay compliant with Ethereum behavior.
	PrecompileAt<AddressU64<1>, ECRecover, EthereumPrecompilesChecks>,
	PrecompileAt<AddressU64<2>, Sha256, EthereumPrecompilesChecks>,
	PrecompileAt<AddressU64<3>, Ripemd160, EthereumPrecompilesChecks>,
	PrecompileAt<AddressU64<4>, Identity, EthereumPrecompilesChecks>,
	PrecompileAt<AddressU64<5>, Modexp, EthereumPrecompilesChecks>,
	PrecompileAt<AddressU64<6>, Bn128Add, EthereumPrecompilesChecks>,
	PrecompileAt<AddressU64<7>, Bn128Mul, EthereumPrecompilesChecks>,
	PrecompileAt<AddressU64<8>, Bn128Pairing, EthereumPrecompilesChecks>,
	PrecompileAt<AddressU64<9>, Blake2F, EthereumPrecompilesChecks>,
	// BIFROST specific precompiles used by the CCCP relayer. 0x0400 is the `bfc-staking`
	// precompile's address on the public chains; here it serves the same `round_info()` /
	// `latest_round()` ABI from `PermissionedAuthority`.
	PrecompileAt<AddressU64<1024>, PermissionedAuthorityPrecompile<R>, BifrostPrecompilesChecks>,
	// Solana PDA derivation (Agave `find_program_address`) at 0x0900 — a stateless view the hub
	// uses to predict a Solana spoke's accounts at product creation.
	PrecompileAt<AddressU64<2304>, SvmPdaPrecompile, BifrostPrecompilesChecks>,
	PrecompileAt<AddressU64<8192>, RelayManagerPrecompile<R>, BifrostPrecompilesChecks>,
	// OmniFi investments v2 (0x0300 block, investments offset +1):
	PrecompileAt<AddressU64<769>, TrancheInvestmentsV2Precompile<R>, BifrostPrecompilesChecks>,
	// v3 tx-registry — a new 0x0700 block at the tx-registry offset (+3); universal
	// (bytes32 address / bytes tx hash) ABI only. See docs/tranche-tx-registry-v3/design.md.
	PrecompileAt<AddressU64<1795>, TrancheTxRegistryV3Precompile<R>, BifrostPrecompilesChecks>,
	// "Universal" (bytes32 address / bytes tx hash) interfaces of tranche-system and
	// tranche-permissions — the 0x0600 block.
	PrecompileAt<AddressU64<1536>, TrancheSystemUniversalPrecompile<R>, BifrostPrecompilesChecks>,
	PrecompileAt<
		AddressU64<1538>,
		TranchePermissionsUniversalPrecompile<R>,
		TranchePermissionsPrecompilesChecks,
	>,
);

type BifrostPrecompilesInner<R> = PrecompileSetBuilder<
	R,
	(PrecompilesInRangeInclusive<(AddressU64<1>, AddressU64<8192>), BifrostPrecompilesAt<R>>,),
>;

bifrost_common_runtime::impl_bifrost_precompiles!(crate::Runtime, BifrostPrecompilesInner);
