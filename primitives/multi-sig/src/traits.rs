use miniscript::{
	bitcoin::{key::Error, Network, PublicKey},
	Descriptor,
};

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

	fn generate_descriptor(
		m: usize,
		raw_pub_keys: Vec<Public>,
	) -> Result<Descriptor<PublicKey>, ()> {
		let desc =
			Descriptor::new_wsh_sortedmulti(m, Self::sort_pub_keys(raw_pub_keys).map_err(|_| ())?)
				.map_err(|_| ())?;
		desc.sanity_check().map_err(|_| ())?;
		Ok(desc)
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
