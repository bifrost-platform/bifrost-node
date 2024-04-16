use crate::{CouncilInstance, RelayExecutiveInstance, TechCommitteeInstance};

use pallet_evm_precompile_blake2::Blake2F;
use pallet_evm_precompile_bn128::{Bn128Add, Bn128Mul, Bn128Pairing};
use pallet_evm_precompile_modexp::Modexp;
use pallet_evm_precompile_simple::{ECRecover, Identity, Ripemd160, Sha256};

use precompile_balance::BalancePrecompile;
use precompile_bfc_offences::BfcOffencesPrecompile;
use precompile_bfc_staking::BfcStakingPrecompile;
use precompile_btc_registration_pool::BtcRegistrationPoolPrecompile;
use precompile_btc_socket_queue::BtcSocketQueuePrecompile;
use precompile_collective::CollectivePrecompile;
use precompile_governance::GovernancePrecompile;
use precompile_relay_manager::RelayManagerPrecompile;

use precompile_utils::precompile_set::*;

type EthereumPrecompilesChecks = (AcceptDelegateCall, CallableByContract, CallableByPrecompile);
type BifrostPrecompilesChecks = (CallableByContract, CallableByPrecompile);

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
	// BIFROST specific precompiles:
	PrecompileAt<AddressU64<256>, BtcRegistrationPoolPrecompile<R>, BifrostPrecompilesChecks>,
	PrecompileAt<AddressU64<257>, BtcSocketQueuePrecompile<R>, BifrostPrecompilesChecks>,
	PrecompileAt<AddressU64<1024>, BfcStakingPrecompile<R>, BifrostPrecompilesChecks>,
	PrecompileAt<AddressU64<1280>, BfcOffencesPrecompile<R>, BifrostPrecompilesChecks>,
	PrecompileAt<AddressU64<2048>, GovernancePrecompile<R>, BifrostPrecompilesChecks>,
	PrecompileAt<
		AddressU64<2049>,
		CollectivePrecompile<R, CouncilInstance>,
		BifrostPrecompilesChecks,
	>,
	PrecompileAt<
		AddressU64<2050>,
		CollectivePrecompile<R, TechCommitteeInstance>,
		BifrostPrecompilesChecks,
	>,
	PrecompileAt<
		AddressU64<2051>,
		CollectivePrecompile<R, RelayExecutiveInstance>,
		BifrostPrecompilesChecks,
	>,
	PrecompileAt<AddressU64<4096>, BalancePrecompile<R>, BifrostPrecompilesChecks>,
	PrecompileAt<AddressU64<8192>, RelayManagerPrecompile<R>, BifrostPrecompilesChecks>,
);

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
		PrecompilesInRangeInclusive<(AddressU64<1>, AddressU64<8192>), BifrostPrecompilesAt<R>>,
	),
>;
