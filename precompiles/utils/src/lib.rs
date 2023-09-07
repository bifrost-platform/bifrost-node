#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

// Allows to use inside this crate `solidity::Codec` derive macro,which depends on
// `precompile_utils` being in the list of imported crates.
extern crate self as precompile_utils;

pub mod evm;
pub mod precompile_set;
pub mod substrate;

pub mod solidity;

pub use fp_evm::Precompile;
use fp_evm::PrecompileFailure;
pub use precompile_utils_macro::{keccak256, precompile, precompile_name_from_address};

/// Alias for Result returning an EVM precompile error.
pub type EvmResult<T = ()> = Result<T, PrecompileFailure>;

pub mod prelude {
	pub use {
		crate::{
			evm::{
				handle::PrecompileHandleExt,
				logs::{log0, log1, log2, log3, log4, LogExt},
			},
			precompile_set::DiscriminantResult,
			solidity::{
				// We export solidity itself to encourage using `solidity::Codec` to avoid
				// confusion with parity_scale_codec,
				self,
				codec::{
					Address,
					BoundedBytes,
					BoundedString,
					BoundedVec,
					// Allow usage of Codec methods while not exporting the name directly.
					// Codec as _,
					Convert,
					UnboundedBytes,
					UnboundedString,
				},
				revert::{
					revert, BacktraceExt, InjectBacktrace, MayRevert, Revert, RevertExt,
					RevertReason,
				},
			},
			substrate::{RuntimeHelper, TryDispatchError},
			EvmResult,
		},
		alloc::string::String,
		pallet_evm::{PrecompileHandle, PrecompileOutput},
		precompile_utils_macro::{keccak256, precompile},
	};
}
