#![cfg_attr(not(feature = "std"), no_std)]

mod pallet;
pub use pallet::pallet::*;

pub mod weights;
use weights::WeightInfo;

use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

use bp_multi_sig::{PsbtBytes, MULTI_SIG_MAX_ACCOUNTS};
use sp_core::{ConstU32, RuntimeDebug};
use sp_runtime::BoundedBTreeMap;

/// The outbound request sequence index.
pub type ReqId = u128;

#[derive(Decode, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
/// The submitted PSBT information of a single outbound request.
pub struct OutboundRequest<AccountId> {
	/// The submitted initial unsigned PSBT (in bytes).
	pub unsigned_psbt: PsbtBytes,
	/// The submitted signed PSBT's (in bytes).
	pub signed_psbts: BoundedBTreeMap<AccountId, PsbtBytes, ConstU32<MULTI_SIG_MAX_ACCOUNTS>>,
}

impl<AccountId: PartialEq + Clone + Ord> OutboundRequest<AccountId> {
	/// Instantiates a new `OutboundRequest` instance.
	pub fn new(unsigned_psbt: PsbtBytes) -> Self {
		Self { unsigned_psbt, signed_psbts: BoundedBTreeMap::default() }
	}

	/// Check if the given authority has already submitted a signed PSBT.
	pub fn is_authority_submitted(&self, authority_id: &AccountId) -> bool {
		self.signed_psbts.contains_key(authority_id)
	}

	/// Check if the given PSBT matches with the initial unsigned PSBT.
	pub fn is_unsigned_psbt(&self, psbt: &PsbtBytes) -> bool {
		self.unsigned_psbt.eq(psbt)
	}

	/// Insert the signed PSBT submitted by the authority.
	pub fn insert_signed_psbt(
		&mut self,
		authority_id: AccountId,
		psbt: PsbtBytes,
	) -> Result<Option<PsbtBytes>, (AccountId, PsbtBytes)> {
		self.signed_psbts.try_insert(authority_id, psbt)
	}
}

#[derive(Decode, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
/// The message payload for unsigned PSBT submission.
pub struct UnsignedPsbtMessage<AccountId> {
	/// The submitter's account address.
	pub submitter: AccountId,
	/// The outbound request's sequence index.
	pub req_id: ReqId,
	/// The unsigned PSBT (in bytes).
	pub psbt: PsbtBytes,
}

#[derive(Decode, Encode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
/// The message payload for signed PSBT submission.
pub struct SignedPsbtMessage<AccountId> {
	/// The authority's account address.
	pub authority_id: AccountId,
	/// The outbound request's sequence index.
	pub req_id: ReqId,
	/// The unsigned PSBT (in bytes).
	pub unsigned_psbt: PsbtBytes,
	/// The signed PSBT (in bytes).
	pub signed_psbt: PsbtBytes,
}
