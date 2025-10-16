#![cfg(feature = "runtime-benchmarks")]

use super::*;
use bp_btc_relay::{traits::PoolManager, Hash, MigrationSequence, Psbt};
use frame_benchmarking::v2::*;
use frame_support::pallet_prelude::DispatchError;
use frame_system::RawOrigin;
use hex::FromHex;

const DUMMY_UNSIGNED_PSBT_STR: &str = "70736274ff01007d0200000001fe6cf7606b61da1e3a380a0e18891128cb9664dee4cfc4218736174cebe22dd40000000000fdffffff02602e720000000000220020b9f7c13f0cb179daa4ee63ef47c72787a8db1b239ed4d213a38f2c65022b1dc5e0932200000000001600148114cec01b43d48f953a503948f5267165ba76ad00000000000100cf02000000036c0c0f3d8c1591901c8175f9c1d2a9640ef73f17f3859b6282d039cb0397da020100000000fdffffffd548c09b87ba0e57bf3708a42b42b06ede64224ca49a7621db751cc8631967590100000000fdffffffa04edd71b9b8dec9258240f12518b5a2451e962df77c93bad7051aaa34505f630100000000fdffffff02809698000000000022002080b89fa035c251d012e7ba2c6bbe6b0948573fac9c7a243f4a45339abb86caf3bc832301000000001600143ff71794fe168a9514f80e274a7f11046e55eb8bae502c0001012b809698000000000022002080b89fa035c251d012e7ba2c6bbe6b0948573fac9c7a243f4a45339abb86caf301056953210200d16a17d43c25ac12e722a5911666bf0ed143c78d14990241fa5d86158fb9262102010a57a1988a7cea118a0a3e3a81686e29bf93761ea4e9ddd988fff28d6f3aa921021436d8bab43f21c8b74522af3c5a28c4366873efd2c1afe951a314ef2b4cbb1d53ae22060200d16a17d43c25ac12e722a5911666bf0ed143c78d14990241fa5d86158fb926043d937de7220602010a57a1988a7cea118a0a3e3a81686e29bf93761ea4e9ddd988fff28d6f3aa90419e3d0ec2206021436d8bab43f21c8b74522af3c5a28c4366873efd2c1afe951a314ef2b4cbb1d04875a65c400010169532102ece3a9b4c4e42811c4b9d424d76ba4ffeda5e6590d9f6144be1175a0bd54dc0b2103547cb2686e9b53e81bdbe1b2b8a0b5b494cfa05223f5e105fe9364bfbb3aa05f2103b238f9c7bbee00e4e9b3df445ea751a77fe5e4d0eca0f74985676e4a93759c4053ae220202ece3a9b4c4e42811c4b9d424d76ba4ffeda5e6590d9f6144be1175a0bd54dc0b0400378953220203547cb2686e9b53e81bdbe1b2b8a0b5b494cfa05223f5e105fe9364bfbb3aa05f0417edfdb4220203b238f9c7bbee00e4e9b3df445ea751a77fe5e4d0eca0f74985676e4a93759c400401a8c9750000";
const DUMMY_SIGNED_PSBT_STR: &str = "70736274ff01007d0200000001fe6cf7606b61da1e3a380a0e18891128cb9664dee4cfc4218736174cebe22dd40000000000fdffffff02602e720000000000220020b9f7c13f0cb179daa4ee63ef47c72787a8db1b239ed4d213a38f2c65022b1dc5e0932200000000001600148114cec01b43d48f953a503948f5267165ba76ad00000000000100cf02000000036c0c0f3d8c1591901c8175f9c1d2a9640ef73f17f3859b6282d039cb0397da020100000000fdffffffd548c09b87ba0e57bf3708a42b42b06ede64224ca49a7621db751cc8631967590100000000fdffffffa04edd71b9b8dec9258240f12518b5a2451e962df77c93bad7051aaa34505f630100000000fdffffff02809698000000000022002080b89fa035c251d012e7ba2c6bbe6b0948573fac9c7a243f4a45339abb86caf3bc832301000000001600143ff71794fe168a9514f80e274a7f11046e55eb8bae502c0001012b809698000000000022002080b89fa035c251d012e7ba2c6bbe6b0948573fac9c7a243f4a45339abb86caf322020200d16a17d43c25ac12e722a5911666bf0ed143c78d14990241fa5d86158fb92647304402205940e2bc3adec4e4c6f937052efaf30ddf3492bc6d0ca83bfc796485ac8246d702205dbaeca826d63ea45906ef6e729613e6e3f02a162ae069d9a1913e5e2614446201220202010a57a1988a7cea118a0a3e3a81686e29bf93761ea4e9ddd988fff28d6f3aa94830450221008a5bbc3390eb8895f4b6c15848b3c18fcb16504498fd58ab0cf6fa3ed1cdb1ad022061dc393ed70dc04444122f813756642b077582046837b63069c72b25ce87712e012202021436d8bab43f21c8b74522af3c5a28c4366873efd2c1afe951a314ef2b4cbb1d4830450221009e4c744060de0798bf782e6ad59fedb6984ddadbc80b62faf23250cefcc038f6022030ef5d71850340976ae2d5850152934b6c2a47a0ed87b8c993a6817e368c21860101056953210200d16a17d43c25ac12e722a5911666bf0ed143c78d14990241fa5d86158fb9262102010a57a1988a7cea118a0a3e3a81686e29bf93761ea4e9ddd988fff28d6f3aa921021436d8bab43f21c8b74522af3c5a28c4366873efd2c1afe951a314ef2b4cbb1d53ae22060200d16a17d43c25ac12e722a5911666bf0ed143c78d14990241fa5d86158fb926043d937de7220602010a57a1988a7cea118a0a3e3a81686e29bf93761ea4e9ddd988fff28d6f3aa90419e3d0ec2206021436d8bab43f21c8b74522af3c5a28c4366873efd2c1afe951a314ef2b4cbb1d04875a65c400010169532102ece3a9b4c4e42811c4b9d424d76ba4ffeda5e6590d9f6144be1175a0bd54dc0b2103547cb2686e9b53e81bdbe1b2b8a0b5b494cfa05223f5e105fe9364bfbb3aa05f2103b238f9c7bbee00e4e9b3df445ea751a77fe5e4d0eca0f74985676e4a93759c4053ae220202ece3a9b4c4e42811c4b9d424d76ba4ffeda5e6590d9f6144be1175a0bd54dc0b0400378953220203547cb2686e9b53e81bdbe1b2b8a0b5b494cfa05223f5e105fe9364bfbb3aa05f0417edfdb4220203b238f9c7bbee00e4e9b3df445ea751a77fe5e4d0eca0f74985676e4a93759c400401a8c9750000";

