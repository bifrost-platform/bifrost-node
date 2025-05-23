//! Helper methods for computing issuance based on inflation

use crate::{pallet::pallet::Config, BalanceOf, Round};

use serde::{Deserialize, Serialize};

use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

use frame_support::traits::Currency;

use sp_runtime::{PerThing, Perbill, RuntimeDebug};

use substrate_fixed::{transcendental::pow as floatpow, types::I64F64};

const SECONDS_PER_YEAR: u32 = 31557600;
const SECONDS_PER_BLOCK: u32 = 3;
pub const BLOCKS_PER_YEAR: u32 = SECONDS_PER_YEAR / SECONDS_PER_BLOCK;

pub fn rounds_per_year<T: Config>() -> u32 {
	let blocks_per_round = <Round<T>>::get().round_length;
	BLOCKS_PER_YEAR / blocks_per_round
}

#[derive(
	Eq,
	PartialEq,
	Clone,
	Copy,
	Encode,
	Decode,
	Default,
	RuntimeDebug,
	MaxEncodedLen,
	TypeInfo,
	Serialize,
	Deserialize,
)]
/// A data structure that represents a certain range in three possible values
pub struct Range<T> {
	/// The minimum value
	pub min: T,
	/// The ideal value
	pub ideal: T,
	/// The maximum value
	pub max: T,
}

impl<T: Ord> Range<T> {
	pub fn is_valid(&self) -> bool {
		self.max >= self.ideal && self.ideal >= self.min
	}
}

impl<T: Ord + Copy> From<T> for Range<T> {
	fn from(other: T) -> Range<T> {
		Range { min: other, ideal: other, max: other }
	}
}

/// Convert an annual inflation to a round inflation
/// round = (1+annual)^(1/rounds_per_year) - 1
pub fn perbill_annual_to_perbill_round(
	annual: Range<Perbill>,
	rounds_per_year: u32,
) -> Range<Perbill> {
	let exponent = I64F64::from_num(1) / I64F64::from_num(rounds_per_year);
	let annual_to_round = |annual: Perbill| -> Perbill {
		let x = I64F64::from_num(annual.deconstruct()) / I64F64::from_num(Perbill::ACCURACY);
		let y: I64F64 = floatpow(I64F64::from_num(1) + x, exponent)
			.expect("Cannot overflow since rounds_per_year is u32 so worst case 0; QED");
		Perbill::from_parts(
			((y - I64F64::from_num(1)) * I64F64::from_num(Perbill::ACCURACY))
				.ceil()
				.to_num::<u32>(),
		)
	};
	let round = Range {
		min: annual_to_round(annual.min),
		ideal: annual_to_round(annual.ideal),
		max: annual_to_round(annual.max),
	};
	round
}

/// Convert annual inflation rate range to round inflation range
pub fn annual_to_round<T: Config>(annual: Range<Perbill>) -> Range<Perbill> {
	let periods = rounds_per_year::<T>();
	perbill_annual_to_perbill_round(annual, periods)
}

/// Compute round issuance range from round inflation range and current total issuance
pub fn round_issuance_range<T: Config>(round_inflation: Range<Perbill>) -> Range<BalanceOf<T>> {
	// total issued native token amount
	let circulating = T::Currency::total_issuance();
	Range {
		min: round_inflation.min * circulating,
		ideal: round_inflation.ideal * circulating,
		max: round_inflation.max * circulating,
	}
}

#[derive(
	Eq,
	PartialEq,
	Clone,
	Encode,
	Decode,
	Default,
	RuntimeDebug,
	TypeInfo,
	MaxEncodedLen,
	Serialize,
	Deserialize,
)]
/// The information about the staking inflation for this network
pub struct InflationInfo<Balance> {
	/// Staking expectations
	pub expect: Range<Balance>,
	/// Annual inflation range
	pub annual: Range<Perbill>,
	/// Round inflation range
	pub round: Range<Perbill>,
}

impl<Balance> InflationInfo<Balance> {
	pub fn new<T: Config>(
		annual: Range<Perbill>,
		expect: Range<Balance>,
	) -> InflationInfo<Balance> {
		InflationInfo { expect, annual, round: annual_to_round::<T>(annual) }
	}

	/// Set round inflation range according to input annual inflation range
	pub fn set_round_from_annual<T: Config>(&mut self, new: Range<Perbill>) {
		self.round = annual_to_round::<T>(new);
	}

	/// Reset round inflation rate based on changes to round length
	pub fn reset_round(&mut self, new_length: u32) {
		let periods = BLOCKS_PER_YEAR / new_length;
		self.round = perbill_annual_to_perbill_round(self.annual, periods);
	}

	/// Set staking expectations
	pub fn set_expectations(&mut self, expect: Range<Balance>) {
		self.expect = expect;
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn mock_annual_to_round(annual: Range<Perbill>, rounds_per_year: u32) -> Range<Perbill> {
		perbill_annual_to_perbill_round(annual, rounds_per_year)
	}

	fn mock_round_issuance_range(
		// Total circulating before minting
		circulating: u128,
		// Round inflation range
		round: Range<Perbill>,
	) -> Range<u128> {
		Range {
			min: round.min * circulating,
			ideal: round.ideal * circulating,
			max: round.max * circulating,
		}
	}

	#[test]
	#[ignore]
	fn simple_issuance_conversion() {
		// 5% inflation for 10_000_0000 = 500,000 minted over the year
		// let's assume there are 10 periods in a year
		// => mint 500_000 over 10 periods => 50_000 minted per period
		let expected_round_issuance_range: Range<u128> =
			Range { min: 48_909, ideal: 48_909, max: 48_909 };
		let schedule = Range {
			min: Perbill::from_percent(5),
			ideal: Perbill::from_percent(5),
			max: Perbill::from_percent(5),
		};
		assert_eq!(
			expected_round_issuance_range,
			mock_round_issuance_range(10_000_000, mock_annual_to_round(schedule, 10))
		);
	}

	#[test]
	#[ignore]
	fn range_issuance_conversion() {
		// 3-5% inflation for 10_000_0000 = 300_000-500,000 minted over the year
		// let's assume there are 10 periods in a year
		// => mint 300_000-500_000 over 10 periods => 30_000-50_000 minted per period
		let expected_round_issuance_range: Range<u128> =
			Range { min: 29_603, ideal: 39298, max: 48_909 };
		let schedule = Range {
			min: Perbill::from_percent(3),
			ideal: Perbill::from_percent(4),
			max: Perbill::from_percent(5),
		};
		assert_eq!(
			expected_round_issuance_range,
			mock_round_issuance_range(10_000_000, mock_annual_to_round(schedule, 10))
		);
	}

	#[test]
	#[ignore]
	fn expected_parameterization() {
		let expected_round_schedule: Range<u128> = Range { min: 45, ideal: 56, max: 56 };
		let schedule = Range {
			min: Perbill::from_percent(4),
			ideal: Perbill::from_percent(5),
			max: Perbill::from_percent(5),
		};
		assert_eq!(
			expected_round_schedule,
			mock_round_issuance_range(10_000_000, mock_annual_to_round(schedule, 8766))
		);
	}
}
