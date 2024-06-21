use bp_multi_sig::ADDRESS_MAX_LENGTH;
use sp_core::{ConstU32, H256, U256};
use sp_std::{vec, vec::Vec};

use precompile_utils::prelude::{Address, BoundedString, UnboundedBytes};

/// The length bounded string type for Bitcoin addresses. (~90 alphanumeric characters)
pub type BitcoinAddressString = BoundedString<ConstU32<ADDRESS_MAX_LENGTH>>;

pub type EvmRollbackRequestOf = (
	UnboundedBytes,       // unsigned_psbt
	Address,              // who
	H256,                 // txid
	U256,                 // vout
	BitcoinAddressString, // to
	U256,                 // amount
	Vec<Address>,         // votes.key
	Vec<bool>,            // votes.value
	bool,                 // is_approved
);

pub struct RollbackRequest {
	pub unsigned_psbt: UnboundedBytes,
	pub who: Address,
	pub txid: H256,
	pub vout: U256,
	pub to: BitcoinAddressString,
	pub amount: U256,
	pub voted_authorities: Vec<Address>,
	pub votes: Vec<bool>,
	pub is_approved: bool,
}

impl RollbackRequest {
	pub fn default() -> Self {
		Self {
			unsigned_psbt: UnboundedBytes::from(vec![]),
			who: Address(Default::default()),
			txid: H256::default(),
			vout: U256::default(),
			to: BitcoinAddressString::from(vec![]),
			amount: U256::default(),
			voted_authorities: vec![],
			votes: vec![],
			is_approved: false,
		}
	}
}

impl From<RollbackRequest> for EvmRollbackRequestOf {
	fn from(value: RollbackRequest) -> Self {
		(
			value.unsigned_psbt,
			value.who,
			value.txid,
			value.vout,
			value.to,
			value.amount,
			value.voted_authorities,
			value.votes,
			value.is_approved,
		)
	}
}
