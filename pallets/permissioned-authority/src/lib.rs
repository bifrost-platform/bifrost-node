//! # Permissioned Authority Pallet
//!
//! A root-managed replacement for `pallet-bfc-staking` on chains whose validators (and their
//! bonded CCCP relayers) are approved by an operator rather than selected by stake — e.g. the
//! private OmniFi mainnet hub.
//!
//! - **Registration is root-only.** `add_authority(validator, relayer)` /
//!   `remove_authority(validator)` edit the *requested* set, and `set_relayer` /
//!   `cancel_relayer_set` replace a validator's relayer; nothing else can change either
//!   (the runtime should filter `pallet-relay-manager`'s own signed `set_relayer`).
//! - **Rounds are change-driven.** On every session rotation the requested set is compared
//!   with the *active* one (plus any pending relayer replacement).
//!   Only when something changed does the pallet start a new round, refresh the selected relayers
//!   through the `RelayManager` hooks and hand the new validator set to `pallet-session`.
//!   Otherwise the round — and therefore the CCCP round the relayers sync across chains — stays
//!   the same, so no round-up is relayed while the set is unchanged.
//! - **Offences are reported, never enforced.** The `OnOffenceHandler` only emits an event:
//!   validators are removed by root alone, so a liveness offence can never empty the set.
//!
//! The `Round` storage keeps the exact shape of `pallet-bfc-staking`'s `RoundInfo` so the
//! `latest_round()`/`round_info()` precompile ABI the relayer reads stays unchanged.
//! `round_length` is a nominal value (`NominalRoundLength`): rounds have no fixed length here.

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unused_crate_dependencies)]

mod pallet;
pub mod weights;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub use pallet::pallet::*;
pub use weights::WeightInfo;

use bp_staking::RoundIndex;
use frame_support::pallet_prelude::*;
use sp_runtime::RuntimeDebug;
use sp_staking::SessionIndex;

pub(crate) const LOG_TARGET: &'static str = "runtime::permissioned-authority";

// syntactic sugar for logging.
#[macro_export]
macro_rules! log {
	($level:tt, $patter:expr $(, $values:expr)* $(,)?) => {
		log::$level!(
			target: crate::LOG_TARGET,
			concat!("[{:?}] 🛡️ ", $patter), <frame_system::Pallet<T>>::block_number() $(, $values)*
		)
	};
}

/// The current round index and transition information. Same layout as
/// `pallet_bfc_staking::RoundInfo`.
#[derive(
	Copy,
	Clone,
	PartialEq,
	Eq,
	Encode,
	Decode,
	DecodeWithMemTracking,
	RuntimeDebug,
	TypeInfo,
	MaxEncodedLen,
	Default,
)]
pub struct RoundInfo<BlockNumber> {
	/// Current round index
	pub current_round_index: RoundIndex,
	/// Current round first session index
	pub first_session_index: SessionIndex,
	/// Current round current session index
	pub current_session_index: SessionIndex,
	/// The first block of the current round
	pub first_round_block: BlockNumber,
	/// The first block of the current session
	pub first_session_block: BlockNumber,
	/// The current block of the current round
	pub current_block: BlockNumber,
	/// The (nominal) length of a round in number of blocks
	pub round_length: u32,
	/// The length of a session in number of blocks
	pub session_length: u32,
}

impl<B: Copy> RoundInfo<B> {
	/// Start a new round at block `now`, in the current session.
	pub fn update_round(&mut self, now: B) {
		self.current_round_index = self.current_round_index.saturating_add(1);
		self.first_session_index = self.current_session_index;
		self.first_round_block = now;
		self.first_session_block = now;
		self.current_block = now;
	}

	/// Start a new session at block `now`.
	pub fn update_session(&mut self, now: B, new_session: SessionIndex) {
		self.current_session_index = new_session;
		self.first_session_block = now;
	}

	/// New block
	pub fn update_block(&mut self, now: B) {
		self.current_block = now;
	}
}
