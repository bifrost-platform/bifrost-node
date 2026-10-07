use pallet_evm_precompile_blake2::Blake2F;
use pallet_evm_precompile_bn128::{Bn128Add, Bn128Mul, Bn128Pairing};
use pallet_evm_precompile_modexp::Modexp;
use pallet_evm_precompile_simple::{ECRecover, Identity, Ripemd160, Sha256};

use precompile_permissioned_authority::PermissionedAuthorityPrecompile;
use precompile_relay_manager::RelayManagerPrecompile;
use precompile_tranche_custom_flows::{
	TrancheCustomFlowsPrecompile, TrancheCustomFlowsUniversalPrecompile,
};
use precompile_tranche_investments::TrancheInvestmentsPrecompile;
use precompile_tranche_investments_v2::TrancheInvestmentsV2Precompile;
use precompile_tranche_permissions::{
	TranchePermissionsPrecompile, TranchePermissionsUniversalPrecompile,
};
use precompile_tranche_system::{TrancheSystemPrecompile, TrancheSystemUniversalPrecompile};
use precompile_tranche_tx_registry::TrancheTxRegistryPrecompile;
use precompile_tranche_tx_registry_v2::TrancheTxRegistryV2Precompile;

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
/// (`authority_address`/`relayer_manager_address` in its config), and the OmniFi tranche-*
/// precompiles. Every precompile keeps the exact address it has on the public Bifrost chains
/// (see `runtime/dev/src/precompiles.rs`), so contracts, SDKs, the relayer and the recorder
/// bot work unchanged against this chain.
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
	PrecompileAt<AddressU64<8192>, RelayManagerPrecompile<R>, BifrostPrecompilesChecks>,
	// OmniFi tranche-* precompiles (v1 block at 0x0200):
	PrecompileAt<AddressU64<512>, TrancheSystemPrecompile<R>, BifrostPrecompilesChecks>,
	PrecompileAt<AddressU64<513>, TrancheInvestmentsPrecompile<R>, BifrostPrecompilesChecks>,
	PrecompileAt<
		AddressU64<514>,
		TranchePermissionsPrecompile<R>,
		TranchePermissionsPrecompilesChecks,
	>,
	PrecompileAt<AddressU64<515>, TrancheTxRegistryPrecompile<R>, BifrostPrecompilesChecks>,
	PrecompileAt<AddressU64<516>, TrancheCustomFlowsPrecompile<R>, BifrostPrecompilesChecks>,
	// OmniFi tranche-* v2 precompiles (0x0300 block, same per-pallet offsets as v1):
	PrecompileAt<AddressU64<769>, TrancheInvestmentsV2Precompile<R>, BifrostPrecompilesChecks>,
	PrecompileAt<AddressU64<771>, TrancheTxRegistryV2Precompile<R>, BifrostPrecompilesChecks>,
	// Non-EVM-compatible ("universal") interfaces of tranche-system/permissions/custom-flows
	// — the 0x0600 block, same per-pallet offsets as the EVM-only 0x0200 block. Same
	// storage; spoke-chain addresses are `bytes32` and foreign tx hashes `bytes` here,
	// while the 0x0200 precompiles keep their original EVM-only ABI.
	PrecompileAt<AddressU64<1536>, TrancheSystemUniversalPrecompile<R>, BifrostPrecompilesChecks>,
	PrecompileAt<
		AddressU64<1538>,
		TranchePermissionsUniversalPrecompile<R>,
		TranchePermissionsPrecompilesChecks,
	>,
	PrecompileAt<
		AddressU64<1540>,
		TrancheCustomFlowsUniversalPrecompile<R>,
		BifrostPrecompilesChecks,
	>,
);

type BifrostPrecompilesInner<R> = PrecompileSetBuilder<
	R,
	(PrecompilesInRangeInclusive<(AddressU64<1>, AddressU64<8192>), BifrostPrecompilesAt<R>>,),
>;

bifrost_common_runtime::impl_bifrost_precompiles!(crate::Runtime, BifrostPrecompilesInner);
