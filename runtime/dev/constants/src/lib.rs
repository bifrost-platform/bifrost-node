pub use bifrost_common_constants::{currency, time};

pub mod fee {
	use frame_support::weights::{constants::WEIGHT_PER_SECOND, Weight};

	/// Current approximation of the gas/s consumption considering
	/// EVM execution over compiled WASM (on 4.4Ghz CPU).
	/// Given the 500ms Weight, from which 75% only are used for transactions,
	/// the total EVM execution gas limit is: GAS_PER_SECOND * 0.500 * 0.75 ~= 50_000_000.
	pub const GAS_PER_SECOND: u64 = 133_333_333;

	/// Approximate ratio of the amount of Weight per Gas.
	/// u64 works for approximations because Weight is a very small unit compared to gas.
	pub const WEIGHT_PER_GAS: u64 = WEIGHT_PER_SECOND / GAS_PER_SECOND;

	pub struct BifrostGasWeightMapping;
	impl pallet_evm::GasWeightMapping for BifrostGasWeightMapping {
		fn gas_to_weight(gas: u64) -> Weight {
			gas.saturating_mul(WEIGHT_PER_GAS)
		}
		fn weight_to_gas(weight: Weight) -> u64 {
			u64::try_from(weight.wrapping_div(WEIGHT_PER_GAS)).unwrap_or(u32::MAX as u64)
		}
	}
}