fn get_dummy_msg() -> Vec<u8> {
	vec![
		0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
		0, 32, 0, 0, 191, 192, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
		0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
		0, 0, 0, 0, 0, 32, 218, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
		0, 0, 0, 0, 0, 0, 0, 0, 8, 175, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
		0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 39, 17, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
		0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 2, 3, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
		0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
		0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 224, 0, 0, 0, 3, 0, 0, 0, 3, 0, 0, 191,
		192, 109, 201, 164, 248, 42, 13, 199, 33, 220, 10, 168, 155, 230, 234, 220, 122, 85, 49,
		145, 193, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
		0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 240, 177, 233, 90, 38, 113, 164, 249,
		51, 65, 98, 109, 205, 42, 220, 97, 61, 3, 216, 93, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 240,
		177, 233, 90, 38, 113, 164, 249, 51, 65, 98, 109, 205, 42, 220, 97, 61, 3, 216, 93, 0, 0,
		0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 34, 147,
		224, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
		0, 0, 192, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
		0, 0, 0, 0, 0,
	]
}

fn get_unsigned_psbt() -> Psbt {
	Psbt::deserialize(&Vec::<u8>::from_hex(DUMMY_UNSIGNED_PSBT_STR).unwrap()).unwrap()
}

fn get_migration_psbt() -> Psbt {
	let mut psbt = get_unsigned_psbt();
	psbt.unsigned_tx.output = vec![psbt.unsigned_tx.output[0].clone()];
	psbt.outputs = vec![psbt.outputs[0].clone()];
	psbt
}

