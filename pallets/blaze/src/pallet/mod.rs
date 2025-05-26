mod impls;

use crate::{
	weights::WeightInfo, BTCTransaction, BroadcastSubmission, FeeRateSubmission,
	OutboundRequestSubmission, Utxo, UtxoStatus, UtxoSubmission,
};

use frame_support::{pallet_prelude::*, traits::StorageVersion};
use frame_system::pallet_prelude::*;

use bp_btc_relay::{
	blaze::{FailureReason, UtxoInfo, UtxoInfoWithSize},
	traits::{PoolManager, SocketQueueManager, SocketVerifier},
	utils::estimate_finalized_input_size,
	UnboundedBytes,
};
use bp_staking::{traits::Authorities, MAX_AUTHORITIES};
use parity_scale_codec::{alloc::string::ToString, Encode};
use sp_core::H256;
use sp_io::hashing::keccak_256;
use sp_runtime::traits::{Block, Header, IdentifyAccount, Verify};
use sp_std::{fmt::Display, vec, vec::Vec};

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Overarching event type
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		/// The signature signed by the issuer.
		type Signature: Verify<Signer = Self::Signer> + Encode + Decode + Parameter;
		/// The signer of the message.
		type Signer: IdentifyAccount<AccountId = Self::AccountId>
			+ Encode
			+ Decode
			+ Parameter
			+ MaxEncodedLen;
		/// The Bifrost relayers.
		type Relayers: Authorities<Self::AccountId>;
		/// Socket queue manager.
		type SocketQueue: SocketVerifier<Self::AccountId> + SocketQueueManager<Self::AccountId>;
		/// The Bitcoin registration pool pallet.
		type RegistrationPool: PoolManager<Self::AccountId>;
		/// The fee rate expiration in blocks.
		#[pallet::constant]
		type FeeRateExpiration: Get<u32>;
		/// The threshold for fault tolerance in blocks.
		#[pallet::constant]
		type ToleranceThreshold: Get<u32>;
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The utxo is unknown.
		UnknownUtxo,
		/// The txid is unknown.
		UnknownTransaction,
		/// The utxo does not exist.
		UtxoDNE,
		/// The utxo(s) are already spent.
		AlreadySpent,
		/// The authority has already voted.
		AlreadyVoted,
		/// The submission is empty.
		EmptySubmission,
		/// The submission is invalid.
		InvalidSubmission,
		/// The value is out of range.
		OutOfRange,
		/// Cannot set the value as identical to the previous value
		NoWritingSameValue,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// The activation status has been set.
		ActivationSet { is_activated: bool },
		/// The deactivation counter has been increased.
		CounterIncreased { counter: u32, failure_reason: FailureReason },
	}

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T> {
		#[serde(skip)]
		pub _config: PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {}
	}

	#[pallet::storage]
	/// The flag that represents whether BLAZE is activated.
	pub type IsActivated<T: Config> = StorageValue<_, bool, ValueQuery>;

	#[pallet::storage]
	/// The counter for fault tolerance. If the counter exceeds the threshold, BLAZE will be deactivated.
	pub type ToleranceCounter<T: Config> = StorageValue<_, u32, ValueQuery>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// The submitted UTXOs by relayers.
	///
	/// Key: UTXO hash (keccak256(txid, vout, amount))
	/// Value: UTXO information
	pub type Utxos<T: Config> = StorageMap<_, Twox64Concat, H256, Utxo<T::AccountId>>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// The UTXOs that are locked to a specific PSBT.
	///
	/// Key: The PSBT txid
	/// Value: The UTXOs that are locked to the PSBT
	pub type PendingTxs<T: Config> =
		StorageMap<_, Twox64Concat, H256, BTCTransaction<T::AccountId>>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// The UTXOs that are spent by a specific PSBT.
	///
	/// Key: The PSBT txid
	/// Value: The UTXOs that are spent by the PSBT
	pub type ConfirmedTxs<T: Config> =
		StorageMap<_, Twox64Concat, H256, BTCTransaction<T::AccountId>>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// The pending outbound Socket messages
	/// Value: SocketMessage's in bytes (The vector will be cleared once SocketQueue builds the PSBT)
	pub type OutboundPool<T: Config> = StorageValue<_, Vec<UnboundedBytes>, ValueQuery>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// The outbound requests that have been executed.
	/// Value: The PSBT txids (The vector will be cleared once SocketQueue handles the requests)
	pub type ExecutedRequests<T: Config> = StorageValue<_, Vec<H256>, ValueQuery>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// The fee rates submitted by the relayers.
	///
	/// Key: The relayer address
	/// Value: The fee rate and the deadline (The fee rate will be removed once the deadline is reached)
	pub type FeeRates<T: Config> = StorageValue<
		_,
		BoundedBTreeMap<T::AccountId, (u64, u64, BlockNumberFor<T>), ConstU32<MAX_AUTHORITIES>>,
		ValueQuery,
	>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		/// Set BLAZE's activation status.
		pub fn set_activation(
			origin: OriginFor<T>,
			is_activated: bool,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			let current = <IsActivated<T>>::get();
			ensure!(current != is_activated, Error::<T>::NoWritingSameValue);
			<IsActivated<T>>::put(is_activated);
			Self::deposit_event(Event::ActivationSet { is_activated });
			Ok(().into())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		/// Submit UTXOs. The submitted UTXO will be available once the majority of the relayers approve it.
		pub fn submit_utxos(
			origin: OriginFor<T>,
			utxo_submission: UtxoSubmission<T::AccountId>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			let UtxoSubmission { authority_id, utxos } = utxo_submission;
			ensure!(!utxos.is_empty(), Error::<T>::EmptySubmission);

			for utxo in utxos {
				let UtxoInfo { txid, vout, amount, address } = utxo;

				let descriptor = match T::RegistrationPool::get_bonded_descriptor(&address) {
					Some(descriptor) => descriptor,
					None => continue,
				};

				// try to hash (keccak256) the utxo data (txid, vout, amount)
				let utxo_hash =
					H256::from_slice(keccak_256(&Encode::encode(&(txid, vout, amount))).as_ref());

				// try to insert the utxo
				if let Some(mut u) = <Utxos<T>>::get(&utxo_hash) {
					// check if the utxo is already approved
					if u.status != UtxoStatus::Unconfirmed {
						continue;
					}
					if u.voters.contains(&authority_id) {
						continue;
					}
					u.voters.try_push(authority_id.clone()).map_err(|_| Error::<T>::OutOfRange)?;

					// check if the utxo majority is reached
					if u.voters.len() as u32 >= T::Relayers::majority() {
						u.status = UtxoStatus::Available;
					}
					<Utxos<T>>::insert(&utxo_hash, u);
				} else {
					let voters = vec![authority_id.clone()];
					let input_vbytes = if let Some(input_vbytes) =
						estimate_finalized_input_size(&descriptor.script_pubkey(), None)
					{
						input_vbytes
					} else {
						continue;
					};
					<Utxos<T>>::insert(
						&utxo_hash,
						Utxo {
							inner: UtxoInfoWithSize {
								hash: utxo_hash,
								txid,
								vout,
								amount,
								descriptor: descriptor.to_string(),
								input_vbytes,
							},
							status: UtxoStatus::Unconfirmed,
							voters: BoundedVec::try_from(voters)
								.map_err(|_| Error::<T>::OutOfRange)?,
						},
					);
				}
			}

			Ok(().into())
		}

		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		/// Spend UTXOs. The UTXO will be spent once the majority of the relayers approve it.
		pub fn broadcast_poll(
			origin: OriginFor<T>,
			broadcast_submission: BroadcastSubmission<T::AccountId>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			let BroadcastSubmission { authority_id, txid } = broadcast_submission;
			ensure!(!<ConfirmedTxs<T>>::contains_key(&txid), Error::<T>::AlreadySpent);

			let mut pending_txs =
				<PendingTxs<T>>::get(&txid).ok_or(Error::<T>::UnknownTransaction)?;

			ensure!(!pending_txs.voters.contains(&authority_id), Error::<T>::AlreadyVoted);
			pending_txs
				.voters
				.try_push(authority_id.clone())
				.map_err(|_| Error::<T>::OutOfRange)?;

			if pending_txs.voters.len() as u32 >= T::Relayers::majority() {
				<PendingTxs<T>>::remove(&txid);
				<ConfirmedTxs<T>>::insert(&txid, pending_txs.clone());

				<ExecutedRequests<T>>::mutate(|requests| {
					requests.push(txid);
				});

				// remove spent utxos
				pending_txs.inputs.iter().for_each(|input| {
					<Utxos<T>>::remove(&input.hash);
				});
			} else {
				<PendingTxs<T>>::insert(&txid, pending_txs.clone());
			}
			Ok(().into())
		}

		#[pallet::call_index(3)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		/// Submit a fee rate. The fee rate is only available until the deadline.
		pub fn submit_fee_rate(
			origin: OriginFor<T>,
			fee_rate_submission: FeeRateSubmission<T::AccountId, BlockNumberFor<T>>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			let FeeRateSubmission { authority_id, lt_fee_rate, fee_rate, .. } = fee_rate_submission;

			let min_fee_rate = 1;
			let max_fee_rate = T::SocketQueue::get_max_fee_rate();
			ensure!(
				lt_fee_rate >= min_fee_rate && lt_fee_rate <= max_fee_rate,
				Error::<T>::OutOfRange
			);
			ensure!(fee_rate >= min_fee_rate && fee_rate <= max_fee_rate, Error::<T>::OutOfRange);

			let mut fee_rates = <FeeRates<T>>::get();
			// fee rate finalization has to be done until expiration
			let expires_at =
				<frame_system::Pallet<T>>::block_number() + T::FeeRateExpiration::get().into();

			fee_rates
				.try_insert(authority_id, (lt_fee_rate, fee_rate, expires_at))
				.map_err(|_| Error::<T>::OutOfRange)?;
			<FeeRates<T>>::put(fee_rates);

			Ok(().into())
		}

		#[pallet::call_index(4)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		/// Submit Socket messages originated from a Bitcoin outbound request.
		pub fn submit_outbound_requests(
			origin: OriginFor<T>,
			outbound_request_submission: OutboundRequestSubmission<T::AccountId>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			let OutboundRequestSubmission { messages, .. } = outbound_request_submission;
			ensure!(!messages.is_empty(), Error::<T>::EmptySubmission);

			let mut pool = <OutboundPool<T>>::get();
			for message in messages {
				// check if the message is already submitted
				if pool.contains(&message) {
					continue;
				}
				// verify the message
				T::SocketQueue::verify_socket_message(&message)?;
				pool.push(message);
			}

			// Update the outbound pool
			<OutboundPool<T>>::put(pool);

			Ok(().into())
		}
	}

	#[pallet::validate_unsigned]
	impl<T: Config> ValidateUnsigned for Pallet<T>
	where
		<<<T as frame_system::Config>::Block as Block>::Header as Header>::Number: Display,
	{
		type Call = Call<T>;

		fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
			match call {
				Call::submit_utxos { utxo_submission, signature } => {
					Self::verify_utxo_submission(utxo_submission, signature)
				},
				Call::broadcast_poll { broadcast_submission, signature } => {
					Self::verify_broadcast_submission(broadcast_submission, signature)
				},
				Call::submit_fee_rate { fee_rate_submission, signature } => {
					Self::verify_submit_fee_rate(fee_rate_submission, signature)
				},
				Call::submit_outbound_requests { outbound_request_submission, signature } => {
					Self::verify_submit_outbound_requests(outbound_request_submission, signature)
				},
				_ => InvalidTransaction::Call.into(),
			}
		}
	}
}
