use crate::{Offence, RoundIndex, TierType, MAX_AUTHORITIES};
use frame_support::{pallet_prelude::ConstU32, BoundedBTreeSet};

use sp_runtime::{DispatchError, Perbill};
use sp_std::vec::Vec;

/// The trait used for authority management. (ex: Relayers)
pub trait Authorities<AccountId> {
	/// Verifies whether the account is an authority.
	fn is_authority(who: &AccountId) -> bool;

	/// Get the number of items in the set.
	fn count() -> usize;

	/// Get the majority number of the set.
	fn majority() -> u32;
}

pub trait StakingManager<AccountId> {
	/// Verifies if the given account is a stash
	fn is_stash(acc: &AccountId) -> bool;

	/// Verifies if the given account is a candidate
	fn is_candidate(acc: &AccountId, tier: TierType) -> bool;

	/// Verifies if the given account is a nominator
	fn is_nominator(acc: &AccountId) -> bool;
}

/// The trait used for `pallet_relay_manager`
pub trait RelayManager<AccountId> {
	/// Verifies if the given account is a relayer
	fn is_relayer(acc: &AccountId) -> bool;

	/// Add the given `relayer` to the `RelayerPool` and bond to the given `controller` account
	fn join_relayers(relayer: AccountId, controller: AccountId) -> Result<(), DispatchError>;

	/// Refresh the current round
	fn refresh_round(now: RoundIndex);

	/// Refresh the relayers status to default.
	fn refresh_relayer_pool();

	/// Refresh the selected relayers based on the new selected candidates
	fn refresh_selected_relayers(round: RoundIndex, selected_candidates: Vec<AccountId>);

	/// Refresh the `CachedSelectedRelayers` based on the new selected relayers
	fn refresh_cached_selected_relayers(
		round: RoundIndex,
		relayers: BoundedBTreeSet<AccountId, ConstU32<MAX_AUTHORITIES>>,
	);

	/// Refresh the `Majority` and `CachedMajority` of the selected relayers
	fn refresh_majority(round: RoundIndex);

	/// Re-bond the old controller to the new controller
	fn replace_bonded_controller(old: AccountId, new: AccountId);

	/// Remove and unbond the controller from `RelayerPool`
	fn leave_relayers(controller: &AccountId);

	/// Kickout relayer from current selected relayers
	fn kickout_relayer(controller: &AccountId);

	/// Collect every heartbeats sent from relayers for the current session. Verifies if each
	/// relayer has pulsed a heartbeat. If not, it will report an offence. This method will be
	/// requested at every block before the session ends.
	fn collect_heartbeats();

	/// Apply the delayed relayer set requests. Replaces the entire bonded storage values from the old to new.
	fn handle_delayed_relayer_sets(now: RoundIndex);
}

/// The trait used for `pallet_bfc_offences`
pub trait OffenceHandler<AccountId, Balance> {
	/// Try to handle the given offence of a specific validator.
	/// This method checks whether the validator exceeds the maximum offence count. If it exceeds
	/// the offence will be handled.
	fn try_handle_offence(
		who: &AccountId,
		stash: &AccountId,
		tier: TierType,
		offence: Offence<Balance>,
	) -> (bool, Balance);

	/// Handles the given offence of a specific validator. The validator's reserved self-bond will
	/// be slashed by the given slash fraction. If not, the validator's offence count will be
	/// incremented.
	fn handle_offence(
		who: &AccountId,
		stash: &AccountId,
		tier: TierType,
		offence: Offence<Balance>,
	) -> (bool, Balance);

	/// This method first check whether the slash mechanism is activated. If it is activated the
	/// target validator's self-bond will be slashed.
	fn try_slash(
		who: &AccountId,
		stash: &AccountId,
		slash_fraction: Perbill,
		bond: Balance,
	) -> Balance;

	/// This method will be requested at every new session. It will check every offences stored in
	/// the system, and will remove the offence if the latest session exceeds the expiration.
	fn refresh_offences(round_index: RoundIndex);

	/// Verifies whether the given count has exceeded the maximum offence count.
	fn is_offence_count_exceeds(count: u32, tier: TierType) -> bool;
}
