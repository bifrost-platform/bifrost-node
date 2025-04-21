use super::pallet::*;
use crate::{
	BTCTransaction, BroadcastSubmission, FeeRateSubmission, SocketMessagesSubmission, UtxoStatus,
	UtxoSubmission,
};
use bp_btc_relay::{
	blaze::{ScoredUtxo, SelectionStrategy, UtxoInfoWithSize},
	traits::{BlazeManager, SocketQueueManager},
	Hash, Psbt, UnboundedBytes,
};
use bp_staking::traits::Authorities;
use frame_support::{
	ensure,
	pallet_prelude::{
		InvalidTransaction, TransactionPriority, TransactionValidity, ValidTransaction,
	},
};
use frame_system::pallet_prelude::BlockNumberFor;
use parity_scale_codec::Encode;
use scale_info::prelude::{format, string::String};
use sp_core::{Get, H256};
use sp_io::hashing::keccak_256;
use sp_runtime::{
	traits::{Block, Header, Verify},
	BoundedBTreeMap, BoundedVec, DispatchError,
};
use sp_std::{fmt::Display, vec, vec::Vec};

impl<T: Config> BlazeManager<T> for Pallet<T> {
	fn is_activated() -> bool {
		<IsActivated<T>>::get()
	}

	fn get_utxos() -> Vec<UtxoInfoWithSize> {
		<Utxos<T>>::iter()
			.filter_map(
				|(_, utxo)| {
					if utxo.status == UtxoStatus::Available {
						Some(utxo.inner)
					} else {
						None
					}
				},
			)
			.collect()
	}

	fn clear_utxos() {
		let utxos = <Utxos<T>>::iter().collect::<Vec<_>>();
		for (hash, utxo) in utxos {
			if utxo.status != UtxoStatus::Used {
				<Utxos<T>>::remove(hash);
			}
		}
	}

	fn lock_utxos(txid: &H256, inputs: &Vec<UtxoInfoWithSize>) -> Result<(), DispatchError> {
		for input in inputs {
			match <Utxos<T>>::get(&input.hash) {
				Some(mut utxo) => {
					utxo.status = UtxoStatus::Locked;
					<Utxos<T>>::insert(input.hash, utxo);
				},
				None => return Err(Error::<T>::UtxoDNE.into()),
			}
		}
		<PendingTxs<T>>::insert(
			txid,
			BTCTransaction { inputs: inputs.clone(), voters: BoundedVec::default() },
		);
		Ok(())
	}

	fn unlock_utxos(txid: &H256) -> Result<(), DispatchError> {
		match <PendingTxs<T>>::take(txid) {
			Some(tx) => {
				for input in &tx.inputs {
					match <Utxos<T>>::get(&input.hash) {
						Some(mut utxo) => {
							utxo.status = UtxoStatus::Available;
							<Utxos<T>>::insert(input.hash, utxo);
						},
						None => return Err(Error::<T>::UtxoDNE.into()),
					}
				}
			},
			None => return Err(Error::<T>::UnknownTransaction.into()),
		};
		Ok(())
	}

	fn extract_utxos_from_psbt(psbt: &Psbt) -> Result<Vec<UtxoInfoWithSize>, DispatchError> {
		let mut inputs = vec![];
		for (i, input) in psbt.inputs.iter().enumerate() {
			let txin = &psbt.unsigned_tx.input[i];
			let mut txid = txin.previous_output.txid.as_byte_array().clone();
			txid.reverse();
			let vout = txin.previous_output.vout;

			let amount = if let Some(ref utxo) = input.witness_utxo {
				utxo.value
			} else if let Some(ref tx) = input.non_witness_utxo {
				tx.output[vout as usize].value
			} else {
				unreachable!()
			};

			let hash = H256::from_slice(
				keccak_256(&Encode::encode(&(H256::from(txid), vout as u32, amount.to_sat())))
					.as_ref(),
			);
			match <Utxos<T>>::get(&hash) {
				Some(utxo) => inputs.push(utxo.inner.clone()),
				None => {
					return Err(Error::<T>::UtxoDNE.into());
				},
			}
		}
		Ok(inputs)
	}

	fn get_outbound_pool() -> Vec<UnboundedBytes> {
		<OutboundPool<T>>::get()
	}

	fn clear_outbound_pool(targets: Vec<UnboundedBytes>) {
		<OutboundPool<T>>::mutate(|x| {
			x.retain(|x| !targets.contains(x));
		});
	}

