//! Runtime-level storage migrations shared by the Bifrost runtimes.

use frame_support::{
	traits::{Get, OnRuntimeUpgrade},
	weights::Weight,
};
use sp_core::H160;
use sp_std::{marker::PhantomData, vec::Vec};

/// The dummy code every precompile address gets at genesis: `PUSH1 0 PUSH1 0 REVERT`.
/// Solidity checks `extcodesize` before a high-level call, so a precompile address
/// with no code can only be reached by a raw call or an EOA transaction.
pub const PRECOMPILE_CODE: [u8; 5] = [0x60, 0x00, 0x60, 0x00, 0xFD];

/// Gives every address in `Addresses` (a runtime's precompile set) the same dummy
/// code genesis gives it, if it has none yet. Precompiles added to an already
/// running chain otherwise have no code. Idempotent — an address that already has
/// code is left untouched — so it can stay wired in across upgrades.
pub struct EnsurePrecompileCode<Runtime, Addresses>(PhantomData<(Runtime, Addresses)>);

impl<Runtime, Addresses> OnRuntimeUpgrade for EnsurePrecompileCode<Runtime, Addresses>
where
	Runtime: pallet_evm::Config,
	Addresses: Get<Vec<H160>>,
{
	fn on_runtime_upgrade() -> Weight {
		let addresses = Addresses::get();
		let mut inserted = 0u64;
		for address in addresses.iter() {
			if !pallet_evm::AccountCodes::<Runtime>::contains_key(address) {
				// Same path genesis takes for `GenesisAccount { code, .. }`.
				let _ = pallet_evm::Pallet::<Runtime>::create_account(
					*address,
					PRECOMPILE_CODE.to_vec(),
					None,
				);
				inserted += 1;
			}
		}
		if inserted > 0 {
			log::info!(
				target: "runtime::precompiles",
				"EnsurePrecompileCode: inserted code at {} precompile address(es)",
				inserted
			);
		}
		// Per inserted address: account sufficients, code metadata, code.
		<Runtime as frame_system::Config>::DbWeight::get()
			.reads_writes(addresses.len() as u64 + inserted, inserted.saturating_mul(3))
	}
}
