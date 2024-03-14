use miniscript::bitcoin::{
	opcodes::all::OP_CHECKMULTISIG, script::Builder, Address, Network, Opcode, PublicKey,
};

use scale_info::prelude::string::String;
use sp_core::Get;
use sp_runtime::{BoundedVec, DispatchError};
use sp_std::str::FromStr;

use crate::BoundedBitcoinAddress;

use super::pallet::*;

impl<T: Config> Pallet<T> {
	/// Convert string typed public keys to `PublicKey` type and return the sorted list.
	fn sort_pub_keys(raw_pub_keys: Vec<String>) -> Result<Vec<PublicKey>, DispatchError> {
		let mut pub_keys = vec![];
		for raw_key in raw_pub_keys.into_iter() {
			let key = PublicKey::from_str(&raw_key).map_err(|_| Error::<T>::InvalidPublicKey)?;
			pub_keys.push(key);
		}
		pub_keys.sort();
		Ok(pub_keys)
	}

	/// Build the script for p2wsh address creation.
	fn build_redeem_script(pub_keys: Vec<PublicKey>) -> Builder {
		let mut redeem_script =
			Builder::new().push_opcode(Opcode::from(<RequiredM<T>>::get().saturating_add(80))); // m

		for key in pub_keys.into_iter() {
			redeem_script = redeem_script.push_key(&key);
		}

		redeem_script
			.push_opcode(Opcode::from(<RequiredN<T>>::get().saturating_add(80))) // n
			.push_opcode(OP_CHECKMULTISIG)
	}

	/// Generate a multi-sig vault address.
	pub fn generate_vault_address(
		raw_pub_keys: Vec<String>,
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

	fn get_bitcoin_network() -> Network {
		match T::IsBitcoinMainnet::get() {
			true => Network::Bitcoin,
			_ => Network::Testnet,
		}
	}
}
