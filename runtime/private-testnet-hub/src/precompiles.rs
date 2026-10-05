use crate::{CouncilInstance, RelayExecutiveInstance, TechCommitteeInstance};

use pallet_evm_precompile_blake2::Blake2F;
use pallet_evm_precompile_bn128::{Bn128Add, Bn128Mul, Bn128Pairing};
use pallet_evm_precompile_modexp::Modexp;
use pallet_evm_precompile_simple::{ECRecover, Identity, Ripemd160, Sha256};

use precompile_balance::BalancePrecompile;
use precompile_bfc_offences::BfcOffencesPrecompile;
use precompile_bfc_staking::BfcStakingPrecompile;
use precompile_bifrost_evm_tx_payment::BifrostTransactionPaymentPrecompile;
use precompile_blaze::BlazePrecompile;
use precompile_btc_registration_pool::BtcRegistrationPoolPrecompile;
use precompile_btc_socket_queue::BtcSocketQueuePrecompile;
use precompile_collective::CollectivePrecompile;
use precompile_governance::GovernancePrecompile;
use precompile_relay_manager::RelayManagerPrecompile;
use precompile_tranche_custom_flows::TrancheCustomFlowsPrecompile;
use precompile_tranche_investments::TrancheInvestmentsPrecompile;
use precompile_tranche_investments_v2::TrancheInvestmentsV2Precompile;
use precompile_tranche_permissions::TranchePermissionsPrecompile;
use precompile_tranche_system::TrancheSystemPrecompile;
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
	PrecompileAt<AddressU64<258>, BlazePrecompile<R>, BifrostPrecompilesChecks>,
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
	// Bifrost Transaction Payment Precompile at 0x0810 (2064)
	PrecompileAt<
		AddressU64<2064>,
		BifrostTransactionPaymentPrecompile<R>,
		BifrostPrecompilesChecks,
	>,
	PrecompileAt<AddressU64<512>, TrancheSystemPrecompile<R>, BifrostPrecompilesChecks>,
	PrecompileAt<AddressU64<513>, TrancheInvestmentsPrecompile<R>, BifrostPrecompilesChecks>,
	PrecompileAt<
		AddressU64<514>,
		TranchePermissionsPrecompile<R>,
		TranchePermissionsPrecompilesChecks,
	>,
	PrecompileAt<AddressU64<515>, TrancheTxRegistryPrecompile<R>, BifrostPrecompilesChecks>,
	PrecompileAt<AddressU64<516>, TrancheCustomFlowsPrecompile<R>, BifrostPrecompilesChecks>,
	// v2 — a new 0x0300 block rather than continuing the 0x0200 tranche block (a fresh,
	// independent address range for pallets with their own storage/extrinsics, not an
	// addition to the existing v1 contracts) — laid out with the *same offsets as v1*
	// for whichever slot each v2 pallet corresponds to (system=+0, investments=+1,
	// permissions=+2, tx-registry=+3, custom-flows=+4), so only investments and
	// tx-registry (the two actually forked into v1/v2 — see
	// docs/tranche-tx-registry/settlement-leg-chunking-design.md) occupy a slot here;
	// 0x0300 (system's slot), 0x0302 (permissions' — freed when permissions was
	// un-forked back to the shared pallet) and 0x0304 (custom-flows') stay unused,
	// since none of those three are forked.
	PrecompileAt<AddressU64<769>, TrancheInvestmentsV2Precompile<R>, BifrostPrecompilesChecks>,
	PrecompileAt<AddressU64<771>, TrancheTxRegistryV2Precompile<R>, BifrostPrecompilesChecks>,
);

type BifrostPrecompilesInner<R> = PrecompileSetBuilder<
	R,
	(PrecompilesInRangeInclusive<(AddressU64<1>, AddressU64<8192>), BifrostPrecompilesAt<R>>,),
>;

bifrost_common_runtime::impl_bifrost_precompiles!(crate::Runtime, BifrostPrecompilesInner);
