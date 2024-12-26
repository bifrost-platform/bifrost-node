#![cfg_attr(not(feature = "std"), no_std)]

pub use bifrost_common_constants::{currency, time};

pub mod fee {
	use frame_support::weights::constants::WEIGHT_REF_TIME_PER_SECOND;

	/// Current approximation of the gas/s consumption considering
	/// EVM execution over compiled WASM (on 4.4Ghz CPU).
	/// Given the 500ms Weight, from which 75% only are used for transactions,
	/// the total EVM execution gas limit is: GAS_PER_SECOND * 0.500 * 0.75 ~= 60_000_000.
	pub const GAS_PER_SECOND: u64 = 160_000_000;

	/// Approximate ratio of the amount of Weight per Gas.
	/// u64 works for approximations because Weight is a very small unit compared to gas.
	pub const WEIGHT_PER_GAS: u64 = WEIGHT_REF_TIME_PER_SECOND / GAS_PER_SECOND;
}
