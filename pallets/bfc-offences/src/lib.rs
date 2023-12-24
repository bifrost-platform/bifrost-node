#![cfg_attr(not(feature = "std"), no_std)]

pub mod migrations;
mod pallet;
pub mod weights;

pub use pallet::pallet::*;
use weights::WeightInfo;

use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

use frame_support::{pallet_prelude::MaxEncodedLen, traits::Currency};

use bp_staking::{Offence, RoundIndex};
use sp_runtime::{traits::Zero, Perbill, RuntimeDebug};
use sp_staking::SessionIndex;
use sp_std::prelude::*;

/// The type that indicates the count of offences
pub type OffenceCount = u32;

pub type BalanceOf<T> =
	<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

pub type NegativeImbalanceOf<T> = <<T as Config>::Currency as Currency<
	<T as frame_system::Config>::AccountId,
>>::NegativeImbalance;

pub(crate) const LOG_TARGET: &'static str = "runtime::bfc-offences";

// syntactic sugar for logging.
#[macro_export]
macro_rules! log {
	($level:tt, $patter:expr $(, $values:expr)* $(,)?) => {
		log::$level!(
			target: crate::LOG_TARGET,
			concat!("[{:?}] 💸 ", $patter), <frame_system::Pallet<T>>::block_number() $(, $values)*
		)
	};
}

/// Used for release versioning upto v2_0_0.
///
/// Obsolete from v3. Keeping around to make encoding/decoding of old migration code easier.
#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
/// A value that represents the current storage version of this pallet.
///
/// This value is used by the `on_runtime_upgrade` logic to determine whether we run storage
/// migration.
enum Releases {
	V1_0_0,
	V2_0_0,
}

impl Default for Releases {
	fn default() -> Self {
		Releases::V2_0_0
	}
}

#[derive(Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
/// The offence information for each validator
pub struct ValidatorOffenceInfo<Balance> {
	/// The latest offence round a validator earned
	pub latest_offence_round_index: RoundIndex,
	/// The latest offence session a validator earned
	pub latest_offence_session_index: SessionIndex,
	/// The total offences a validator have earned
	pub offence_count: OffenceCount,
	/// The aggregated slash fraction
	pub aggregated_slash_fraction: Perbill,
	/// The detail of offences this validator have earned
	pub offences: Vec<Offence<Balance>>,
}

impl<
		Balance: Copy
			+ Zero
			+ PartialOrd
			+ sp_std::ops::AddAssign
			+ sp_std::ops::SubAssign
			+ sp_std::ops::Sub<Output = Balance>
			+ sp_std::fmt::Debug,
	> ValidatorOffenceInfo<Balance>
{
	pub fn new(offence: Offence<Balance>) -> Self {
		ValidatorOffenceInfo {
			latest_offence_round_index: offence.round_index,
			latest_offence_session_index: offence.session_index,
			offence_count: 1u32,
			aggregated_slash_fraction: offence.slash_fraction,
			offences: vec![offence],
		}
	}

	fn set_latest_offence(&mut self, round_index: RoundIndex, session_index: SessionIndex) {
		self.latest_offence_round_index = round_index;
		self.latest_offence_session_index = session_index;
	}

	fn increase_offence_count(&mut self) {
		self.offence_count += 1;
	}

	fn increase_slash_fraction(&mut self, slash_fraction: Perbill) {
		let old = self.aggregated_slash_fraction.deconstruct();
		let new = Perbill::from_parts(old + slash_fraction.deconstruct());
		self.aggregated_slash_fraction = new;
	}

	/// Increase offence related field
	///
	/// 1. Set offence round & session index
	/// 2. Increase offence count
	/// 3. Increase aggregated slash fraction
	/// 4. Push offence to offences vector
	pub fn add_offence(&mut self, offence: Offence<Balance>) {
		self.set_latest_offence(offence.round_index, offence.session_index);
		self.increase_offence_count();
		self.increase_slash_fraction(offence.slash_fraction);
		self.offences.push(offence);
	}
}
