use crate::{migrations, BalanceOf, PropIndex, Proposal, WeightInfo};

use frame_support::{
	pallet_prelude::*,
	traits::{Currency, Imbalance, OnRuntimeUpgrade, ReservableCurrency, StorageVersion},
};
use frame_system::pallet_prelude::*;

use impl_serde::serialize::to_hex;
use sp_runtime::traits::Zero;
use sp_std::prelude::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	/// The current storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(3);

	/// Pallet for bfc utility
	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	/// Configuration trait of this pallet
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Overarching event type.
		/// The currency type.
		type Currency: Currency<Self::AccountId> + ReservableCurrency<Self::AccountId>;
		/// The origin which may forcibly mint native tokens.
		type MintableOrigin: EnsureOrigin<Self::RuntimeOrigin>;
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The given amount is too low to process.
		AmountTooLow,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A motion has been proposed by a public account.
		Proposed { proposal_index: PropIndex },
		/// Minted native tokens and deposit.
		MintNative { beneficiary: T::AccountId, minted: BalanceOf<T> },
	}

	#[pallet::storage]
	#[pallet::unbounded]
	/// Storage for accepted proposals. Proposal passed by governance will be stored here.
	pub type AcceptedProposals<T: Config> = StorageValue<_, Vec<Proposal>, ValueQuery>;

	#[pallet::storage]
	/// Storage for proposal index. Whenever proposal is accepted, index will be increased.
	pub type ProposalIndex<T: Config> = StorageValue<_, PropIndex, ValueQuery>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_runtime_upgrade() -> Weight {
			migrations::v3_update::MigrateToV3Update::<T>::on_runtime_upgrade()
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T> {
		pub proposal_index: PropIndex,
		#[serde(skip)]
		pub _config: PhantomData<T>,
	}

	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self { proposal_index: 0, _config: Default::default() }
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			ProposalIndex::<T>::put(self.proposal_index);
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::community_proposal())]
		/// General Proposal
		/// ####
		/// General community proposal without changes on codes.
		pub fn community_proposal(
			origin: OriginFor<T>,
			proposal: Vec<u8>,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;

			let mut proposal_index = ProposalIndex::<T>::get();
			let proposal = Proposal { proposal_hex: to_hex(&proposal[..], true), proposal_index };
			let mut proposals = AcceptedProposals::<T>::get();
			proposals.push(proposal);
			AcceptedProposals::<T>::put(proposals);
			proposal_index += 1;
			ProposalIndex::<T>::put(proposal_index);

			Self::deposit_event(Event::Proposed { proposal_index });
			Ok(().into())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::mint_native())]
		/// Mint the exact amount of native tokens and deposit to the target address.
		pub fn mint_native(
			origin: OriginFor<T>,
			beneficiary: T::AccountId,
			mint: BalanceOf<T>,
		) -> DispatchResultWithPostInfo {
			T::MintableOrigin::ensure_origin(origin)?;
			ensure!(!mint.is_zero(), Error::<T>::AmountTooLow);

			let minted = T::Currency::deposit_creating(&beneficiary, mint);
			Self::deposit_event(Event::MintNative { beneficiary, minted: minted.peek() });

			Ok(().into())
		}
	}
}