	fn try_fee_rate_finalization(n: BlockNumberFor<T>) -> Option<(u64, u64)> {
		let mut submitted_fee_rates = <FeeRates<T>>::get();
		// remove expired fee rates
		submitted_fee_rates.retain(|_, (_, _, expires_at)| n <= *expires_at);
		<FeeRates<T>>::put(submitted_fee_rates.clone());

		// check majority
		if submitted_fee_rates.len() as u32 >= T::Relayers::majority() {
			// choose the median fee rate
			let mut fee_rates = Vec::with_capacity(submitted_fee_rates.len());
			let mut lt_fee_rates = Vec::with_capacity(submitted_fee_rates.len());
			for (_, (lt_fee_rate, fee_rate, _)) in &submitted_fee_rates {
				lt_fee_rates.push(*lt_fee_rate);
				fee_rates.push(*fee_rate);
			}

			fee_rates.sort();
			lt_fee_rates.sort();

			let median_index = fee_rates.len() / 2;
			let median_fee_rate = fee_rates[median_index];
			let median_lt_fee_rate = lt_fee_rates[median_index];

			if median_fee_rate >= median_lt_fee_rate {
				return Some((median_lt_fee_rate, median_fee_rate));
			}
		}
		None
	}

	fn clear_fee_rates() {
		<FeeRates<T>>::put(BoundedBTreeMap::new());
	}

	fn select_coins(
		pool: Vec<ScoredUtxo>,
		target: u64,
		cost_of_change: u64,
		max_selection_weight: u64,
		max_tries: usize,
		change_target: u64,
	) -> Option<(Vec<UtxoInfoWithSize>, SelectionStrategy)> {
		match Self::select_coins_bnb(
			pool.clone(),
			target,
			cost_of_change,
			max_selection_weight,
			max_tries,
		) {
			Some(selected) => Some(selected),
			None => Self::select_coins_knapsack(pool, target, change_target, max_selection_weight),
		}
	}

	fn handle_tolerance_counter(is_increase: bool) {
		let current_counter = <ToleranceCounter<T>>::get();
		let next_counter = if is_increase {
			current_counter.saturating_add(1)
		} else {
			current_counter.saturating_sub(1)
		};
		if next_counter > T::ToleranceThreshold::get() {
			<IsActivated<T>>::put(false);
			<ToleranceCounter<T>>::put(0);
			Self::deposit_event(Event::ActivationSet { is_activated: false });
		} else if current_counter != next_counter {
			<ToleranceCounter<T>>::put(next_counter);
			Self::deposit_event(Event::ToleranceCounterUpdated { new: next_counter });
		}
	}

	fn ensure_activation(is_activated: bool) -> Result<(), DispatchError> {
		ensure!(Self::is_activated() == is_activated, Error::<T>::InvalidActivationState);
		Ok(())
	}

