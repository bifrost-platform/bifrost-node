pub mod traits;

use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

use frame_support::pallet_prelude::MaxEncodedLen;
use sp_runtime::{traits::Zero, Perbill, RuntimeDebug};
use sp_staking::SessionIndex;

/// The type that indicates the index of a round
pub type RoundIndex = u32;

/// The maximum authorities allowed
pub const MAX_AUTHORITIES: u32 = 1_000;

#[derive(
	Eq,
	PartialEq,
	Ord,
	PartialOrd,
	Encode,
	Decode,
	Clone,
	Copy,
	RuntimeDebug,
	TypeInfo,
	MaxEncodedLen,
)]
/// The tier type of a validator node.
pub enum TierType {
	/// The validator node must operate cross-chain functionality with a running relayer
	Full,
	/// The validator node without cross-chain functionality
	Basic,
	/// The type that references to both full and basic for filtering
	All,
}

impl Default for TierType {
	fn default() -> Self {
		TierType::Basic
	}
}

#[derive(Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
/// The detailed information of offences that a specific validator earned
pub struct Offence<Balance> {
	/// The round index this offence happened
	pub round_index: RoundIndex,
	/// The session index this offence happened
	pub session_index: SessionIndex,
	/// The current self-bond of the offender
	pub self_bond: Balance,
	/// The total slash amount (self-bond + nominations) this offence holds
	pub total_slash: Balance,
	/// The self-bond slash amount this offence holds
	pub offender_slash: Balance,
	/// The total nomination slash amount this offence holds
	pub nominators_slash: Balance,
	/// The slash fraction this offence holds
	pub slash_fraction: Perbill,
}

impl<
		Balance: Copy
			+ Zero
			+ PartialOrd
			+ sp_std::ops::AddAssign
			+ sp_std::ops::SubAssign
			+ sp_std::ops::Sub<Output = Balance>
			+ sp_std::fmt::Debug,
	> Offence<Balance>
{
	pub fn new(
		round_index: RoundIndex,
		session_index: SessionIndex,
		self_bond: Balance,
		total_slash: Balance,
		offender_slash: Balance,
		nominators_slash: Balance,
		slash_fraction: Perbill,
	) -> Self {
		Offence {
			round_index,
			session_index,
			self_bond,
			total_slash,
			offender_slash,
			nominators_slash,
			slash_fraction,
		}
	}
}
