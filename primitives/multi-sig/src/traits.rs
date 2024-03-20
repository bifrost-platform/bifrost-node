use miniscript::bitcoin::{
	key::Error, opcodes::all::OP_CHECKMULTISIG, script::Builder, Address, Network, Opcode,
	PublicKey, Script,
};

use sp_std::{vec, vec::Vec};

use crate::Public;

pub trait MultiSigManager {
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

	/// Build the script for p2wsh address creation.
	fn build_redeem_script(pub_keys: Vec<PublicKey>, m: u8, n: u8) -> Builder {
		let mut redeem_script = Builder::new().push_opcode(Opcode::from(m.saturating_add(80))); // m

		for key in pub_keys.iter() {
			redeem_script = redeem_script.push_key(&key);
		}

		redeem_script
			.push_opcode(Opcode::from(n.saturating_add(80))) // n
			.push_opcode(OP_CHECKMULTISIG)
	}

	/// Creates a witness pay to script hash address.
	fn generate_address(script: &Script, network: Network) -> Address {
		Address::p2wsh(script, network)
	}
}
