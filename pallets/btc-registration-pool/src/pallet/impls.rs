use sp_runtime::DispatchError;

use crate::BoundedBitcoinAddress;

use super::pallet::*;

impl<T: Config> Pallet<T> {
	/// Convert a Bitcoin address to a string type.
	pub fn convert_bitcoin_address_to_string(
		address: &BoundedBitcoinAddress,
	) -> Result<String, DispatchError> {
		Ok(String::from_utf8(address.clone().into_inner())
			.map_err(|_| <Error<T>>::InvalidBitcoinAddress)?)
	}
}
