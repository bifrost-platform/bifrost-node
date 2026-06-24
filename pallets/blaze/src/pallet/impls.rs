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
use bp_cccp::traits::SocketVerifier;
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
	fn replace_authority(old: &T::AccountId, new: &T::AccountId) {
		// Migrate unconfirmed UTXO count from old to new authority.
		let old_count = <UnconfirmedUtxoCount<T>>::take(old);
		if old_count > 0 {
			<UnconfirmedUtxoCount<T>>::insert(new, old_count);
		}

		// Replace authority in unconfirmed UTXOs (still accumulating votes)
		<Utxos<T>>::iter().for_each(|(hash, mut utxo)| {
			if utxo.status == UtxoStatus::Unconfirmed && utxo.voters.contains(old) {
				utxo.replace_authority(old, new);
				<Utxos<T>>::insert(hash, utxo);
			}
		});

		// Replace authority in pending PSBTs (waiting for broadcast confirmation votes)
		<PendingTxs<T>>::iter().for_each(|(txid, mut tx)| {
			if tx.voters.contains(old) {
				tx.replace_authority(old, new);
				<PendingTxs<T>>::insert(txid, tx);
			}
		});

		// Replace authority in fee rates map
		let mut fee_rates = <FeeRates<T>>::get();
		if let Some(val) = fee_rates.remove(old) {
			fee_rates
				.try_insert(new.clone(), val)
				.expect("Should not fail as we just removed an element");
			<FeeRates<T>>::put(fee_rates);
		}
	}

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
				if utxo.status == UtxoStatus::Unconfirmed {
					if let Some(submitter) = utxo.voters.first() {
						<UnconfirmedUtxoCount<T>>::mutate(submitter, |c| *c = c.saturating_sub(1));
					}
				}
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

			let target_txid = H256::from(txid);
			let target_vout = vout as u32;
			let target_amount = amount.to_sat();

			// Look up UTXO by (txid, vout, amount) since the hash now includes
			// the address which is not available from the PSBT.
			let utxo = <Utxos<T>>::iter()
				.find(|(_, u)| {
					u.inner.txid == target_txid
						&& u.inner.vout == target_vout
						&& u.inner.amount == target_amount
				})
				.map(|(_, u)| u.inner.clone())
				.ok_or(Error::<T>::UtxoDNE)?;

			inputs.push(utxo);
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
			b.effective_value.cmp(&a.effective_value).then(
				a.fee
					.saturating_sub(a.long_term_fee)
					.cmp(&b.fee.saturating_sub(b.long_term_fee)),
			)
		});

		let mut best_selection = Vec::new();
		let mut best_waste = u64::MAX;

		// precompute suffix sums for O(1) remaining value lookup
		let mut suffix_sums = vec![0u64; pool.len() + 1];
		for i in (0..pool.len()).rev() {
			suffix_sums[i] = suffix_sums[i + 1].saturating_add(pool[i].effective_value);
		}

		let mut curr_selection = Vec::new();

		fn dfs(
			index: usize,
			tries: &mut usize,
			pool: &[ScoredUtxo],
			suffix_sums: &[u64],
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

			if curr_value + suffix_sums[index] < target {
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
					+ curr_selection
						.iter()
						.map(|x| x.fee.saturating_sub(x.long_term_fee))
						.sum::<u64>();
				if waste < *best_waste {
					*best_waste = waste;
					*best_selection = curr_selection.iter().map(|x| x.utxo.clone()).collect();
				}
				return;
			}
			// perfect match found, no need to explore further
			if *best_waste == 0 {
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
				suffix_sums,
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
				suffix_sums,
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
			&suffix_sums,
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
	///
	/// Modeled after Bitcoin Core's `KnapsackSolver`. The stochastic
	/// `ApproximateBestSubset` is replaced with a deterministic bounded DFS
	/// suitable for on-chain (consensus-critical) execution.
	fn select_coins_knapsack(
		pool: Vec<ScoredUtxo>,
		target: u64,
		change_target: u64,
		max_weight: u64,
	) -> Option<(Vec<UtxoInfoWithSize>, SelectionStrategy)> {
		// Phase 1 — categorize UTXOs (mirrors Bitcoin Core's KnapsackSolver).
		let mut applicable: Vec<ScoredUtxo> = vec![];
		let mut lowest_larger: Option<ScoredUtxo> = None;
		let mut best_exact: Option<ScoredUtxo> = None;

		for scored in pool.iter() {
			if scored.effective_value >= target
				&& scored.effective_value <= target + change_target
				&& scored.utxo.input_vbytes <= max_weight
			{
				if best_exact.as_ref().map_or(true, |x| scored.effective_value < x.effective_value)
				{
					best_exact = Some(scored.clone());
				}
			} else if scored.effective_value < target + change_target {
				applicable.push(scored.clone());
			} else if lowest_larger
				.as_ref()
				.map_or(true, |x| scored.effective_value < x.effective_value)
			{
				lowest_larger = Some(scored.clone());
			}
		}

		// Single UTXO covers target within acceptable change range.
		if let Some(exact) = best_exact {
			return Some((vec![exact.utxo], SelectionStrategy::Knapsack));
		}

		let total_lower: u64 = applicable.iter().map(|x| x.effective_value).sum();

		// Not enough value even with all applicable UTXOs — fall back to
		// the smallest single UTXO that exceeds target.
		if total_lower < target {
			return lowest_larger
				.filter(|x| x.utxo.input_vbytes <= max_weight)
				.map(|x| (vec![x.utxo], SelectionStrategy::Knapsack));
		}

		// Phase 2 — bounded DFS subset search.
		// Replaces Bitcoin Core's stochastic ApproximateBestSubset with a
		// deterministic branch-and-bound that explores non-contiguous subsets.
		applicable.sort_by(|a, b| b.effective_value.cmp(&a.effective_value));

		let mut suffix_sums = vec![0u64; applicable.len() + 1];
		for i in (0..applicable.len()).rev() {
			suffix_sums[i] = suffix_sums[i + 1].saturating_add(applicable[i].effective_value);
		}

		let mut best_selection: Vec<UtxoInfoWithSize> = Vec::new();
		let mut best_total = u64::MAX;
		let mut curr_selection: Vec<ScoredUtxo> = Vec::new();

		fn dfs(
			index: usize,
			tries: &mut usize,
			applicable: &[ScoredUtxo],
			suffix_sums: &[u64],
			curr_selection: &mut Vec<ScoredUtxo>,
			curr_value: u64,
			curr_weight: u64,
			target: u64,
			max_weight: u64,
			best_selection: &mut Vec<UtxoInfoWithSize>,
			best_total: &mut u64,
			max_tries: usize,
		) {
			if *tries >= max_tries {
				return;
			}
			*tries += 1;

			// Prune: cannot reach target with remaining UTXOs.
			if curr_value + suffix_sums[index] < target {
				return;
			}
			// Prune: weight already exceeded.
			if curr_weight > max_weight {
				return;
			}
			// Solution found — record if it is the best so far.
			if curr_value >= target {
				if curr_value < *best_total {
					*best_total = curr_value;
					*best_selection = curr_selection.iter().map(|x| x.utxo.clone()).collect();
				}
				return;
			}
			// Optimal (exact match) already found.
			if *best_total == target {
				return;
			}

			if index >= applicable.len() {
				return;
			}

			// Include current UTXO.
			curr_selection.push(applicable[index].clone());
			dfs(
				index + 1,
				tries,
				applicable,
				suffix_sums,
				curr_selection,
				curr_value + applicable[index].effective_value,
				curr_weight + applicable[index].utxo.input_vbytes,
				target,
				max_weight,
				best_selection,
				best_total,
				max_tries,
			);
			curr_selection.pop();

			// Exclude current UTXO.
			dfs(
				index + 1,
				tries,
				applicable,
				suffix_sums,
				curr_selection,
				curr_value,
				curr_weight,
				target,
				max_weight,
				best_selection,
				best_total,
				max_tries,
			);
		}

		let mut tries = 0;
		dfs(
			0,
			&mut tries,
			&applicable,
			&suffix_sums,
			&mut curr_selection,
			0,
			0,
			target,
			max_weight,
			&mut best_selection,
			&mut best_total,
			100_000,
		);

		// Phase 3 — compare DFS result with lowest_larger.
		// A single larger UTXO may be preferable (fewer inputs = smaller tx).
		if !best_selection.is_empty() {
			if let Some(ref larger) = lowest_larger {
				if larger.utxo.input_vbytes <= max_weight && larger.effective_value <= best_total {
					return Some((vec![larger.utxo.clone()], SelectionStrategy::Knapsack));
				}
			}
			Some((best_selection, SelectionStrategy::Knapsack))
		} else {
			lowest_larger
				.filter(|x| x.utxo.input_vbytes <= max_weight)
				.map(|x| (vec![x.utxo], SelectionStrategy::Knapsack))
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

		// reject if the number of UTXOs exceeds the per-submission limit.
		if utxos.len() > crate::MAX_UTXOS_PER_SUBMISSION {
			return InvalidTransaction::ExhaustsResources.into();
		}

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
							keccak_256(&Encode::encode(&(
								x.txid,
								x.vout,
								x.amount,
								x.address.clone(),
							)))
							.as_ref(),
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

		// reject if the number of messages exceeds the per-submission limit.
		if messages.len() > crate::MAX_SOCKET_MESSAGES_PER_SUBMISSION {
			return InvalidTransaction::ExhaustsResources.into();
		}

		// reject if any individual message exceeds the configured size limit.
		let max_bytes = T::SocketQueue::get_max_socket_message_bytes() as usize;
		if messages.iter().any(|m| m.len() > max_bytes) {
			return InvalidTransaction::ExhaustsResources.into();
		}

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
