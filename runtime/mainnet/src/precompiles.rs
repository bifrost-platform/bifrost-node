//! The BIFROST Network EVM precompiles. This can be compiled with `#[no_std]`, ready for Wasm.

use crate::{CouncilInstance, TechCommitteeInstance};

use codec::Decode;
use frame_support::dispatch::{Dispatchable, GetDispatchInfo, PostDispatchInfo};

use pallet_evm::{Context, Precompile, PrecompileResult, PrecompileSet};
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

use fp_evm::{ExitRevert, PrecompileFailure};

use sp_core::H160;
use sp_std::marker::PhantomData;

/// The PrecompileSet installed in the BIFROST runtime
#[derive(Debug, Clone, Copy)]
pub struct BifrostPrecompiles<R>(PhantomData<R>);

impl<R> BifrostPrecompiles<R> {
	pub fn new() -> Self {
		Self(Default::default())
	}

	pub fn used_addresses() -> impl Iterator<Item = H160> {
		sp_std::vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 1024, 1280, 2048, 2049, 2050, 4096, 8192]
			.into_iter()
			.map(|x| hash(x))
	}
}

impl<R> PrecompileSet for BifrostPrecompiles<R>
where
	BfcStakingPrecompile<R>: Precompile,
	BfcOffencesPrecompile<R>: Precompile,
	RelayManagerPrecompile<R>: Precompile,
	GovernancePrecompile<R>: Precompile,
	CollectivePrecompile<R, CouncilInstance>: Precompile,
	CollectivePrecompile<R, TechCommitteeInstance>: Precompile,
	BalancePrecompile<R>: Precompile,
	R: pallet_evm::Config,
	<R::Call as Dispatchable>::Origin: From<Option<R::AccountId>>,
	R::Call: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo + Decode,
{
	fn execute(
		&self,
		address: H160,
		input: &[u8],
		target_gas: Option<u64>,
		context: &Context,
		is_static: bool,
	) -> Option<PrecompileResult> {
		if self.is_precompile(address) && address > hash(9) && address != context.address {
			return Some(Err(PrecompileFailure::Revert {
				exit_status: ExitRevert::Reverted,
				output: "Cannot be called with DELEGATECALL or CALLCODE".into(),
				cost: 0,
			}))
		}
		match address {
			// Standard Ethereum precompiles
			a if a == hash(1) => Some(ECRecover::execute(input, target_gas, context, is_static)),
			a if a == hash(2) => Some(Sha256::execute(input, target_gas, context, is_static)),
			a if a == hash(3) => Some(Ripemd160::execute(input, target_gas, context, is_static)),
			a if a == hash(4) => Some(Identity::execute(input, target_gas, context, is_static)),
			a if a == hash(5) => Some(Modexp::execute(input, target_gas, context, is_static)),
			a if a == hash(6) => Some(Bn128Add::execute(input, target_gas, context, is_static)),
			a if a == hash(7) => Some(Bn128Mul::execute(input, target_gas, context, is_static)),
			a if a == hash(8) => Some(Bn128Pairing::execute(input, target_gas, context, is_static)),
			a if a == hash(9) => Some(Blake2F::execute(input, target_gas, context, is_static)),

			// BIFROST custom precompiles
			a if a == hash(1024) =>
				Some(BfcStakingPrecompile::<R>::execute(input, target_gas, context, is_static)),
			a if a == hash(1280) =>
				Some(BfcOffencesPrecompile::<R>::execute(input, target_gas, context, is_static)),
			a if a == hash(2048) =>
				Some(GovernancePrecompile::<R>::execute(input, target_gas, context, is_static)),
			a if a == hash(2049) => Some(CollectivePrecompile::<R, CouncilInstance>::execute(
				input, target_gas, context, is_static,
			)),
			a if a == hash(2050) =>
				Some(CollectivePrecompile::<R, TechCommitteeInstance>::execute(
					input, target_gas, context, is_static,
				)),
			a if a == hash(4096) =>
				Some(BalancePrecompile::<R>::execute(input, target_gas, context, is_static)),
			a if a == hash(8192) =>
				Some(RelayManagerPrecompile::<R>::execute(input, target_gas, context, is_static)),

			// Default
			_ => None,
		}
	}

	fn is_precompile(&self, address: H160) -> bool {
		Self::used_addresses().any(|x| x == address)
	}
}

fn hash(a: u64) -> H160 {
	H160::from_low_u64_be(a)
}
