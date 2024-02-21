use pallet_btc_registration_pool::{ADDRESS_MAX_LENGTH, SIGNATURE_BYTE_MAX_LENGTH};

use precompile_utils::prelude::{Address, BoundedBytes, BoundedString};
use sp_core::ConstU32;
use sp_std::vec::Vec;

/// The length bounded string type for Bitcoin addresses. (~62 alphanumeric characters)
pub type BitcoinAddressString = BoundedString<ConstU32<ADDRESS_MAX_LENGTH>>;

/// The byte size bounded type for signatures. (~65 bytes)
pub type SignatureBytes = BoundedBytes<ConstU32<SIGNATURE_BYTE_MAX_LENGTH>>;

/// The solidity type for `RegistrationPool`.
pub type EvmRegistrationPoolOf =
	(Vec<Address>, Vec<BitcoinAddressString>, Vec<BitcoinAddressString>);
