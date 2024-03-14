use miniscript::bitcoin::{
	opcodes::all::OP_CHECKMULTISIG, script::Builder, Address, Network, Opcode, PublicKey,
};

use scale_info::prelude::string::ToString;
use sp_core::Get;
use sp_runtime::{BoundedVec, DispatchError};
use sp_std::{str, str::FromStr, vec, vec::Vec};

use crate::BoundedBitcoinAddress;

use super::pallet::*;

impl<T: Config> Pallet<T> {
	/// Convert string typed public keys to `PublicKey` type and return the sorted list.
	fn sort_pub_keys(raw_pub_keys: Vec<[u8; 33]>) -> Result<Vec<PublicKey>, DispatchError> {
		let mut pub_keys = vec![];
		for raw_key in raw_pub_keys.iter() {
			let key = PublicKey::from_slice(raw_key).map_err(|_| Error::<T>::InvalidPublicKey)?;
			pub_keys.push(key);
		}
		pub_keys.sort();
		Ok(pub_keys)
	}

	/// Build the script for p2wsh address creation.
	fn build_redeem_script(pub_keys: Vec<PublicKey>) -> Builder {
		let mut redeem_script =
			Builder::new().push_opcode(Opcode::from(<RequiredM<T>>::get().saturating_add(80))); // m

		for key in pub_keys.iter() {
			redeem_script = redeem_script.push_key(&key);
		}

		redeem_script
			.push_opcode(Opcode::from(<RequiredN<T>>::get().saturating_add(80))) // n
			.push_opcode(OP_CHECKMULTISIG)
	}

	/// Generate a multi-sig vault address.
	pub fn generate_vault_address(
		raw_pub_keys: Vec<[u8; 33]>,
	) -> Result<BoundedBitcoinAddress, DispatchError> {
		let sorted_pub_keys = Self::sort_pub_keys(raw_pub_keys)?;
		let redeem_script = Self::build_redeem_script(sorted_pub_keys);

		// generate vault address
		Ok(BoundedVec::try_from(
			Address::p2wsh(redeem_script.as_script(), Self::get_bitcoin_network())
				.to_string()
				.as_bytes()
				.to_vec(),
		)
		.map_err(|_| Error::<T>::InvalidBitcoinAddress)?)
	}

	/// Check if the given address is valid on the target Bitcoin network. Then returns the checked address.
	pub fn get_checked_bitcoin_address(
		address: &Vec<u8>,
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
