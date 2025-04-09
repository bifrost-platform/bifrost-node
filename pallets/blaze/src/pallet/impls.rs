use frame_support::pallet_prelude::{TransactionPriority, TransactionValidity, ValidTransaction};

use crate::{FeeRateSubmission, OutboundRequestSubmission, SpendTxosSubmission, UtxoSubmission};

use super::pallet::*;

impl<T: Config> Pallet<T> {
	pub fn verify_submit_utxos(
		utxo_submission: &UtxoSubmission<T::AccountId>,
		signature: &T::Signature,
	) -> TransactionValidity {
		let UtxoSubmission { authority_id, utxos, pool_round } = utxo_submission;

		// TODO: verify authority
		// TODO: verify signature

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
		let FeeRateSubmission { authority_id, fee_rate, pool_round } = fee_rate_submission;

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
		let OutboundRequestSubmission { authority_id, messages, pool_round } =
			outbound_request_submission;

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
		let SpendTxosSubmission { authority_id, locked_txos, pool_round } = spend_txos_submission;

		// TODO: verify authority
		// TODO: verify signature

		ValidTransaction::with_tag_prefix("SpendTxosSubmission")
			.priority(TransactionPriority::MAX)
			.and_provides(authority_id)
			.propagate(true)
			.build()
	}
}
