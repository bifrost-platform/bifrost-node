use crate::{weights::WeightInfo, PendingFeeRate, PoolRound, Utxo};

use frame_support::{pallet_prelude::*, traits::StorageVersion};
use frame_system::pallet_prelude::*;

use bp_btc_relay::UnboundedBytes;
use sp_core::H256;
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
		type WeightInfo: WeightInfo;
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
	/// key: pool round
	/// value: utxo(s)
	pub type Utxos<T: Config> =
		StorageMap<_, Twox64Concat, PoolRound, Vec<Utxo<T::AccountId>>, ValueQuery>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// key: txid
	/// value: utxo(s)
	pub type LockedTxos<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		PoolRound,
		Twox64Concat,
		H256,
		Vec<Utxo<T::AccountId>>,
		ValueQuery,
	>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// key: txid
	/// value: utxo(s)
	pub type SpentTxos<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		PoolRound,
		Twox64Concat,
		H256,
		Vec<Utxo<T::AccountId>>,
		ValueQuery,
	>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// key: pool round
	/// value: pending outbound requests socket messages (in bytes)
	pub type OutboundPool<T: Config> =
		StorageMap<_, Twox64Concat, PoolRound, Vec<UnboundedBytes>, ValueQuery>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// key: pool round
	/// value: pending fee rate
	pub type FeeRate<T: Config> =
		StorageMap<_, Twox64Concat, PoolRound, PendingFeeRate<T::AccountId>, ValueQuery>;
}