fn get_bumped_psbt() -> Psbt {
	use bp_btc_relay::Amount;
	let mut ret = get_unsigned_psbt();
	ret.unsigned_tx.output[0].value -= Amount::from_sat(100);
	ret
}

fn get_signed_psbt() -> Psbt {
	Psbt::deserialize(&Vec::<u8>::from_hex(DUMMY_SIGNED_PSBT_STR).unwrap()).unwrap()
}

fn get_psbt_req<T: Config>(psbt: &Psbt, msgs: Vec<Vec<u8>>) -> (H256, PsbtRequest<T::AccountId>) {
	let mut txid = psbt.unsigned_tx.compute_txid().to_byte_array();
	txid.reverse();
	let txid = H256::from(txid);
	let req = PsbtRequest::new(psbt.serialize(), msgs, RequestType::Normal);

	(txid, req)
}

fn setup_pending<T: Config>() {
	let (txid, req) = get_psbt_req::<T>(&get_unsigned_psbt(), vec![get_dummy_msg()]);
	<PendingRequests<T>>::insert(&txid, req);
}

fn setup_finalized<T: Config>() -> H256 {
	let (txid, req) = get_psbt_req::<T>(&get_signed_psbt(), vec![get_dummy_msg()]);
	<FinalizedRequests<T>>::insert(&txid, req);
	txid
}

fn setup_executives<T: Config>() -> Result<T::AccountId, DispatchError> {
	<Authority<T>>::put(account::<T::AccountId>("authority", 0, 0));
	<BitcoinSocket<T>>::put(account::<T::AccountId>("socket", 0, 0));
	<Socket<T>>::put(account::<T::AccountId>("socket", 0, 0));

	let user = account("user", 0, 0);
	T::RegistrationPool::set_benchmark(
		&[account("executive", 0, 0), account("executive", 1, 0), account("executive", 2, 0)],
		&user,
	)?;

	Ok(user)
}

