use bp_staking::traits::Authorities;
use frame_support::pallet_prelude::{
	InvalidTransaction, TransactionPriority, TransactionValidity, ValidTransaction,
};
use parity_scale_codec::alloc::string::ToString;
use scale_info::prelude::{format, string::String};
use sp_runtime::traits::Verify;
use sp_std::vec::Vec;

use crate::{FeeRateSubmission, OutboundRequestSubmission, SpendTxosSubmission, UtxoSubmission};

use super::pallet::*;

impl<T: Config> Pallet<T> {
	pub fn verify_submit_utxos(
		utxo_submission: &UtxoSubmission<T::AccountId>,
		signature: &T::Signature,
	) -> TransactionValidity {
		let UtxoSubmission { authority_id, votes } = utxo_submission;

		// verify if the authority is a selected relayer.
		if !T::Relayers::is_authority(&authority_id) {
			return Err(InvalidTransaction::BadSigner.into());
		}

		// verify if the signature was originated from the authority.
		let message = format!(
			"{}",
			votes.iter().map(|x| x.utxo_hash.to_string()).collect::<Vec<String>>().concat()
		);
		if !signature.verify(message.as_bytes(), &authority_id) {
			return Err(InvalidTransaction::BadProof.into());
		}

		ValidTransaction::with_tag_prefix("UtxoSubmission")
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

	pub fn verify_spend_txos(
		spend_txos_submission: &SpendTxosSubmission<T::AccountId>,
		signature: &T::Signature,
	) -> TransactionValidity {
		let SpendTxosSubmission { authority_id, locked_txos } = spend_txos_submission;

		// TODO: verify authority
		// TODO: verify signature

		ValidTransaction::with_tag_prefix("SpendTxosSubmission")
			.priority(TransactionPriority::MAX)
			.and_provides(authority_id)
			.propagate(true)
			.build()
	}
}
