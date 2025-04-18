mod impls;

use crate::{
	weights::WeightInfo, FeeRateSubmission, OutboundRequestSubmission, Utxo, UtxoInfo,
	UtxoSubmission,
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
		/// The utxo is already locked.
		UtxoAlreadyLocked,
		/// The utxo is already spent.
		UtxoAlreadySpent,
		/// The utxo is not locked.
		UtxoNotLocked,
		/// The utxo is unknown.
		UnknownUtxo,
		/// The submission is empty.
		EmptySubmission,
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
	/// key: utxo hash (keccak256(txid, vout, amount))
	/// value: utxo
	pub type LockedTxos<T: Config> = StorageMap<_, Twox64Concat, H256, Utxo<T::AccountId>>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// key: utxo hash (keccak256(txid, vout, amount))
	/// value: utxo
	pub type SpentTxos<T: Config> = StorageMap<_, Twox64Concat, H256, Utxo<T::AccountId>>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// pending outbound requests socket messages (in bytes)
	pub type OutboundPool<T: Config> = StorageValue<_, Vec<UnboundedBytes>, ValueQuery>;

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

			Weight::from_parts(0, 0)
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
			if utxos.is_empty() {
				return Err(Error::<T>::EmptySubmission.into());
			}

			for utxo in utxos {
				let UtxoInfo { txid, vout, amount } = utxo;

				// try to hash (keccak256) the utxo data (txid, vout, amount)
				let utxo_hash =
					H256::from_slice(keccak_256(&Encode::encode(&(txid, vout, amount))).as_ref());

				// check if the utxo is already locked
				if <LockedTxos<T>>::contains_key(&utxo_hash) {
					return Err(Error::<T>::UtxoAlreadyLocked.into());
				}
				// check if the utxo is already spent
				if <SpentTxos<T>>::contains_key(&utxo_hash) {
					return Err(Error::<T>::UtxoAlreadySpent.into());
				}

				// try to insert the utxo
				if let Some(mut u) = <Utxos<T>>::get(&utxo_hash) {
					// check if the utxo is already approved
					if u.is_approved {
						continue;
					}
					if u.voters.contains(&authority_id) {
						continue;
					}
					u.voters.try_push(authority_id.clone()).map_err(|_| Error::<T>::OutOfRange)?;

					// check if the utxo majority is reached
					if u.voters.len() as u32 >= T::Relayers::majority() {
						u.is_approved = true;
					}
					<Utxos<T>>::insert(&utxo_hash, u);
				} else {
					let voters = vec![authority_id.clone()];
					<Utxos<T>>::insert(
						&utxo_hash,
						Utxo {
							inner: UtxoInfo { txid, vout, amount },
							is_approved: false,
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
			utxo_submission: UtxoSubmission<T::AccountId>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			let UtxoSubmission { authority_id, utxos } = utxo_submission;
			if utxos.is_empty() {
				return Err(Error::<T>::EmptySubmission.into());
			}

			for utxo in utxos {
				let UtxoInfo { txid, vout, amount } = utxo;

				// try to hash (keccak256) the utxo data (txid, vout, amount)
				let utxo_hash =
					H256::from_slice(keccak_256(&Encode::encode(&(txid, vout, amount))).as_ref());

				// check if the utxo is available
				if <Utxos<T>>::contains_key(&utxo_hash) {
					return Err(Error::<T>::UtxoNotLocked.into());
				}
				// check if the utxo is already spent
				if <SpentTxos<T>>::contains_key(&utxo_hash) {
					return Err(Error::<T>::UtxoAlreadySpent.into());
				}

				if let Some(mut u) = <LockedTxos<T>>::get(&utxo_hash) {
					if u.voters.contains(&authority_id) {
						continue;
					}
					u.voters.try_push(authority_id.clone()).map_err(|_| Error::<T>::OutOfRange)?;

					if u.voters.len() as u32 >= T::Relayers::majority() {
						<LockedTxos<T>>::remove(&utxo_hash);
						<SpentTxos<T>>::insert(&utxo_hash, u);
					} else {
						<LockedTxos<T>>::insert(&utxo_hash, u);
					}
				} else {
					return Err(Error::<T>::UnknownUtxo.into());
				}
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
			if messages.is_empty() {
				return Err(Error::<T>::EmptySubmission.into());
			}

			for message in messages {
				// check if the message is already submitted
				let mut pool = <OutboundPool<T>>::get();
				if pool.contains(&message) {
					continue;
				}
				// verify the message
				T::Verifier::verify_socket_message(&message)?;
				pool.push(message);
				<OutboundPool<T>>::put(pool);
			}
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
					Self::verify_utxo_submission(utxo_submission, signature, "UtxosSubmission")
				},
				Call::spend_txos { utxo_submission, signature } => {
					Self::verify_utxo_submission(utxo_submission, signature, "SpendTxosSubmission")
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
