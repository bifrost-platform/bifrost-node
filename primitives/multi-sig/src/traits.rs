use miniscript::bitcoin::{key::Error, Address, Network, PublicKey, Script};

use sp_std::{vec, vec::Vec};

use crate::{BoundedBitcoinAddress, Public};

pub trait MultiSigManager {
	/// Check if the PSBT finalizable.
	fn is_finalizable(m: u8) -> bool;

	/// Convert string typed public keys to `PublicKey` type and return the sorted list.
	fn sort_pub_keys(raw_pub_keys: Vec<Public>) -> Result<Vec<PublicKey>, Error> {
		let mut pub_keys = vec![];
		for raw_key in raw_pub_keys.iter() {
			let key = PublicKey::from_slice(raw_key.as_ref())?;
			pub_keys.push(key);
		}
		pub_keys.sort();
		Ok(pub_keys)
	}

	/// Creates a witness pay to script hash address.
	fn generate_address(script: &Script, network: Network) -> Address {
		Address::p2wsh(script, network)
	}
}

pub trait PoolManager<AccountId> {
	/// Get the refund address of the given user.
	fn get_refund_address(who: &AccountId) -> Option<BoundedBitcoinAddress>;

	/// Get the system vault address.
	fn get_system_vault() -> Option<BoundedBitcoinAddress>;

	/// Get the Bitcoin network of the current runtime.
	fn get_bitcoin_network() -> Network;

	/// Get the Bitcoin chain ID.
	fn get_bitcoin_chain_id() -> u32;
}
