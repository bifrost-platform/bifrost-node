use sp_runtime::DispatchError;

use crate::UnboundedBytes;

pub trait SocketVerifier<AccountId> {
	/// Verify a Socket message whether it is valid.
	fn verify_socket_message(msg: &UnboundedBytes) -> Result<(), DispatchError>;

	/// Returns the maximum allowed byte size for a single socket message.
	fn get_max_socket_message_bytes() -> u32;
}

pub trait RelayQueueManager<AccountId> {
	/// Replace an authority ID in all pending transfers.
	///
	/// This updates the authority ID in voter lists for:
	/// - On-flight transfers (on_flight_voters)
	/// - Pending finalization (finalization_voters)
	///
	/// Called when a relayer changes their account ID to maintain
	/// voting integrity across authority replacements.
	fn replace_authority(old: &AccountId, new: &AccountId);
}

impl<AccountId> RelayQueueManager<AccountId> for () {
	fn replace_authority(_old: &AccountId, _new: &AccountId) {}
}
