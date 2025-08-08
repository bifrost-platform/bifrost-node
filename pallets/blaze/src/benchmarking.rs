#![cfg(feature = "runtime-benchmarks")]

use super::*;
use bp_btc_relay::{blaze::UtxoInfo, traits::SocketQueueManager, UnboundedBytes};
use frame_benchmarking::v2::*;
use frame_system::RawOrigin;
use parity_scale_codec::Decode;
use sp_core::H256;
use sp_std::vec;

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn set_activation() {
		let toggle = !<IsActivated<T>>::get();

		#[extrinsic_call]
		_(RawOrigin::Root, toggle);
	}

	#[benchmark]
	fn submit_utxos() {
		let authority: T::AccountId = account("authority", 0, 0);
		let utxos = vec![UtxoInfo {
			txid: H256::from([1u8; 32]),
			vout: 0,
			amount: 100000,
			address: Default::default(),
		}];
		let utxo_submission = UtxoSubmission { authority_id: authority, utxos };
		// Create a dummy signature
		let signature = T::Signature::decode(&mut [0u8; 65].as_ref()).expect("Valid signature");

		// Activate the pallet first
		<IsActivated<T>>::put(true);

		#[extrinsic_call]
		_(RawOrigin::None, utxo_submission, signature);
	}

	#[benchmark]
	fn broadcast_poll() {
		let authority: T::AccountId = account("authority", 0, 0);
		let txid = H256::from([2u8; 32]);

		// Setup: Create a pending transaction first
		let pending_tx = BTCTransaction { inputs: vec![], voters: Default::default() };
		<PendingTxs<T>>::insert(&txid, pending_tx);

		let broadcast_submission = BroadcastSubmission { authority_id: authority, txid };
		let signature = T::Signature::decode(&mut [0u8; 65].as_ref()).expect("Valid signature");

		// Activate the pallet first
		<IsActivated<T>>::put(true);

		#[extrinsic_call]
		_(RawOrigin::None, broadcast_submission, signature);
	}

	#[benchmark]
	fn submit_fee_rate() {
		let authority: T::AccountId = account("authority", 0, 0);
		let current_block = frame_system::Pallet::<T>::block_number();
		let deadline = current_block + 100u32.into();

		let fee_rate_submission =
			FeeRateSubmission { authority_id: authority, lt_fee_rate: 10, fee_rate: 10, deadline };
		let signature = T::Signature::decode(&mut [0u8; 65].as_ref()).expect("Valid signature");

		<IsActivated<T>>::put(true);
		T::SocketQueue::set_max_fee_rate(u64::MAX);

		#[extrinsic_call]
		_(RawOrigin::None, fee_rate_submission, signature);
	}

	#[benchmark]
	fn submit_outbound_requests() {
		let authority: T::AccountId = account("authority", 0, 0);
		let messages = vec![UnboundedBytes::from(vec![1, 2, 3, 4])];

		let outbound_request_submission =
			SocketMessagesSubmission { authority_id: authority, messages };
		let signature = T::Signature::decode(&mut [0u8; 65].as_ref()).expect("Valid signature");

		#[extrinsic_call]
		_(RawOrigin::None, outbound_request_submission, signature);
	}

	#[benchmark]
	fn force_push_utxos() {
		let utxos = vec![UtxoInfo {
			txid: H256::from([1u8; 32]),
			vout: 0,
			amount: 100000,
			address: Default::default(),
		}];

		// Deactivate the pallet first (required for force_push_utxos)
		<IsActivated<T>>::put(false);

		#[extrinsic_call]
		_(RawOrigin::Root, utxos);
	}

	#[benchmark]
	fn remove_outbound_messages() {
		let authority: T::AccountId = account("authority", 0, 0);
		let messages = vec![UnboundedBytes::from(vec![1, 2, 3, 4])];

		// Setup: Add messages to outbound pool first
		<OutboundPool<T>>::put(messages.clone());

		let remove_submission = SocketMessagesSubmission { authority_id: authority, messages };
		let signature = T::Signature::decode(&mut [0u8; 65].as_ref()).expect("Valid signature");

		// Deactivate the pallet first (required for remove_outbound_messages)
		<IsActivated<T>>::put(false);

		#[extrinsic_call]
		_(RawOrigin::None, remove_submission, signature);
	}

	impl_benchmark_test_suite!(Pallet, mock::new_test_ext(), mock::Test);
}