	fn try_fee_rate_finalization() -> Option<u64> {
		let submitted_fee_rates = <FeeRates<T>>::get();

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
	/// Coin selection by BNB.
	fn select_coins_bnb(
		mut pool: Vec<ScoredUtxo>,
		target: u64,
		cost_of_change: u64,
		max_selection_weight: u64,
		max_tries: usize,
	) -> Option<(Vec<UtxoInfoWithSize>, SelectionStrategy)> {
		pool.sort_by(|a, b| {
			b.effective_value
				.cmp(&a.effective_value)
				.then((a.fee - a.long_term_fee).cmp(&(b.fee - b.long_term_fee)))
		});

		let mut best_selection = Vec::new();
		let mut best_waste = u64::MAX;

		let mut curr_selection = Vec::new();

		fn dfs(
			index: usize,
			tries: &mut usize,
			pool: &[ScoredUtxo],
			curr_selection: &mut Vec<ScoredUtxo>,
			curr_value: u64,
			curr_weight: u64,
			target: u64,
			cost_of_change: u64,
			max_selection_weight: u64,
			best_selection: &mut Vec<UtxoInfoWithSize>,
			best_waste: &mut u64,
			max_tries: usize,
		) {
			if *tries >= max_tries {
				return;
			}
			*tries += 1;

			let available_remaining = pool[index..].iter().map(|x| x.effective_value).sum::<u64>();
			if curr_value + available_remaining < target {
				return;
			}
			if curr_value > target + cost_of_change {
				return;
			}
			if curr_weight > max_selection_weight {
				return;
			}
			if curr_value >= target {
				let waste = curr_value - target
					+ curr_selection.iter().map(|x| x.fee - x.long_term_fee).sum::<u64>();
				if waste < *best_waste {
					*best_waste = waste;
					*best_selection = curr_selection.iter().map(|x| x.utxo.clone()).collect();
				}
				return;
			}

			if index >= pool.len() {
				return;
			}

			curr_selection.push(pool[index].clone());
			dfs(
				index + 1,
				tries,
				pool,
				curr_selection,
				curr_value + pool[index].effective_value,
				curr_weight + pool[index].utxo.input_vbytes,
				target,
				cost_of_change,
				max_selection_weight,
				best_selection,
				best_waste,
				max_tries,
			);
			curr_selection.pop();

			dfs(
				index + 1,
				tries,
				pool,
				curr_selection,
				curr_value,
				curr_weight,
				target,
				cost_of_change,
				max_selection_weight,
				best_selection,
				best_waste,
				max_tries,
			);
		}

		let mut tries = 0;
		dfs(
			0,
			&mut tries,
			&pool,
			&mut curr_selection,
			0,
			0,
			target,
			cost_of_change,
			max_selection_weight,
			&mut best_selection,
			&mut best_waste,
			max_tries,
		);

		if best_selection.is_empty() {
			None
		} else {
			Some((best_selection, SelectionStrategy::Bnb))
		}
	}

	/// Coin selection by knapsack solver.
	fn select_coins_knapsack(
		pool: Vec<ScoredUtxo>,
		target: u64,
		change_target: u64,
		max_weight: u64,
	) -> Option<(Vec<UtxoInfoWithSize>, SelectionStrategy)> {
		let mut applicable = vec![];
		let mut total_lower = 0u64;
		let mut lowest_larger: Option<&UtxoInfoWithSize> = None;

		for scored in &pool {
			if scored.effective_value >= target && scored.effective_value <= target + change_target
			{
				return Some((vec![scored.utxo.clone()], SelectionStrategy::Knapsack));
			} else if scored.effective_value < target + change_target {
				applicable.push(scored);
				total_lower += scored.effective_value;
			} else if lowest_larger.map_or(true, |x| scored.utxo.amount < x.amount) {
				lowest_larger = Some(&scored.utxo);
			}
		}

		if total_lower == target {
			return Some((
				applicable.iter().map(|x| x.utxo.clone()).collect(),
				SelectionStrategy::Knapsack,
			));
		}
		if total_lower < target {
			return lowest_larger.map(|x| (vec![x.clone()], SelectionStrategy::Knapsack));
		}

		applicable.sort_by(|a, b| b.effective_value.cmp(&a.effective_value));

		let mut best = vec![];
		let mut best_total = u64::MAX;

		for i in 0..applicable.len() {
			let mut selected = vec![];
			let mut total = 0;
			let mut weight = 0;

			for j in i..applicable.len() {
				let scored = applicable[j];
				total += scored.effective_value;
				weight += scored.utxo.input_vbytes;
				selected.push(scored.utxo.clone());

				if total >= target && weight <= max_weight {
					if total < best_total {
						best_total = total;
						best = selected.clone();
					}
					break;
				}
			}
		}

		if !best.is_empty() {
			Some((best, SelectionStrategy::Knapsack))
		} else {
			lowest_larger.map(|x| (vec![x.clone()], SelectionStrategy::Knapsack))
		}
	}

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
			.and_provides((authority_id, signature))
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
			.and_provides((authority_id, txid, signature))
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
		let FeeRateSubmission { authority_id, lt_fee_rate, fee_rate, deadline } =
			fee_rate_submission;

		// verify if the authority is a selected relayer.
		Self::verify_authority(authority_id)?;

		// verify if the deadline is not expired.
		let now = <frame_system::Pallet<T>>::block_number();
		if now > *deadline {
			return Err(InvalidTransaction::Stale.into());
		}

		// verify if the signature was originated from the authority.
		let message = format!("{}:{}:{}", deadline, lt_fee_rate, fee_rate);
		Self::verify_signature(message.as_bytes(), signature, authority_id)?;

		ValidTransaction::with_tag_prefix("FeeRateSubmission")
			.priority(TransactionPriority::MAX)
			.and_provides((authority_id, lt_fee_rate, fee_rate, signature))
			.propagate(true)
			.build()
	}

	/// Verify an outbound requests submission.
	pub fn verify_submit_outbound_requests(
		outbound_request_submission: &SocketMessagesSubmission<T::AccountId>,
		signature: &T::Signature,
	) -> TransactionValidity {
		let SocketMessagesSubmission { authority_id, messages } = outbound_request_submission;

		// verify if the authority is a selected relayer.
		Self::verify_authority(authority_id)?;

		// verify if the signature was originated from the authority.
		let message = messages
			.iter()
			.map(|x| array_bytes::bytes2hex("0x", x))
			.collect::<Vec<String>>()
			.concat();
		Self::verify_signature(message.as_bytes(), signature, authority_id)?;

		ValidTransaction::with_tag_prefix("OutboundRequestSubmission")
			.priority(TransactionPriority::MAX)
			.and_provides((authority_id, signature))
			.propagate(true)
			.build()
	}

	pub fn verify_remove_outbound_messages(
		remove_submission: &SocketMessagesSubmission<T::AccountId>,
		signature: &T::Signature,
	) -> TransactionValidity {
		let SocketMessagesSubmission { authority_id, messages } = remove_submission;

		// verify if the authority is psbt manager.
		T::SocketQueue::verify_authority(authority_id)?;

		// verify if the signature was originated from psbt manager.
		let message = messages
			.iter()
			.map(|x| array_bytes::bytes2hex("0x", x))
			.collect::<Vec<String>>()
			.concat();

		Self::verify_signature(message.as_bytes(), signature, authority_id)?;

		ValidTransaction::with_tag_prefix("RemoveOutboundMessagesSubmission")
			.priority(TransactionPriority::MAX)
			.and_provides((authority_id, signature))
			.propagate(true)
			.build()
	}
}
