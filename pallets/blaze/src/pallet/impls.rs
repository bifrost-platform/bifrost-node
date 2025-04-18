use bp_staking::traits::Authorities;
use frame_support::pallet_prelude::{
	InvalidTransaction, TransactionPriority, TransactionValidity, ValidTransaction,
};
use frame_system::pallet_prelude::BlockNumberFor;
use parity_scale_codec::Encode;
use scale_info::prelude::{format, string::String};
use sp_core::H256;
use sp_io::hashing::keccak_256;
use sp_runtime::traits::{Block, Header, Verify};
use sp_std::{fmt::Display, vec::Vec};

use crate::{FeeRateSubmission, OutboundRequestSubmission, UtxoSubmission};

use super::pallet::*;

impl<T: Config> Pallet<T> {
	pub fn verify_utxo_submission(
		utxo_submission: &UtxoSubmission<T::AccountId>,
		signature: &T::Signature,
		tag_prefix: &'static str,
	) -> TransactionValidity {
		let UtxoSubmission { authority_id, utxos: votes } = utxo_submission;

		// verify if the authority is a selected relayer.
		if !T::Relayers::is_authority(&authority_id) {
			return Err(InvalidTransaction::BadSigner.into());
		}

		// verify if the signature was originated from the authority.
		let message = [
			keccak_256(tag_prefix.as_bytes()).as_slice(),
			format!(
				"{}",
				votes
					.iter()
					.map(|x| {
						let utxo_hash = H256::from_slice(
							keccak_256(&Encode::encode(&(x.txid, x.vout, x.amount))).as_ref(),
						);
						hex::encode(utxo_hash)
					})
					.collect::<Vec<String>>()
					.concat()
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
		fee_rate_submission: &FeeRateSubmission<T::AccountId, BlockNumberFor<T>>,
		signature: &T::Signature,
	) -> TransactionValidity
	where
		<<<T as frame_system::Config>::Block as Block>::Header as Header>::Number: Display,
	{
		let FeeRateSubmission { authority_id, fee_rate, deadline } = fee_rate_submission;

		// verify if the authority is a selected relayer.
		if !T::Relayers::is_authority(&authority_id) {
			return Err(InvalidTransaction::BadSigner.into());
		}

		// verify if the deadline is not expired.
		let now = <frame_system::Pallet<T>>::block_number();
		if now > *deadline {
			return Err(InvalidTransaction::Stale.into());
		}

		// verify if the signature was originated from the authority.
		let message = format!("{}:{}", deadline, fee_rate);
		if !signature.verify(message.as_bytes(), &authority_id) {
			return Err(InvalidTransaction::BadProof.into());
		}

		ValidTransaction::with_tag_prefix("FeeRateSubmission")
			.priority(TransactionPriority::MAX)
			.and_provides((authority_id, fee_rate))
			.propagate(true)
			.build()
	}

	pub fn verify_submit_outbound_requests(
		outbound_request_submission: &OutboundRequestSubmission<T::AccountId>,
		signature: &T::Signature,
	) -> TransactionValidity {
		let OutboundRequestSubmission { authority_id, messages } = outbound_request_submission;

		// verify if the authority is a selected relayer.
		if !T::Relayers::is_authority(&authority_id) {
			return Err(InvalidTransaction::BadSigner.into());
		}

		// verify if the signature was originated from the authority.
		let message = format!(
			"{}",
			messages
				.iter()
				.map(|x| array_bytes::bytes2hex("0x", x))
				.collect::<Vec<String>>()
				.concat()
		);
		if !signature.verify(message.as_bytes(), &authority_id) {
			return Err(InvalidTransaction::BadProof.into());
		}

		ValidTransaction::with_tag_prefix("OutboundRequestSubmission")
			.priority(TransactionPriority::MAX)
			.and_provides(authority_id)
			.propagate(true)
			.build()
	}
}
