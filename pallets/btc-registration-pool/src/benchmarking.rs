#![cfg(feature = "runtime-benchmarks")]

use super::*;
use bp_btc_relay::MigrationSequence;
use frame_benchmarking::v2::*;
use frame_support::traits::SortedMembers;
use frame_system::RawOrigin;
use sp_core::H160;

const DUMMY_PUBKEY: [u8; 33] = [
	3, 178, 213, 85, 125, 177, 174, 114, 225, 138, 189, 244, 149, 1, 166, 108, 182, 52, 129, 74,
	247, 197, 202, 224, 38, 98, 86, 100, 36, 14, 86, 206, 122,
];

fn setup_normal_state<T: Config>() {
	<ServiceState<T>>::put(MigrationSequence::Normal);
	let current_round = 1u32;
	<CurrentRound<T>>::put(current_round);
}

#[benchmarks(
	where
		H160: Into<T::AccountId>,
		Call<T>: parity_scale_codec::Decode,
)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn request_set_refund() {
		setup_normal_state::<T>();

		let caller: T::AccountId = account("caller", 0, 0);

		// Setup: Create existing registration for the user
		let old_refund_address = b"bcrt1qtwjzfmpctpp9g2y7urgjt63jwm9r2xat5pua3g".to_vec();

		let old_refund_address_bounded =
			old_refund_address.clone().try_into().expect("Valid address");

		// Create BitcoinRelayTarget with proper initialization
		let relay_target = BitcoinRelayTarget::new::<T>(old_refund_address_bounded, 2u32, 3u32);

		<RegistrationPool<T>>::insert(1u32, &caller, relay_target);

		// New refund address (must be different from old one)
		let new_refund_address = b"bcrt1q9y7q8pls5z5qljgav7v65ma9jsw94pplxmh39q".to_vec();

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), new_refund_address);
	}

	#[benchmark]
	fn request_vault() {
		// TODO: no available presubmission case
		use bp_btc_relay::Public;

		setup_normal_state::<T>();

		let caller: T::AccountId = account("caller", 0, 0);
		let refund_address = b"bcrt1qtwjzfmpctpp9g2y7urgjt63jwm9r2xat5pua3g".to_vec();

		// Setup PreSubmittedPubKeys for executives to test the full flow
		let executives = T::Executives::sorted_members();
		for (i, executive) in executives.iter().enumerate() {
			// Create dummy public keys (33 bytes each)
			let mut pub_key_bytes = [0u8; 33];
			pub_key_bytes[0] = 0x02; // Compressed public key prefix
			pub_key_bytes[32] = i as u8; // Make each key unique

			let pub_key = Public(pub_key_bytes);

			// Add to PreSubmittedPubKeys for the current round
			<PreSubmittedPubKeys<T>>::mutate(1u32, executive, |keys| {
				keys.insert(pub_key);
			});
		}

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), refund_address);
	}

	#[benchmark]
	fn request_system_vault() {
		setup_normal_state::<T>();

		#[extrinsic_call]
		_(RawOrigin::Root, false);
	}

	#[benchmark]
	fn submit_vault_key() {
		use crate::{BitcoinRelayTarget, VaultKeySubmission};
		use bp_btc_relay::Public;

		setup_normal_state::<T>();

		// Prepare data
		let authority: T::AccountId = T::Executives::sorted_members()[0].clone();

		let who: T::AccountId = account("user", 0, 0);
		let refund_address_bytes = b"bcrt1qtwjzfmpctpp9g2y7urgjt63jwm9r2xat5pua3g".to_vec();
		let refund_address_bounded =
			refund_address_bytes.clone().try_into().expect("valid address");

		// Insert relay target in pending state
		let relay_target = BitcoinRelayTarget::new::<T>(refund_address_bounded, 2u32, 3u32);
		<RegistrationPool<T>>::insert(<CurrentRound<T>>::get(), &who, relay_target);

		let key_submission = VaultKeySubmission {
			authority_id: authority,
			who: who.clone(),
			pub_key: Public(DUMMY_PUBKEY),
			pool_round: <CurrentRound<T>>::get(),
		};
		let signature = T::Signature::decode(&mut [0u8; 65].as_ref()).expect("valid sig");

		#[extrinsic_call]
		_(RawOrigin::None, key_submission, signature);
	}

	#[benchmark]
	fn migration_control() {
		<ServiceState<T>>::put(MigrationSequence::SetExecutiveMembers);

		#[extrinsic_call]
		_(RawOrigin::Root);
	}

	#[benchmark]
	fn drop_previous_round() {
		let current_round = 2u32;
		let round_to_drop = 1u32;
		<CurrentRound<T>>::put(current_round);

		#[extrinsic_call]
		_(RawOrigin::Root, round_to_drop);
	}

	#[benchmark]
	fn set_max_presubmission() {
		let new_max = 200u32;

		#[extrinsic_call]
		_(RawOrigin::Root, new_max);
	}

	#[benchmark]
	fn set_multi_sig_ratio() {
		let new_ratio = sp_runtime::Percent::from_percent(75);

		#[extrinsic_call]
		_(RawOrigin::Root, new_ratio);
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
