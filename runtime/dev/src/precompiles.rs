use crate::{CouncilInstance, TechCommitteeInstance};

use pallet_evm_precompile_blake2::Blake2F;
use pallet_evm_precompile_bn128::{Bn128Add, Bn128Mul, Bn128Pairing};
use pallet_evm_precompile_modexp::Modexp;
use pallet_evm_precompile_simple::{ECRecover, Identity, Ripemd160, Sha256};

use precompile_balance::BalancePrecompile;
use precompile_bfc_offences::BfcOffencesPrecompile;
use precompile_bfc_staking::BfcStakingPrecompile;
use precompile_collective::CollectivePrecompile;
use precompile_governance::GovernancePrecompile;
use precompile_relay_manager::RelayManagerPrecompile;

use precompile_utils::precompile_set::*;

/// The PrecompileSet installed in the BIFROST runtime.
/// We include the nine Istanbul precompiles
/// (https://github.com/ethereum/go-ethereum/blob/3c46f557/core/vm/contracts.go#L69)
/// as well as a special precompile for dispatching Substrate extrinsics
/// The following distribution has been decided for the precompiles
/// 0-1023: Ethereum Mainnet Precompiles
/// 1024-8192 BIFROST Mainnet specific precompiles
pub type BifrostPrecompiles<R> = PrecompileSetBuilder<
	R,
	(
		// Skip precompiles if out of range.
		PrecompilesInRangeInclusive<
			(AddressU64<1>, AddressU64<8192>),
			(
				// Ethereum precompiles:
				// We allow DELEGATECALL to stay compliant with Ethereum behavior.
				PrecompileAt<AddressU64<1>, ECRecover, ForbidRecursion, AllowDelegateCall>,
				PrecompileAt<AddressU64<2>, Sha256, ForbidRecursion, AllowDelegateCall>,
				PrecompileAt<AddressU64<3>, Ripemd160, ForbidRecursion, AllowDelegateCall>,
				PrecompileAt<AddressU64<4>, Identity, ForbidRecursion, AllowDelegateCall>,
				PrecompileAt<AddressU64<5>, Modexp, ForbidRecursion, AllowDelegateCall>,
				PrecompileAt<AddressU64<6>, Bn128Add, ForbidRecursion, AllowDelegateCall>,
				PrecompileAt<AddressU64<7>, Bn128Mul, ForbidRecursion, AllowDelegateCall>,
				PrecompileAt<AddressU64<8>, Bn128Pairing, ForbidRecursion, AllowDelegateCall>,
				PrecompileAt<AddressU64<9>, Blake2F, ForbidRecursion, AllowDelegateCall>,
				// BIFROST specific precompiles:
				PrecompileAt<AddressU64<1024>, BfcStakingPrecompile<R>>,
				PrecompileAt<AddressU64<1280>, BfcOffencesPrecompile<R>>,
				PrecompileAt<AddressU64<2048>, GovernancePrecompile<R>>,
				PrecompileAt<AddressU64<2049>, CollectivePrecompile<R, CouncilInstance>>,
				PrecompileAt<AddressU64<2050>, CollectivePrecompile<R, TechCommitteeInstance>>,
				PrecompileAt<AddressU64<4096>, BalancePrecompile<R>>,
				PrecompileAt<AddressU64<8192>, RelayManagerPrecompile<R>>,
			),
		>,
	),
>;
