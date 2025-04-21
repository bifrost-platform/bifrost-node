mod impls;

use crate::{
	weights::WeightInfo, FeeRateSubmission, OutboundRequestSubmission, Txos, Utxo, UtxoInfo,
	UtxoStatus, UtxoSubmission,
};

use frame_support::{pallet_prelude::*, traits::StorageVersion};
use frame_system::pallet_prelude::*;

use bp_btc_relay::{traits::SocketVerifier, UnboundedBytes};
use bp_staking::{traits::Authorities, MAX_AUTHORITIES};
use parity_scale_codec::Encode;
use sp_core::{H256, U256};
use sp_io::hashing::keccak_256;
use sp_runtime::traits::{Block, Header, IdentifyAccount, Verify};
use sp_std::{fmt::Display, vec, vec::Vec};

#[frame_support::pallet]
pub mod pallet {
	use crate::SpendTxosSubmission;

	use super::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
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
		/// Socket message verifier.
		type Verifier: SocketVerifier<Self::AccountId>;
		/// The fee rate expiration in blocks.
		#[pallet::constant]
		type FeeRateExpiration: Get<u32>;
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The utxo is not locked.
		UtxoNotLocked,
		/// The utxo is unknown.
		UnknownUtxo,
		/// The txid is unknown.
		UnknownTransaction,
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
	#[pallet::unbounded]
	/// key: utxo hash (keccak256(txid, vout, amount))
	/// value: utxo
	pub type Utxos<T: Config> = StorageMap<_, Twox64Concat, H256, Utxo<T::AccountId>>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// key: PSBT txid
	/// value: UTXO hashes that are locked to the PSBT
	pub type LockedTxos<T: Config> = StorageMap<_, Twox64Concat, H256, Txos<T::AccountId>>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// key: PSBT txid
	/// value: UTXO hashes that are spent by the PSBT
	pub type SpentTxos<T: Config> = StorageMap<_, Twox64Concat, H256, Txos<T::AccountId>>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// pending outbound requests socket messages (in bytes)
	pub type OutboundPool<T: Config> = StorageValue<_, Vec<UnboundedBytes>, ValueQuery>;

	#[pallet::storage]
	#[pallet::unbounded]
	pub type ExecutedRequests<T: Config> = StorageValue<_, Vec<H256>, ValueQuery>;

	#[pallet::storage]
	#[pallet::unbounded]
	pub type FeeRates<T: Config> = StorageValue<
		_,
		BoundedBTreeMap<T::AccountId, (U256, BlockNumberFor<T>), ConstU32<MAX_AUTHORITIES>>,
		ValueQuery,
	>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(n: BlockNumberFor<T>) -> Weight {
			// remove expired fee rates
			let mut fee_rates = <FeeRates<T>>::get();
			fee_rates.retain(|_, (_, expires_at)| n <= *expires_at);
			<FeeRates<T>>::put(fee_rates);

			Weight::from_parts(0, 0) // TODO: add weight
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn set_activation(
			origin: OriginFor<T>,
			is_activated: bool,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			let current = <IsActivated<T>>::get();
			ensure!(current != is_activated, Error::<T>::NoWritingSameValue);
			<IsActivated<T>>::put(is_activated);
			Ok(().into())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn submit_utxos(
			origin: OriginFor<T>,
			utxo_submission: UtxoSubmission<T::AccountId>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			let UtxoSubmission { authority_id, utxos } = utxo_submission;
			ensure!(!utxos.is_empty(), Error::<T>::EmptySubmission);

			for utxo in utxos {
				let UtxoInfo { txid, vout, amount } = utxo;

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
					<Utxos<T>>::insert(
						&utxo_hash,
						Utxo {
							inner: UtxoInfo { txid, vout, amount },
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
		pub fn spend_txos(
			origin: OriginFor<T>,
			spend_submission: SpendTxosSubmission<T::AccountId>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			let SpendTxosSubmission { authority_id, txid, mut utxo_hashes } = spend_submission;
			ensure!(!utxo_hashes.is_empty(), Error::<T>::EmptySubmission);
			ensure!(!<SpentTxos<T>>::contains_key(&txid), Error::<T>::AlreadySpent);

			let mut locked_txos =
				<LockedTxos<T>>::get(&txid).ok_or(Error::<T>::UnknownTransaction)?;

			// check if the utxo hashes are identical to the locked txos
			utxo_hashes.sort();
			ensure!(utxo_hashes == locked_txos.utxo_hashes, Error::<T>::InvalidSubmission);

			ensure!(!locked_txos.voters.contains(&authority_id), Error::<T>::AlreadyVoted);
			locked_txos
				.voters
				.try_push(authority_id.clone())
				.map_err(|_| Error::<T>::OutOfRange)?;

			if locked_txos.voters.len() as u32 >= T::Relayers::majority() {
				<LockedTxos<T>>::remove(&txid);
				<SpentTxos<T>>::insert(&txid, locked_txos.clone());

				for utxo_hash in utxo_hashes {
					let mut utxo = <Utxos<T>>::get(&utxo_hash).ok_or(Error::<T>::UnknownUtxo)?;
					utxo.status = UtxoStatus::Spent;
					<Utxos<T>>::insert(&utxo_hash, utxo);
				}
				<ExecutedRequests<T>>::mutate(|requests| {
					requests.push(txid);
				});
			} else {
				<LockedTxos<T>>::insert(&txid, locked_txos.clone());
			}
			Ok(().into())
		}

		#[pallet::call_index(3)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn submit_fee_rate(
			origin: OriginFor<T>,
			fee_rate_submission: FeeRateSubmission<T::AccountId, BlockNumberFor<T>>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			let FeeRateSubmission { authority_id, fee_rate, .. } = fee_rate_submission;

			let mut fee_rates = <FeeRates<T>>::get();
			// fee rate finalization has to be done until expiration
			let expires_at =
				<frame_system::Pallet<T>>::block_number() + T::FeeRateExpiration::get().into();

			fee_rates
				.try_insert(authority_id, (fee_rate, expires_at))
				.map_err(|_| Error::<T>::OutOfRange)?;
			<FeeRates<T>>::put(fee_rates);

			// TODO: finalize the fee rate here or in SocketQueue

			Ok(().into())
		}

		#[pallet::call_index(4)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
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
				T::Verifier::verify_socket_message(&message)?;
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
				Call::spend_txos { spend_submission, signature } => {
					Self::verify_spend_txos_submission(spend_submission, signature)
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
