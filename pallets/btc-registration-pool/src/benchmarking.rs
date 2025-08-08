#![cfg(feature = "runtime-benchmarks")]

use super::*;
use bp_btc_relay::{MigrationSequence, Public};
use frame_benchmarking::v2::*;
use frame_support::traits::SortedMembers;
use frame_system::RawOrigin;
use sp_core::H160;
use sp_std::vec;

const DUMMY_PUBKEY: [u8; 33] = [
	3, 178, 213, 85, 125, 177, 174, 114, 225, 138, 189, 244, 149, 1, 166, 108, 182, 52, 129, 74,
	247, 197, 202, 224, 38, 98, 86, 100, 36, 14, 86, 206, 122,
];

const DUMMY_PUBKEY_2: [u8; 33] = [
	3, 219, 80, 67, 235, 124, 28, 228, 152, 173, 21, 112, 134, 126, 117, 2, 187, 85, 202, 248, 209,
	131, 84, 52, 53, 206, 200, 131, 43, 172, 6, 228, 251,
];

const DUMMY_PUBKEY_3: [u8; 33] = [
	2, 66, 138, 231, 16, 176, 114, 167, 67, 219, 32, 129, 85, 197, 56, 222, 148, 95, 28, 59, 177,
	140, 39, 192, 59, 120, 255, 156, 224, 10, 210, 119, 177,
];
const DUMMY_PUBKEY_4: [u8; 33] = [
	3, 225, 57, 237, 13, 169, 112, 84, 219, 210, 126, 106, 196, 245, 143, 181, 212, 200, 111, 240,
	114, 252, 41, 205, 37, 57, 202, 58, 59, 198, 192, 20, 141,
];
const DUMMY_PUBKEY_5: [u8; 33] = [
	2, 169, 30, 40, 209, 35, 178, 175, 162, 239, 120, 43, 183, 17, 220, 75, 46, 163, 137, 90, 23,
	94, 160, 46, 88, 220, 214, 15, 186, 146, 84, 129, 241,
];

fn setup_normal_state<T: Config>() {
	<ServiceState<T>>::put(MigrationSequence::Normal);
	let current_round = 1u32;
	<CurrentRound<T>>::put(current_round);

	T::Executives::add(&account::<T::AccountId>("executive1", 0, 0));
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
	fn submit_system_vault_key() {
		use crate::VaultKeySubmission;
		use bp_btc_relay::Public;
		use sp_core::H160;

		setup_normal_state::<T>();

		// Setup system vault first
		let current_round = <CurrentRound<T>>::get();
		let system_vault = MultiSigAccount::new(2u32, 3u32);
		<SystemVault<T>>::insert(current_round, system_vault);

		// Prepare data
		let authority: T::AccountId = T::Executives::sorted_members()[0].clone();
		let precompile: T::AccountId = H160::from_low_u64_be(crate::ADDRESS_U64).into();

		let key_submission = VaultKeySubmission {
			authority_id: authority,
			who: precompile,
			pub_key: Public(DUMMY_PUBKEY),
			pool_round: current_round,
		};
		let signature = T::Signature::decode(&mut [0u8; 65].as_ref()).expect("valid sig");

		#[extrinsic_call]
		_(RawOrigin::None, key_submission, signature);
	}

	#[benchmark]
	fn vault_key_presubmission() {
		use crate::VaultKeyPreSubmission;
		use bp_btc_relay::Public;

		setup_normal_state::<T>();

		<MaxPreSubmission<T>>::put(u32::MAX);

		// Prepare data
		let authority: T::AccountId = T::Executives::sorted_members()[0].clone();
		let current_round = <CurrentRound<T>>::get();

		// Create multiple dummy public keys
		let mut pub_keys = vec![
			Public(DUMMY_PUBKEY),
			Public(DUMMY_PUBKEY_2),
			Public(DUMMY_PUBKEY_3),
			Public(DUMMY_PUBKEY_4),
			Public(DUMMY_PUBKEY_5),
		];

		let key_submission =
			VaultKeyPreSubmission { authority_id: authority, pub_keys, pool_round: current_round };
		let signature = T::Signature::decode(&mut [0u8; 65].as_ref()).expect("valid sig");

		#[extrinsic_call]
		_(RawOrigin::None, key_submission, signature);
	}

	#[benchmark]
	fn clear_vault() {
		setup_normal_state::<T>();

		let current_round = <CurrentRound<T>>::get();
		let caller: T::AccountId = account("caller", 0, 0);
		let refund_address_bytes = b"bcrt1qtwjzfmpctpp9g2y7urgjt63jwm9r2xat5pua3g".to_vec();
		let refund_address_bounded: BoundedBitcoinAddress =
			refund_address_bytes.clone().try_into().expect("valid address");

		// Create and register a vault
		let mut relay_target =
			BitcoinRelayTarget::new::<T>(refund_address_bounded.clone(), 2u32, 3u32);

		// Set up a generated vault address
		let vault_address_bytes = b"bcrt1q9y7q8pls5z5qljgav7v65ma9jsw94pplxmh39q".to_vec();
		let vault_address_bounded: BoundedBitcoinAddress =
			vault_address_bytes.clone().try_into().expect("valid vault address");
		relay_target.set_vault_address(vault_address_bounded.clone());

		// Insert the registration and bonded data
		<RegistrationPool<T>>::insert(current_round, &caller, relay_target);
		<BondedVault<T>>::insert(current_round, &vault_address_bounded, caller.clone());
		<BondedRefund<T>>::insert(current_round, &refund_address_bounded, vec![caller]);

		#[extrinsic_call]
		_(RawOrigin::Root, vault_address_bytes);
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
