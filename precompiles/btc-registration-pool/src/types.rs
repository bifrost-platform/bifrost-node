use bp_multi_sig::{ADDRESS_MAX_LENGTH, PUBLIC_KEY_MAX_LENGTH};

use precompile_utils::prelude::{Address, BoundedString};
use sp_core::ConstU32;
use sp_std::{vec, vec::Vec};

/// The length bounded string type for Bitcoin addresses. (~64 alphanumeric characters)
pub type BitcoinAddressString = BoundedString<ConstU32<ADDRESS_MAX_LENGTH>>;

/// The length bounded string type for public keys. (~66 alphanumeric characters)
pub type PublicKeyString = BoundedString<ConstU32<PUBLIC_KEY_MAX_LENGTH>>;

/// The solidity type for `RegistrationPool`.
pub type EvmRegistrationPoolOf =
	(Vec<Address>, Vec<BitcoinAddressString>, Vec<BitcoinAddressString>);

/// The solidity type for pending registrations.
pub type EvmPendingRegistrationsOf = (Vec<Address>, Vec<BitcoinAddressString>);

pub type EvmRegistrationInfoOf =
	(Address, BitcoinAddressString, BitcoinAddressString, Vec<Address>, Vec<PublicKeyString>);

pub struct RegistrationInfo {
	pub user_bfc_address: Address,
	pub refund_address: BitcoinAddressString,
	pub vault_address: BitcoinAddressString,
	pub authorities: Vec<Address>,
	pub pub_keys: Vec<PublicKeyString>,
}

impl RegistrationInfo {
	pub fn default() -> Self {
		Self {
			user_bfc_address: Address(Default::default()),
			refund_address: BitcoinAddressString::from(vec![]),
			vault_address: BitcoinAddressString::from(vec![]),
			authorities: vec![],
			pub_keys: vec![],
		}
	}
}

impl From<RegistrationInfo> for EvmRegistrationInfoOf {
	fn from(value: RegistrationInfo) -> Self {
		(
			value.user_bfc_address,
			value.refund_address,
			value.vault_address,
			value.authorities,
			value.pub_keys,
		)
	}
}
