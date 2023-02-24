use crate::{BalanceOf, PropIndex, Proposal, Releases, WeightInfo};

use frame_support::{
	pallet_prelude::*,
	traits::{Currency, Imbalance, ReservableCurrency},
};
use frame_system::pallet_prelude::*;

use impl_serde::serialize::to_hex;
use sp_runtime::traits::Zero;
use sp_std::prelude::*;

#[frame_support::pallet]
pub mod pallet {

	use super::*;

	/// Pallet for bfc utility
	#[pallet::pallet]
	#[pallet::generate_store(pub(crate) trait Store)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	/// Configuration trait of this pallet
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
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
	/// Storage version of this pallet.
	pub(crate) type StorageVersion<T: Config> = StorageValue<_, Releases, ValueQuery>;

	#[pallet::storage]
	/// Storage for accepted proposals. Proposal passed by governance will be stored here.
	pub type AcceptedProposals<T: Config> = StorageValue<_, Vec<Proposal>, ValueQuery>;

	#[pallet::storage]
	/// Storage for proposal index. Whenever proposal is accepted, index will be increased.
	pub type ProposalIndex<T: Config> = StorageValue<_, PropIndex, ValueQuery>;

	#[pallet::genesis_config]
	pub struct GenesisConfig {}

	#[cfg(feature = "std")]
	impl Default for GenesisConfig {
		fn default() -> Self {
			Self {}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig {
		fn build(&self) {
			StorageVersion::<T>::put(Releases::V2_0_0);
			ProposalIndex::<T>::put(0);
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

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn decode_works() {
		let proposal = b"This is test proposal";
		let hex = to_hex(proposal, true);
		let decode = sp_core::bytes::from_hex(hex.as_str()).unwrap();
		assert_eq!(decode, "This is test proposal".as_bytes());
	}
}
