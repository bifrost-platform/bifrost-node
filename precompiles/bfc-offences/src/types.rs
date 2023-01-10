use frame_support::traits::Currency;

use pallet_bfc_offences::{OffenceCount, ValidatorOffenceInfo};
use precompile_utils::prelude::Address;

use bp_staking::RoundIndex;
use sp_core::H160;
use sp_staking::SessionIndex;
use sp_std::{marker::PhantomData, vec, vec::Vec};

pub type BalanceOf<Runtime> = <<Runtime as pallet_bfc_offences::Config>::Currency as Currency<
	<Runtime as frame_system::Config>::AccountId,
>>::Balance;

pub type OffencesOf<Runtime> = pallet_bfc_offences::Pallet<Runtime>;

pub type EvmValidatorOffenceOf = (Address, RoundIndex, SessionIndex, OffenceCount);

pub type EvmValidatorOffencesOf =
	(Vec<Address>, Vec<RoundIndex>, Vec<SessionIndex>, Vec<OffenceCount>);

/// EVM struct for validator offence
pub struct ValidatorOffence<Runtime: pallet_bfc_offences::Config> {
	/// The address of this validator
	pub validator: Address,
	/// The latest round this validator earned an offence
	pub latest_offence_round_index: RoundIndex,
	/// The latest session this validator earned an offence
	pub latest_offence_session_index: RoundIndex,
	/// The total offences this validator earned
	pub offence_count: OffenceCount,
	/// A zero-sized type used to mark things that "act like" they own a T.
	phantom: PhantomData<Runtime>,
}

impl<Runtime> ValidatorOffence<Runtime>
where
	Runtime: pallet_bfc_offences::Config,
	Runtime::AccountId: Into<H160>,
{
	pub fn default() -> Self {
		ValidatorOffence {
			validator: Address(Default::default()),
			latest_offence_round_index: 0u32,
			latest_offence_session_index: 0u32,
			offence_count: 0u32,
			phantom: PhantomData,
		}
	}

	pub fn set_offence(
		&mut self,
		validator: Runtime::AccountId,
		offence: ValidatorOffenceInfo<BalanceOf<Runtime>>,
	) {
		self.validator = Address(validator.into());
		self.latest_offence_round_index = offence.latest_offence_round_index;
		self.latest_offence_session_index = offence.latest_offence_session_index;
		self.offence_count = offence.offence_count;
	}
}

/// EVM struct for validator offences
pub struct ValidatorOffences<Runtime: pallet_bfc_offences::Config> {
	/// The address of this validator
	pub validator: Vec<Address>,
	/// The latest round this validator earned an offence
	pub latest_offence_round_index: Vec<RoundIndex>,
	/// The latest session this validator earned an offence
	pub latest_offence_session_index: Vec<RoundIndex>,
	/// The total offences this validator earned
	pub offence_count: Vec<OffenceCount>,
	/// A zero-sized type used to mark things that "act like" they own a T.
	phantom: PhantomData<Runtime>,
}

impl<Runtime> ValidatorOffences<Runtime>
where
	Runtime: pallet_bfc_offences::Config,
	Runtime::AccountId: Into<H160>,
{
	pub fn default() -> Self {
		ValidatorOffences {
			validator: vec![],
			latest_offence_round_index: vec![],
			latest_offence_session_index: vec![],
			offence_count: vec![],
			phantom: PhantomData,
		}
	}

	pub fn insert_empty(&mut self) {
		self.validator.push(Address(Default::default()));
		self.latest_offence_round_index.push(0u32);
		self.latest_offence_session_index.push(0u32);
		self.offence_count.push(0u32);
	}

	pub fn insert_offence(&mut self, offence: ValidatorOffence<Runtime>) {
		self.validator.push(Address(offence.validator.into()));
		self.latest_offence_round_index.push(offence.latest_offence_round_index);
		self.latest_offence_session_index.push(offence.latest_offence_session_index);
		self.offence_count.push(offence.offence_count);
	}
}

impl<Runtime> From<ValidatorOffences<Runtime>> for EvmValidatorOffenceOf
where
	Runtime: pallet_bfc_offences::Config,
{
	fn from(offence: ValidatorOffences<Runtime>) -> Self {
		(
			offence.validator[0],
			offence.latest_offence_round_index[0],
			offence.latest_offence_session_index[0],
			offence.offence_count[0],
		)
	}
}

impl<Runtime> From<ValidatorOffences<Runtime>> for EvmValidatorOffencesOf
where
	Runtime: pallet_bfc_offences::Config,
{
	fn from(offence: ValidatorOffences<Runtime>) -> Self {
		(
			offence.validator,
			offence.latest_offence_round_index,
			offence.latest_offence_session_index,
			offence.offence_count,
		)
	}
}
