use bp_staking::traits::Authorities;
use frame_support::pallet_prelude::{
	InvalidTransaction, TransactionPriority, TransactionValidity, ValidTransaction,
};
use parity_scale_codec::alloc::string::ToString;
use scale_info::prelude::{format, string::String};
use sp_io::hashing::keccak_256;
use sp_runtime::traits::Verify;
use sp_std::vec::Vec;

use crate::{FeeRateSubmission, OutboundRequestSubmission, UtxoSubmission};

use super::pallet::*;

impl<T: Config> Pallet<T> {
	pub fn verify_utxo_submission(
		utxo_submission: &UtxoSubmission<T::AccountId>,
		signature: &T::Signature,
		tag_prefix: &'static str,
	) -> TransactionValidity {
		let UtxoSubmission { authority_id, votes } = utxo_submission;

		// verify if the authority is a selected relayer.
		if !T::Relayers::is_authority(&authority_id) {
			return Err(InvalidTransaction::BadSigner.into());
		}

		// verify if the signature was originated from the authority.
		let message = [
			keccak_256(tag_prefix.as_bytes()).as_slice(),
			format!(
				"{}",
				votes.iter().map(|x| x.utxo_hash.to_string()).collect::<Vec<String>>().concat()
			)
			.as_bytes(),
		]
		.concat();
		if !signature.verify(&*message, &authority_id) {
			return Err(InvalidTransaction::BadProof.into());
		}

		ValidTransaction::with_tag_prefix(tag_prefix)
			.priority(TransactionPriority::MAX)
			.and_provides(authority_id)
			.propagate(true)
			.build()
	}

	pub fn verify_submit_fee_rate(
		fee_rate_submission: &FeeRateSubmission<T::AccountId>,
		signature: &T::Signature,
	) -> TransactionValidity {
		let FeeRateSubmission { authority_id, fee_rate } = fee_rate_submission;

		// TODO: verify authority
		// TODO: verify signature

		ValidTransaction::with_tag_prefix("FeeRateSubmission")
			.priority(TransactionPriority::MAX)
			.and_provides(authority_id)
			.propagate(true)
			.build()
	}

	pub fn verify_submit_outbound_requests(
		outbound_request_submission: &OutboundRequestSubmission<T::AccountId>,
		signature: &T::Signature,
	) -> TransactionValidity {
		let OutboundRequestSubmission { authority_id, messages } = outbound_request_submission;

		// TODO: verify authority
		// TODO: verify signature

		ValidTransaction::with_tag_prefix("OutboundRequestSubmission")
			.priority(TransactionPriority::MAX)
			.and_provides(authority_id)
			.propagate(true)
			.build()
	}
}
