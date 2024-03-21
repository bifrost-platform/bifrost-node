use bp_multi_sig::{Psbt, PsbtBytes};
use sp_core::H256;
use sp_io::hashing::keccak_256;
use sp_runtime::DispatchError;

use super::pallet::*;

impl<T: Config> Pallet<T> {
	/// Try to deserialize the given bytes to a `PSBT` instance.
	pub fn try_get_checked_psbt(psbt: &PsbtBytes) -> Result<Psbt, DispatchError> {
		Ok(Psbt::deserialize(psbt).map_err(|_| Error::<T>::InvalidPsbt)?)
	}

	/// Try to combine the signed PSBT with the origin. If fails, the given PSBT is considered as invalid.
	pub fn verify_signed_psbt(origin: &PsbtBytes, signed: &PsbtBytes) -> Result<(), DispatchError> {
		let mut o = Self::try_get_checked_psbt(origin)?;
		let s = Self::try_get_checked_psbt(signed)?;
		Ok(o.combine(s).map_err(|_| Error::<T>::InvalidPsbt)?)
	}

	/// Hash the PSBT bytes with keccak256.
	pub fn hash_psbt(psbt: &PsbtBytes) -> H256 {
		H256(keccak_256(psbt))
	}
}
