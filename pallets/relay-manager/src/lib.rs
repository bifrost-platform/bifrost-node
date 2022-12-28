#![cfg_attr(not(feature = "std"), no_std)]

pub mod migrations;
mod pallet;
pub mod weights;

pub use pallet::{pallet::*, *};
use weights::WeightInfo;

use frame_support::traits::{ValidatorSet, ValidatorSetWithIdentification};
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

use sp_runtime::{Perbill, RuntimeDebug};
use sp_staking::{
	offence::{Kind, Offence},
	SessionIndex,
};
use sp_std::{marker::PhantomData, prelude::*};

/// A type for representing the validator id in a session.
pub type ValidatorId<T> = <<T as Config>::ValidatorSet as ValidatorSet<
	<T as frame_system::Config>::AccountId,
>>::ValidatorId;

/// A tuple of (ValidatorId, Identification) where `Identification` is the full identification of
/// `ValidatorId`.
pub type IdentificationTuple<T> = (
	ValidatorId<T>,
	<<T as Config>::ValidatorSet as ValidatorSetWithIdentification<
		<T as frame_system::Config>::AccountId,
	>>::Identification,
);

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo)]
/// A value placed in storage that represents the current version of the Relay Manager storage. This
/// value is used by the `on_runtime_upgrade` logic to determine whether we run storage migration
/// logic.
enum Releases {
	V1_0_0,
	V2_0_0,
}

impl Default for Releases {
	fn default() -> Self {
		Releases::V2_0_0
	}
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo)]
/// An enum that represents the current state of a relayer
pub enum RelayerStatus {
	/// It is well behaved and sent a heartbeat for the current session
	Active,
	/// It is offline due to unsending heartbeats for the current session
	Idle,
	/// It is kicked out due to continueing unresponsiveness
	KickedOut,
}

#[derive(Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
/// The bonded controller and its owned relayer address
pub struct Relayer<AccountId> {
	/// This relayer's address
	pub relayer: AccountId,
	/// This relayers' bonded controller address
	pub controller: AccountId,
}

#[derive(Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
/// The current state of a specific relayer
pub struct RelayerMetadata<AccountId> {
	/// This relayer's bonded controller address
	pub controller: AccountId,
	/// This relayer's current status
	pub status: RelayerStatus,
}

impl<AccountId: PartialEq + Clone> RelayerMetadata<AccountId> {
	pub fn new(controller: AccountId) -> Self {
		RelayerMetadata { controller, status: RelayerStatus::Active }
	}

	pub fn go_offline(&mut self) {
		self.status = RelayerStatus::Idle;
	}

	pub fn go_online(&mut self) {
		self.status = RelayerStatus::Active;
	}

	pub fn kick_out(&mut self) {
		self.status = RelayerStatus::KickedOut;
	}

	pub fn set_controller(&mut self, controller: AccountId) {
		self.controller = controller;
	}
}

#[derive(RuntimeDebug, TypeInfo)]
#[cfg_attr(feature = "std", derive(Clone, PartialEq, Eq))]
/// An offence that is filed if a validator didn't send a heartbeat message.
pub struct UnresponsivenessOffence<Offender, T> {
	/// The current session index in which we report the unresponsive validators.
	///
	/// It acts as a time measure for unresponsiveness reports and effectively will always point
	/// at the end of the session.
	pub session_index: SessionIndex,
	/// The size of the validator set in the current session.
	pub validator_set_count: u32,
	/// Authorities that were unresponsive during the current session.
	pub offenders: Vec<Offender>,
	/// A zero-sized type used to mark things that "act like" they own a T.
	phantom: PhantomData<T>,
}

impl<Offender: Clone, T: pallet::pallet::Config> Offence<Offender>
	for UnresponsivenessOffence<Offender, T>
{
	const ID: Kind = *b"relay-mgr:offlin";
	type TimeSlot = SessionIndex;

	fn offenders(&self) -> Vec<Offender> {
		self.offenders.clone()
	}

	fn session_index(&self) -> SessionIndex {
		self.session_index
	}

	fn validator_set_count(&self) -> u32 {
		self.validator_set_count
	}

	fn time_slot(&self) -> Self::TimeSlot {
		self.session_index
	}

	fn slash_fraction(&self, _offenders: u32) -> Perbill {
		<HeartbeatSlashFraction<T>>::get()
	}
}
