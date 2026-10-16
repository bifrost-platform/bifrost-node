mod impls;

use crate::{RoundInfo, WeightInfo};

use bp_staking::traits::{RelayManager, RelayerSetManager};
use frame_support::{pallet_prelude::*, traits::Contains};
use frame_system::pallet_prelude::*;
use sp_runtime::Perbill;
use sp_staking::SessionIndex;
use sp_std::prelude::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	/// The current storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The relayer management hooks (`pallet-relay-manager`). Each validator is bonded to
		/// exactly one relayer; the selected relayer set follows the active validator set.
		type RelayManager: RelayManager<Self::AccountId>;
		/// Whether the given account has session keys registered. A validator can only be
		/// added once its keys are set, so the session pallet never queues a keyless authority.
		type HasSessionKeys: Contains<Self::AccountId>;
		/// Relayer address replacement (`pallet-relay-manager`). Replacements are requested by
		/// root through this pallet and applied at the next round; a pending replacement also
		/// triggers a new round, since the CCCP relayer set changes with it.
		type RelayerSets: RelayerSetManager<Self::AccountId>;
		/// The maximum number of authorities.
		#[pallet::constant]
		type MaxAuthorities: Get<u32>;
		/// The round length reported in `RoundInfo`. Rounds here only change with the authority
		/// set, so this is a nominal value (relayers use it to size their bootstrap look-back).
		#[pallet::constant]
		type NominalRoundLength: Get<u32>;
		/// The session length in blocks, reported in `RoundInfo`.
		#[pallet::constant]
		type SessionLength: Get<u32>;
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The validator is already a (requested) authority.
		AlreadyAuthority,
		/// The validator is not a (requested) authority.
		NotAuthority,
		/// The validator was removed and the removal hasn't been applied yet (next session).
		PendingRemoval,
		/// The validator has no session keys registered.
		SessionKeysNotSet,
		/// The authority set would exceed `MaxAuthorities`.
		TooManyAuthorities,
		/// The last authority cannot be removed — the chain would stop producing blocks.
		CannotRemoveLastAuthority,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An authority was added. Takes effect at the next session rotation.
		AuthorityAdded { validator: T::AccountId, relayer: T::AccountId },
		/// An authority was removed. Takes effect at the next session rotation.
		AuthorityRemoved { validator: T::AccountId },
		/// The authority set changed and a new round started.
		NewRound { round: u32, validators: Vec<T::AccountId> },
		/// A relayer replacement was requested. Applied at the next session rotation, which also
		/// starts a new round.
		RelayerSetRequested { validator: T::AccountId, new_relayer: T::AccountId },
		/// A pending relayer replacement was cancelled.
		RelayerSetCancelled { validator: T::AccountId },
		/// An offence was reported for an authority. Nothing is enforced; informational only.
		OffenceReported { offender: T::AccountId, slash_fraction: Perbill, session: SessionIndex },
	}

	#[pallet::storage]
	/// The requested authority (validator) set, edited by root. Applied at the next session
	/// rotation.
	pub type Authorities<T: Config> =
		StorageValue<_, BoundedVec<T::AccountId, T::MaxAuthorities>, ValueQuery>;

	#[pallet::storage]
	/// The authority set applied in the current round.
	pub type ActiveAuthorities<T: Config> =
		StorageValue<_, BoundedVec<T::AccountId, T::MaxAuthorities>, ValueQuery>;

	#[pallet::storage]
	/// The current round information.
	pub type Round<T: Config> = StorageValue<_, RoundInfo<BlockNumberFor<T>>, ValueQuery>;

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		/// The initial `(validator, relayer)` pairs. The validators must match the session
		/// pallet's genesis keys.
		pub authorities: Vec<(T::AccountId, T::AccountId)>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			// An empty set is only valid for the default genesis (e.g. the benchmarking
			// `development` preset); a real chain spec must list at least one authority.
			let mut validators: Vec<T::AccountId> = vec![];
			for (validator, relayer) in &self.authorities {
				assert!(!validators.contains(validator), "duplicate authority in genesis");
				T::RelayManager::join_relayers(relayer.clone(), validator.clone())
					.expect("genesis relayer must be valid");
				validators.push(validator.clone());
			}
			let validators: BoundedVec<T::AccountId, T::MaxAuthorities> =
				validators.try_into().expect("too many genesis authorities");
			<Authorities<T>>::put(&validators);
			<ActiveAuthorities<T>>::put(&validators);

			// Start Round 1 at Block 0
			let round = RoundInfo {
				current_round_index: 1u32,
				round_length: T::NominalRoundLength::get(),
				session_length: T::SessionLength::get(),
				..Default::default()
			};
			<Round<T>>::put(round);
			T::RelayManager::refresh_round(1u32);
			T::RelayManager::refresh_selected_relayers(1u32, validators.into_inner());
			T::RelayManager::refresh_majority(1u32);
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(n: BlockNumberFor<T>) -> Weight {
			<Round<T>>::mutate(|round| round.update_block(n));
			T::DbWeight::get().reads_writes(1, 1)
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Register a new authority (validator) and its bonded relayer.
		///
		/// The relayer joins `pallet-relay-manager`'s relayer pool immediately; both become
		/// active (selected) at the next session rotation, which also starts a new round.
		/// The validator must already have session keys registered (`session.set_keys`).
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::add_authority())]
		pub fn add_authority(
			origin: OriginFor<T>,
			validator: T::AccountId,
			relayer: T::AccountId,
		) -> DispatchResult {
			ensure_root(origin)?;

			let mut authorities = <Authorities<T>>::get();
			ensure!(!authorities.contains(&validator), Error::<T>::AlreadyAuthority);
			// removed but not yet applied: its relayer is still bonded until the next rotation
			ensure!(
				!<ActiveAuthorities<T>>::get().contains(&validator),
				Error::<T>::PendingRemoval
			);
			ensure!(T::HasSessionKeys::contains(&validator), Error::<T>::SessionKeysNotSet);

			authorities
				.try_push(validator.clone())
				.map_err(|_| Error::<T>::TooManyAuthorities)?;
			T::RelayManager::join_relayers(relayer.clone(), validator.clone())?;
			<Authorities<T>>::put(authorities);

			Self::deposit_event(Event::AuthorityAdded { validator, relayer });
			Ok(())
		}

		/// Remove an authority (validator). Its relayer is unbonded and both leave the active
		/// set at the next session rotation, which also starts a new round. The last authority
		/// cannot be removed.
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::remove_authority())]
		pub fn remove_authority(origin: OriginFor<T>, validator: T::AccountId) -> DispatchResult {
			ensure_root(origin)?;

			let mut authorities = <Authorities<T>>::get();
			let position = authorities
				.iter()
				.position(|a| *a == validator)
				.ok_or(Error::<T>::NotAuthority)?;
			ensure!(authorities.len() > 1, Error::<T>::CannotRemoveLastAuthority);

			authorities.remove(position);
			<Authorities<T>>::put(authorities);
			// a relayer replacement requested for a leaving validator is moot — drop it, or it
			// would keep a round change pending even once the set is back to the active one
			if T::RelayerSets::cancel_relayer_set(&validator).is_ok() {
				Self::deposit_event(Event::RelayerSetCancelled { validator: validator.clone() });
			}
			// never applied: not selected anywhere yet, so its relayer can leave right away
			if !<ActiveAuthorities<T>>::get().contains(&validator) {
				T::RelayManager::leave_relayers(&validator);
			}

			Self::deposit_event(Event::AuthorityRemoved { validator });
			Ok(())
		}

		/// Replace the relayer bonded to `validator` with `new_relayer`. Applied at the next
		/// session rotation, which also starts a new round (the CCCP relayer set changes).
		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::set_relayer())]
		pub fn set_relayer(
			origin: OriginFor<T>,
			validator: T::AccountId,
			new_relayer: T::AccountId,
		) -> DispatchResult {
			ensure_root(origin)?;
			ensure!(<Authorities<T>>::get().contains(&validator), Error::<T>::NotAuthority);

			T::RelayerSets::request_relayer_set(&validator, new_relayer.clone())?;

			Self::deposit_event(Event::RelayerSetRequested { validator, new_relayer });
			Ok(())
		}

		/// Cancel the pending relayer replacement of `validator`.
		#[pallet::call_index(3)]
		#[pallet::weight(<T as Config>::WeightInfo::cancel_relayer_set())]
		pub fn cancel_relayer_set(origin: OriginFor<T>, validator: T::AccountId) -> DispatchResult {
			ensure_root(origin)?;

			T::RelayerSets::cancel_relayer_set(&validator)?;

			Self::deposit_event(Event::RelayerSetCancelled { validator });
			Ok(())
		}
	}
}
