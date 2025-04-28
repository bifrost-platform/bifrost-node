use bp_btc_relay::traits::BlazeManager;
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

use crate::{BroadcastSubmission, FeeRateSubmission, OutboundRequestSubmission, UtxoSubmission};

use super::pallet::*;

impl<T: Config> BlazeManager<T> for Pallet<T> {
	fn is_activated() -> bool {
		<IsActivated<T>>::get()
	}

	fn take_executed_requests() -> Vec<H256> {
		<ExecutedRequests<T>>::take()
	}

	fn try_fee_rate_finalization(n: BlockNumberFor<T>) -> Option<u64> {
		let mut submitted_fee_rates = <FeeRates<T>>::get();
		// remove expired fee rates
		submitted_fee_rates.retain(|_, (_, expires_at)| n <= *expires_at);

		// check majority
		if submitted_fee_rates.len() as u32 >= T::Relayers::majority() {
			// choose the median fee rate
			let mut fee_rates = submitted_fee_rates.values().cloned().collect::<Vec<_>>();
			fee_rates.sort();
			let median_index = fee_rates.len() / 2;
			let median_fee_rate = fee_rates[median_index];
			Some(median_fee_rate.0)
		} else {
			None
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Helper function to verify if an authority is a valid relayer
	fn verify_authority(authority_id: &T::AccountId) -> Result<(), InvalidTransaction> {
		if !T::Relayers::is_authority(authority_id) {
			return Err(InvalidTransaction::BadSigner.into());
		}
		Ok(())
	}

	/// Helper function to verify a signature
	fn verify_signature(
		message: &[u8],
		signature: &T::Signature,
		authority_id: &T::AccountId,
	) -> Result<(), InvalidTransaction> {
		if !signature.verify(message, authority_id) {
			return Err(InvalidTransaction::BadProof.into());
		}
		Ok(())
	}

	/// Verify a UTXO submission.
	pub fn verify_utxo_submission(
		utxo_submission: &UtxoSubmission<T::AccountId>,
		signature: &T::Signature,
	) -> TransactionValidity {
		let UtxoSubmission { authority_id, utxos } = utxo_submission;

		// verify if the authority is a selected relayer.
		Self::verify_authority(authority_id)?;

		// verify if the signature was originated from the authority.
		let message = [
			keccak_256("UtxosSubmission".as_bytes()).as_slice(),
			format!(
				"{}",
				utxos
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
		Self::verify_signature(&message, signature, authority_id)?;

		ValidTransaction::with_tag_prefix("UtxosSubmission")
			.priority(TransactionPriority::MAX)
			.and_provides(authority_id)
			.propagate(true)
			.build()
	}

	/// Verify a spend UTXO submission.
	pub fn verify_broadcast_submission(
		broadcast_submission: &BroadcastSubmission<T::AccountId>,
		signature: &T::Signature,
	) -> TransactionValidity {
		let BroadcastSubmission { authority_id, txid } = broadcast_submission;

		// verify if the authority is a selected relayer.
		Self::verify_authority(authority_id)?;

		// verify if the signature was originated from the authority.
		let message = [keccak_256("BroadcastPoll".as_bytes()).as_slice(), txid.as_bytes()].concat();
		Self::verify_signature(&message, signature, authority_id)?;

		ValidTransaction::with_tag_prefix("BroadcastPoll")
			.priority(TransactionPriority::MAX)
			.and_provides((authority_id, txid))
			.propagate(true)
			.build()
	}

	/// Verify a fee rate submission.
	pub fn verify_submit_fee_rate(
		fee_rate_submission: &FeeRateSubmission<T::AccountId, BlockNumberFor<T>>,
		signature: &T::Signature,
	) -> TransactionValidity
	where
		<<<T as frame_system::Config>::Block as Block>::Header as Header>::Number: Display,
	{
		let FeeRateSubmission { authority_id, fee_rate, deadline } = fee_rate_submission;

		// verify if the authority is a selected relayer.
		Self::verify_authority(authority_id)?;

		// verify if the deadline is not expired.
		let now = <frame_system::Pallet<T>>::block_number();
		if now > *deadline {
			return Err(InvalidTransaction::Stale.into());
		}

		// verify if the signature was originated from the authority.
		let message = format!("{}:{}", deadline, fee_rate);
		Self::verify_signature(message.as_bytes(), signature, authority_id)?;

		ValidTransaction::with_tag_prefix("FeeRateSubmission")
			.priority(TransactionPriority::MAX)
			.and_provides((authority_id, fee_rate))
			.propagate(true)
			.build()
	}

	/// Verify an outbound requests submission.
	pub fn verify_submit_outbound_requests(
		outbound_request_submission: &OutboundRequestSubmission<T::AccountId>,
		signature: &T::Signature,
	) -> TransactionValidity {
		let OutboundRequestSubmission { authority_id, messages } = outbound_request_submission;

		// verify if the authority is a selected relayer.
		Self::verify_authority(authority_id)?;

		// verify if the signature was originated from the authority.
		let message = format!(
			"{}",
			messages
				.iter()
				.map(|x| array_bytes::bytes2hex("0x", x))
				.collect::<Vec<String>>()
				.concat()
		);
		Self::verify_signature(message.as_bytes(), signature, authority_id)?;

		ValidTransaction::with_tag_prefix("OutboundRequestSubmission")
			.priority(TransactionPriority::MAX)
			.and_provides(authority_id)
			.propagate(true)
			.build()
	}
}
