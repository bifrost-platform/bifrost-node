mod impls;

use crate::{
	weights::WeightInfo, FeeRateSubmission, OutboundRequestSubmission, PendingFeeRate,
	SpendTxosSubmission, Utxo, UtxoSubmission, UtxoVote,
};

use frame_support::{pallet_prelude::*, traits::StorageVersion};
use frame_system::pallet_prelude::*;

use bp_btc_relay::UnboundedBytes;
use bp_staking::traits::Authorities;
use parity_scale_codec::Encode;
use sp_core::{H256, U256};
use sp_io::hashing::keccak_256;
use sp_runtime::traits::{IdentifyAccount, Verify};
use sp_std::vec::Vec;

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
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The utxo is already locked.
		UtxoAlreadyLocked,
		/// The utxo is already spent.
		UtxoAlreadySpent,
		/// The value is out of range.
		OutOfRange,
		/// The hash is invalid.
		InvalidHash,
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
	/// key: txid, vout
	/// value: utxo
	pub type Utxos<T: Config> = StorageMap<_, Twox64Concat, H256, Utxo<T::AccountId>>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// key: txid, vout
	/// value: utxo
	pub type LockedTxos<T: Config> = StorageMap<_, Twox64Concat, H256, U256>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// key: txid, vout
	/// value: utxo
	pub type SpentTxos<T: Config> = StorageMap<_, Twox64Concat, H256, U256>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// pending outbound requests socket messages (in bytes)
	pub type OutboundPool<T: Config> = StorageValue<_, Vec<UnboundedBytes>, ValueQuery>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// pending fee rate
	pub type FeeRate<T: Config> = StorageValue<_, PendingFeeRate<T::AccountId>, ValueQuery>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn set_activation(
			origin: OriginFor<T>,
			is_activated: bool,
		) -> DispatchResultWithPostInfo {
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

			let UtxoSubmission { authority_id, votes } = utxo_submission;

			for vote in votes {
				let UtxoVote { txid, vout, amount, vote, lock_time, utxo_hash } = vote;

				// try to hash (keccak256) the utxo data (txid, vout, amount, lock_time)
				let check_hash = H256::from_slice(
					keccak_256(&Encode::encode(&(txid, vout, amount, lock_time))).as_ref(),
				);
				if check_hash != utxo_hash {
					return Err(Error::<T>::InvalidHash.into());
				}

				// check if the utxo is already locked
				if <LockedTxos<T>>::contains_key(&utxo_hash) {
					return Err(Error::<T>::UtxoAlreadyLocked.into());
				}
				// check if the utxo is already spent
				if <SpentTxos<T>>::contains_key(&utxo_hash) {
					return Err(Error::<T>::UtxoAlreadySpent.into());
				}

				// try to insert the utxo
				if let Some(mut utxo) = <Utxos<T>>::get(&utxo_hash) {
					// check if the utxo is already approved
					if utxo.is_approved {
						continue;
					}
					utxo.votes
						.try_insert(authority_id.clone(), vote)
						.map_err(|_| Error::<T>::OutOfRange)?;

					// check if the utxo majority is reached
					if utxo.votes.iter().filter(|v| *v.1).count() as u32 >= T::Relayers::majority()
					{
						utxo.is_approved = true;
					}
					<Utxos<T>>::insert(&utxo_hash, utxo);
				} else {
					let mut votes = BoundedBTreeMap::default();
					votes
						.try_insert(authority_id.clone(), vote)
						.map_err(|_| Error::<T>::OutOfRange)?;

					<Utxos<T>>::insert(
						&utxo_hash,
						Utxo { txid, vout, amount, is_approved: false, lock_time, votes },
					);
				}
			}

			Ok(().into())
		}

		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn submit_fee_rate(
			origin: OriginFor<T>,
			fee_rate_submission: FeeRateSubmission<T::AccountId>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			Ok(().into())
		}

		#[pallet::call_index(3)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn submit_outbound_requests(
			origin: OriginFor<T>,
			outbound_request_submission: OutboundRequestSubmission<T::AccountId>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			Ok(().into())
		}

		#[pallet::call_index(4)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		pub fn spend_txos(
			origin: OriginFor<T>,
			spend_txos_submission: SpendTxosSubmission<T::AccountId>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			Ok(().into())
		}
	}

	#[pallet::validate_unsigned]
	impl<T: Config> ValidateUnsigned for Pallet<T> {
		type Call = Call<T>;

		fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
			match call {
				Call::submit_utxos { utxo_submission, signature } => {
					Self::verify_submit_utxos(utxo_submission, signature)
				},
				Call::submit_fee_rate { fee_rate_submission, signature } => {
					Self::verify_submit_fee_rate(fee_rate_submission, signature)
				},
				Call::submit_outbound_requests { outbound_request_submission, signature } => {
					Self::verify_submit_outbound_requests(outbound_request_submission, signature)
				},
				Call::spend_txos { spend_txos_submission, signature } => {
					Self::verify_spend_txos(spend_txos_submission, signature)
				},
				_ => InvalidTransaction::Call.into(),
			}
		}
	}
}
