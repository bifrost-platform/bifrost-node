use pallet_btc_registration_pool::{ADDRESS_MAX_BYTE_LENGTH, SIGNATURE_MAX_BYTE_LENGTH};

use precompile_utils::prelude::{Address, BoundedBytes, BoundedString};
use sp_core::ConstU32;
use sp_std::vec::Vec;

pub type BitcoinAddressString = BoundedString<ConstU32<ADDRESS_MAX_BYTE_LENGTH>>;

pub type SignatureBytes = BoundedBytes<ConstU32<SIGNATURE_MAX_BYTE_LENGTH>>;

pub type BtcRegistrationPoolOf<Runtime> = pallet_btc_registration_pool::Pallet<Runtime>;

pub type EvmRegistrationPoolOf =
	(Vec<Address>, Vec<BitcoinAddressString>, Vec<BitcoinAddressString>);
