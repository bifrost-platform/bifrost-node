use bp_multi_sig::{
	traits::{MultiSigManager, PoolManager},
	Address, AddressState, Network, UnboundedBytes,
};
use scale_info::prelude::string::ToString;
use sp_core::Get;
use sp_runtime::{BoundedVec, DispatchError};
use sp_std::{str, str::FromStr, vec::Vec};

use crate::{BoundedBitcoinAddress, Public};

use super::pallet::*;

impl<T: Config> MultiSigManager for Pallet<T> {
	fn is_finalizable(m: u8) -> bool {
		<RequiredM<T>>::get() <= m
	}
}

impl<T: Config> PoolManager<T::AccountId> for Pallet<T> {
	fn get_refund_address(who: &T::AccountId) -> Option<BoundedBitcoinAddress> {
		if let Some(relay_target) = Self::registration_pool(who) {
			Some(relay_target.refund_address)
		} else {
			None
		}
	}

	fn get_system_vault() -> Option<BoundedBitcoinAddress> {
		if let Some(vault) = Self::system_vault() {
			match vault.address {
				AddressState::Pending => return None,
				AddressState::Generated(address) => return Some(address),
			};
		} else {
			None
		}
	}

	fn get_bitcoin_network() -> Network {
		Self::get_bitcoin_network()
	}
}

impl<T: Config> Pallet<T> {
	/// Generate a multi-sig vault address.
	pub fn generate_vault_address(
		raw_pub_keys: Vec<Public>,
	) -> Result<BoundedBitcoinAddress, DispatchError> {
		let sorted_pub_keys =
			Self::sort_pub_keys(raw_pub_keys).map_err(|_| Error::<T>::InvalidPublicKey)?;
		let redeem_script = Self::build_redeem_script(
			sorted_pub_keys,
			<RequiredM<T>>::get(),
			<RequiredN<T>>::get(),
		);

		// generate vault address
		Ok(BoundedVec::try_from(
			Self::generate_address(redeem_script.as_script(), Self::get_bitcoin_network())
				.to_string()
				.as_bytes()
				.to_vec(),
		)
		.map_err(|_| Error::<T>::InvalidBitcoinAddress)?)
	}

	/// Check if the given address is valid on the target Bitcoin network. Then returns the checked address.
	pub fn get_checked_bitcoin_address(
		address: &UnboundedBytes,
	) -> Result<BoundedBitcoinAddress, DispatchError> {
		let raw_address = str::from_utf8(address).map_err(|_| Error::<T>::InvalidBitcoinAddress)?;
		let unchecked_address =
			Address::from_str(raw_address).map_err(|_| Error::<T>::InvalidBitcoinAddress)?;
		let checked_address = unchecked_address
			.require_network(Self::get_bitcoin_network())
			.map_err(|_| Error::<T>::InvalidBitcoinAddress)?
			.to_string();

		Ok(BoundedVec::try_from(checked_address.as_bytes().to_vec())
			.map_err(|_| Error::<T>::InvalidBitcoinAddress)?)
	}

	/// Get the Bitcoin network of the current runtime.
	fn get_bitcoin_network() -> Network {
		match T::IsBitcoinMainnet::get() {
			true => Network::Bitcoin,
			_ => Network::Testnet,
		}
	}
}
