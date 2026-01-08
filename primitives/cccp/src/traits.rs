use sp_runtime::DispatchError;

use crate::UnboundedBytes;

pub trait SocketVerifier<AccountId> {
	/// Verify a Socket message whether it is valid.
	fn verify_socket_message(msg: &UnboundedBytes) -> Result<(), DispatchError>;
}