#[benchmarks(
	where
		H160: Into<T::AccountId>,
		T::AccountId: Into<H160>,
)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn set_authority() {
		let authority: T::AccountId = account("authority", 0, 0);
		#[extrinsic_call]
		_(RawOrigin::Root, authority);
	}

	#[benchmark]
	fn set_socket() {
		let authority: T::AccountId = account("socket", 0, 0);
		#[extrinsic_call]
		_(RawOrigin::Root, authority, false);
	}

	#[benchmark]
	fn submit_unsigned_psbt() {
		let psbt_manager = account("authority", 0, 0);
		<MaxFeeRate<T>>::put(u64::MAX);

		let user = setup_executives::<T>().unwrap();

		let system_vault =
			T::RegistrationPool::get_system_vault(T::RegistrationPool::get_current_round())
				.unwrap();
		let refund = T::RegistrationPool::get_refund_address(&user).unwrap();
		let outputs = vec![(system_vault, vec![]), (refund, vec![get_dummy_msg()])];
		let psbt = get_unsigned_psbt().serialize();
		let msg = UnsignedPsbtMessage { authority_id: psbt_manager, outputs, psbt };

		let signature = T::Signature::decode(&mut [0u8; 65].as_ref()).expect("valid signature");

		#[extrinsic_call]
		_(RawOrigin::None, msg, signature);
	}

	#[benchmark]
	fn submit_signed_psbt() {
		<MaxFeeRate<T>>::put(u64::MAX);

		let _ = setup_executives::<T>();
		setup_pending::<T>();

		let msg = SignedPsbtMessage {
			authority_id: account("authority", 0, 0),
			unsigned_psbt: get_unsigned_psbt().serialize(),
			signed_psbt: get_signed_psbt().serialize(),
		};
		let signature = T::Signature::decode(&mut [0u8; 65].as_ref()).expect("valid signature");

		#[extrinsic_call]
		_(RawOrigin::None, msg, signature);
	}

	#[benchmark]
	fn submit_executed_request() {
		let _ = setup_executives::<T>();
		let txid = setup_finalized::<T>();

		let msg = ExecutedPsbtMessage { authority_id: account("authority", 0, 0), txid };
		let signature = T::Signature::decode(&mut [0u8; 65].as_ref()).expect("valid signature");

		#[extrinsic_call]
		_(RawOrigin::None, msg, signature);
	}

	#[benchmark]
	fn submit_rollback_request() {
		<MaxFeeRate<T>>::put(u64::MAX);

		let user = setup_executives::<T>().unwrap();

		let rollback_txid = H256::from([1u8; 32]);
		let vout = U256::from(1);
		let amount = U256::from(1000000000u64);

		let msg = RollbackPsbtMessage {
			who: user,
			txid: rollback_txid,
			vout,
			amount,
			unsigned_psbt: get_unsigned_psbt().serialize(),
		};

		#[extrinsic_call]
		_(RawOrigin::Root, msg);
	}

	#[benchmark]
	fn submit_rollback_poll() {
		<MaxFeeRate<T>>::put(u64::MAX);

		let user = setup_executives::<T>().unwrap();
		let psbt = get_unsigned_psbt();
		let mut txid_bytes = psbt.unsigned_tx.compute_txid().to_byte_array();
		txid_bytes.reverse();
		let psbt_txid = H256::from(txid_bytes);
		let rollback_txid = H256::from([1u8; 32]);
		let vout = U256::from(1);
		let amount = U256::from(1000000000u64);
		let rollback_msg = RollbackPsbtMessage {
			who: user,
			txid: rollback_txid,
			vout,
			amount,
			unsigned_psbt: psbt.serialize(),
		};
		let _ = Pallet::<T>::submit_rollback_request(RawOrigin::Root.into(), rollback_msg);

		let authority_id = account("authority", 0, 0);
		let msg = RollbackPollMessage { authority_id, txid: psbt_txid, is_approved: true };
		let signature = T::Signature::decode(&mut [0u8; 65].as_ref()).expect("valid signature");

		#[extrinsic_call]
		_(RawOrigin::None, msg, signature);
	}

	#[benchmark]
	fn submit_migration_request() {
		<MaxFeeRate<T>>::put(u64::MAX);
		let _ = setup_executives::<T>();

		let psbt = get_migration_psbt();
		let _ = T::RegistrationPool::set_service_state(MigrationSequence::UTXOTransfer);
		#[extrinsic_call]
		_(RawOrigin::Root, psbt.serialize());
	}

	#[benchmark]
	fn set_max_fee_rate() {
		#[extrinsic_call]
		_(RawOrigin::Root, 2000);
	}

	#[benchmark]
	fn submit_bump_fee_request() {
		<MaxFeeRate<T>>::put(u64::MAX);
		let _ = setup_executives::<T>();

		let txid = setup_finalized::<T>();
		let msg = ExecutedPsbtMessage { authority_id: account("authority", 0, 0), txid };
		let signature = T::Signature::decode(&mut [0u8; 65].as_ref()).expect("valid signature");
		let _ = Pallet::<T>::submit_executed_request(RawOrigin::None.into(), msg, signature);

		#[extrinsic_call]
		_(RawOrigin::Root, txid, get_bumped_psbt().serialize())
	}

	#[benchmark]
	fn drop_pending_rollback_request() {
		<MaxFeeRate<T>>::put(u64::MAX);

		let psbt = get_unsigned_psbt();
		let mut txid = psbt.unsigned_tx.compute_txid().to_byte_array();
		txid.reverse();
		let psbt_txid = H256::from(txid);

		let user = setup_executives::<T>().unwrap();
		let rollback_txid = H256::from([1u8; 32]);
		let vout = U256::from(1);
		let amount = U256::from(1000000000u64);
		let msg = RollbackPsbtMessage {
			who: user,
			txid: rollback_txid,
			vout,
			amount,
			unsigned_psbt: psbt.serialize(),
		};
		let _ = Pallet::<T>::submit_rollback_request(RawOrigin::Root.into(), msg);

		#[extrinsic_call]
		_(RawOrigin::Root, psbt_txid);
	}

	impl_benchmark_test_suite!(Pallet, mock::new_test_ext(), mock::Test);
}
