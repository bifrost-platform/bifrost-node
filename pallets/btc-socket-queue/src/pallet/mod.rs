mod impls;

use crate::{
	migrations, ExecutedPsbtMessage, PsbtRequest, RequestType, RollbackPollMessage,
	RollbackPsbtMessage, RollbackRequest, SignedPsbtMessage, SocketMessage, UnsignedPsbtMessage,
	WeightInfo,
};

use frame_support::{
	pallet_prelude::*,
	traits::{OnRuntimeUpgrade, SortedMembers, StorageVersion},
};
use frame_system::pallet_prelude::*;

use bp_btc_relay::{
	traits::{BlazeManager, PoolManager, SocketQueueManager},
	Amount, BoundedBitcoinAddress, MigrationSequence, UnboundedBytes,
};
use bp_staking::traits::Authorities;
use miniscript::bitcoin::FeeRate;
use scale_info::prelude::string::ToString;
use sp_core::{H160, H256, U256};
use sp_io::hashing::keccak_256;
use sp_runtime::traits::{IdentifyAccount, Verify};
use sp_std::{vec, vec::Vec};

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(2);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_evm::Config {
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
		/// The relay executive members.
		type Executives: SortedMembers<Self::AccountId>;
		/// The Bifrost relayers.
		type Relayers: Authorities<Self::AccountId>;
		/// The Bitcoin registration pool pallet.
		type RegistrationPool: PoolManager<Self::AccountId>;
		/// The Blaze pallet.
		type Blaze: BlazeManager;
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
		/// The maximum fee rate that can be set for PSBT.
		type DefaultMaxFeeRate: Get<u64>;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The authority has already submitted a signed PSBT.
		AuthorityAlreadySubmitted,
		/// The signed PSBT is already submitted by an authority.
		SignedPsbtAlreadySubmitted,
		/// The socket message is already submitted.
		SocketMessageAlreadySubmitted,
		/// The request has already been finalized or exists.
		RequestAlreadyExists,
		/// The request has already been approved.
		RequestAlreadyApproved,
		/// The authority account does not exist.
		AuthorityDNE,
		/// The socket contract does not exist.
		SocketDNE,
		/// The socket message does not exist.
		SocketMessageDNE,
		/// U256 overflowed.
		U256OverFlowed,
		/// The user does not exist.
		UserDNE,
		/// The system vault does not exist.
		SystemVaultDNE,
		/// The request hasn't been submitted yet.
		RequestDNE,
		/// The submitted PSBT is invalid.
		InvalidPsbt,
		/// The submitted unchecked output is invalid.
		InvalidUncheckedOutput,
		/// The contract calldata is invalid.
		InvalidCalldata,
		/// The socket message is invalid.
		InvalidSocketMessage,
		/// The request information is invalid.
		InvalidRequestInfo,
		/// The transaction information is invalid.
		InvalidTxInfo,
		/// The given bitcoin address is invalid.
		InvalidBitcoinAddress,
		/// Cannot finalize the PSBT.
		CannotFinalizePsbt,
		/// The value is out of range.
		OutOfRange,
		/// Cannot overwrite to the same value.
		NoWritingSameValue,
		/// Service is under maintenance mode.
		UnderMaintenance,
		/// The PSBT fee rate was not set properly.
		InvalidFeeRate,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An unsigned PSBT for an outbound request has been submitted.
		UnsignedPsbtSubmitted { txid: H256 },
		/// A signed PSBT for an outbound request has been submitted.
		SignedPsbtSubmitted { txid: H256, authority_id: T::AccountId },
		/// An unsigned PSBT for RBF has been submitted.
		BumpFeePsbtSubmitted { old_txid: H256, new_txid: H256 },
		/// An unsigned PSBT for a vault migration request has been submitted.
		MigrationPsbtSubmitted { txid: H256 },
		/// An unsigned PSBT for a rollback request has been submitted.
		RollbackPsbtSubmitted { txid: H256 },
		/// A rollback poll has been submitted.
		RollbackPollSubmitted { txid: H256, authority_id: T::AccountId, is_approved: bool },
		/// A rollback request has been approved.
		RollbackApproved { txid: H256 },
		/// An outbound request has been finalized.
		RequestFinalized { txid: H256 },
		/// An outbound request has been executed.
		RequestExecuted { txid: H256 },
		/// An authority has been set.
		AuthoritySet { new: T::AccountId },
		/// A socket contract has been set.
		SocketSet { new: T::AccountId, is_bitcoin: bool },
		/// The maximum PSBT fee rate has been set.
		MaxFeeRateSet { new: u64 },
	}

	#[pallet::storage]
	/// The `Socket` contract address.
	pub type Socket<T: Config> = StorageValue<_, T::AccountId, OptionQuery>;

	#[pallet::storage]
	/// The `BitcoinSocket` contract address.
	pub type BitcoinSocket<T: Config> = StorageValue<_, T::AccountId, OptionQuery>;

	#[pallet::storage]
	/// The core authority address. The account that is permitted to submit unsigned PSBT's.
	pub type Authority<T: Config> = StorageValue<_, T::AccountId, OptionQuery>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// The submitted `SocketMessage` instances.
	/// key: Request sequence ID.
	/// value:
	/// 	0. The PSBT txid that contains the socket message.
	/// 	1. The socket message in bytes.
	pub type SocketMessages<T: Config> = StorageMap<_, Twox64Concat, U256, (H256, SocketMessage)>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// Pending outbound requests that are not ready to be finalized.
	/// key: The pending PSBT's txid.
	/// value: The PSBT information.
	pub type PendingRequests<T: Config> =
		StorageMap<_, Twox64Concat, H256, PsbtRequest<T::AccountId>>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// Finalized outbound requests.
	/// key: The finalized PSBT's txid.
	/// value: The PSBT information.
	pub type FinalizedRequests<T: Config> =
		StorageMap<_, Twox64Concat, H256, PsbtRequest<T::AccountId>>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// Outbound requests that has been broadcasted to the Bitcoin network.
	/// key: The executed PSBT's txid.
	/// value: The PSBT information.
	pub type ExecutedRequests<T: Config> =
		StorageMap<_, Twox64Concat, H256, PsbtRequest<T::AccountId>>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// Pending or approved rollback requests.
	/// key: The PSBT's txid.
	/// value: The rollback information.
	pub type RollbackRequests<T: Config> =
		StorageMap<_, Twox64Concat, H256, RollbackRequest<T::AccountId>>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// Mapped outbound txids.
	/// key: The PSBT's txid.
	/// value: The composed socket messages.
	pub type BondedOutboundTx<T: Config> = StorageMap<_, Twox64Concat, H256, Vec<UnboundedBytes>>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// Mapped rollback outputs.
	/// key #1: The rollback transaction txid.
	/// key #2: The rollback transaction output index.
	/// value: The rollback PSBT txid.
	pub type BondedRollbackOutputs<T: Config> =
		StorageDoubleMap<_, Twox64Concat, H256, Twox64Concat, U256, H256>;

	#[pallet::storage]
	/// The maximum fee rate(sat/vb) that can be set for PSBT.
	pub type MaxFeeRate<T: Config> = StorageValue<_, u64, ValueQuery>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T>
	where
		T::AccountId: Into<H160>,
		H160: Into<T::AccountId>,
	{
		fn on_runtime_upgrade() -> Weight {
			migrations::init_v2::InitV2::<T>::on_runtime_upgrade()
		}

		fn on_initialize(n: BlockNumberFor<T>) -> Weight {
			if T::Blaze::is_activated() {
				// TODO: impl function for BLAZE actions
				let executed_requests = T::Blaze::take_executed_requests();
				for txid in executed_requests {
					if let Some(request) = <FinalizedRequests<T>>::take(&txid) {
						<ExecutedRequests<T>>::insert(&txid, request);
						Self::deposit_event(Event::RequestExecuted { txid });
					}
				}
			}

			Weight::from_parts(0, 0) // TODO: add weight
		}
	}

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		pub authority: Option<T::AccountId>,
		#[serde(skip)]
		pub _config: PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			if let Some(a) = &self.authority {
				Authority::<T>::put(a);
			}
			<MaxFeeRate<T>>::put(T::DefaultMaxFeeRate::get());
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T>
	where
		T::AccountId: Into<H160>,
		H160: Into<T::AccountId>,
	{
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		/// Set the authority address.
		pub fn set_authority(
			origin: OriginFor<T>,
			new: T::AccountId,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin.clone())?;

			if let Some(old) = <Authority<T>>::get() {
				ensure!(old != new, Error::<T>::NoWritingSameValue);
			}

			<Authority<T>>::put(new.clone());
			Self::deposit_event(Event::AuthoritySet { new });

			Ok(().into())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		/// Set the `Socket` or `BitcoinSocket` contract address.
		pub fn set_socket(
			origin: OriginFor<T>,
			new: T::AccountId,
			is_bitcoin: bool,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin.clone())?;

			if is_bitcoin {
				if let Some(old) = <BitcoinSocket<T>>::get() {
					ensure!(old != new, Error::<T>::NoWritingSameValue);
				}

				<BitcoinSocket<T>>::put(new.clone());
				Self::deposit_event(Event::SocketSet { new, is_bitcoin });
			} else {
				if let Some(old) = <Socket<T>>::get() {
					ensure!(old != new, Error::<T>::NoWritingSameValue);
				}

				<Socket<T>>::put(new.clone());
				Self::deposit_event(Event::SocketSet { new, is_bitcoin });
			}

			Ok(().into())
		}

		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		/// Submit an unsigned PSBT of an outbound request.
		/// This extrinsic can only be executed by the `Authority`.
		pub fn submit_unsigned_psbt(
			origin: OriginFor<T>,
			msg: UnsignedPsbtMessage<T::AccountId>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			let UnsignedPsbtMessage { outputs, psbt, .. } = msg;

			ensure!(
				T::RegistrationPool::get_service_state() == MigrationSequence::Normal,
				Error::<T>::UnderMaintenance
			);

			// verify if psbt bytes are valid
			let psbt_obj = Self::try_get_checked_psbt(&psbt)?;
			let txid = Self::convert_txid(psbt_obj.unsigned_tx.compute_txid());

			// verify if the fee rate is set properly
			Self::try_psbt_fee_verification(&psbt_obj)?;

			// prevent storage duplication
			ensure!(!<PendingRequests<T>>::contains_key(&txid), Error::<T>::RequestAlreadyExists);
			ensure!(!<FinalizedRequests<T>>::contains_key(&txid), Error::<T>::RequestAlreadyExists);
			ensure!(!<ExecutedRequests<T>>::contains_key(&txid), Error::<T>::RequestAlreadyExists);
			ensure!(!<RollbackRequests<T>>::contains_key(&txid), Error::<T>::RequestAlreadyExists);

			// verify PSBT outputs
			let (deserialized_msgs, serialized_msgs) =
				Self::try_psbt_output_verification(&psbt_obj, outputs)?;

			for msg in deserialized_msgs {
				<SocketMessages<T>>::insert(msg.req_id.sequence, (txid, msg));
			}
			<PendingRequests<T>>::insert(
				&txid,
				PsbtRequest::new(psbt.clone(), serialized_msgs, RequestType::Normal),
			);
			Self::deposit_event(Event::UnsignedPsbtSubmitted { txid });

			Ok(().into())
		}

		#[pallet::call_index(3)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		/// Submit a signed PSBT of a pending outbound request.
		/// This extrinsic can only be executed by relay executives.
		pub fn submit_signed_psbt(
			origin: OriginFor<T>,
			msg: SignedPsbtMessage<T::AccountId>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			let SignedPsbtMessage { authority_id, unsigned_psbt, signed_psbt } = msg;

			// verify if psbt bytes are valid
			let unsigned_psbt_obj = Self::try_get_checked_psbt(&unsigned_psbt)?;
			let signed_psbt_obj = Self::try_get_checked_psbt(&signed_psbt)?;

			let txid = Self::convert_txid(unsigned_psbt_obj.unsigned_tx.compute_txid());

			let mut pending_request =
				<PendingRequests<T>>::get(&txid).ok_or(Error::<T>::RequestDNE)?;

			// prevent storage duplications
			ensure!(
				!pending_request.is_signed_psbt_submitted(&signed_psbt),
				Error::<T>::SignedPsbtAlreadySubmitted
			);
			ensure!(!pending_request.is_unsigned_psbt(&signed_psbt), Error::<T>::InvalidPsbt);

			// combine signed PSBT
			let combined_psbt_obj = Self::try_psbt_combination(
				&mut Self::try_get_checked_psbt(&pending_request.combined_psbt)?,
				&signed_psbt_obj,
			)?;
			pending_request.set_combined_psbt(combined_psbt_obj.serialize());
			pending_request
				.signed_psbts
				.try_insert(authority_id.clone(), signed_psbt.clone())
				.map_err(|_| Error::<T>::OutOfRange)?;

			// if finalizable (quorum reached m), then accept the request
			match Self::try_psbt_finalization(combined_psbt_obj) {
				Ok(finalized_psbt_obj) => {
					pending_request.set_finalized_psbt(finalized_psbt_obj.serialize());

					// move pending to finalized
					<FinalizedRequests<T>>::insert(&txid, pending_request.clone());
					<PendingRequests<T>>::remove(&txid);

					if matches!(pending_request.request_type, RequestType::Normal) {
						<BondedOutboundTx<T>>::insert(&txid, pending_request.socket_messages);
					}

					Self::deposit_event(Event::SignedPsbtSubmitted { txid, authority_id });
					Self::deposit_event(Event::RequestFinalized { txid });
				},
				Err(_) => {
					// if not, remain as pending
					<PendingRequests<T>>::insert(&txid, pending_request);
					Self::deposit_event(Event::SignedPsbtSubmitted { txid, authority_id });
				},
			}

			Ok(().into())
		}

		#[pallet::call_index(4)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		/// Submit an executed PSBT request.
		pub fn submit_executed_request(
			origin: OriginFor<T>,
			msg: ExecutedPsbtMessage<T::AccountId>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			let ExecutedPsbtMessage { txid, .. } = msg;

			let request = <FinalizedRequests<T>>::get(&txid).ok_or(Error::<T>::RequestDNE)?;
			if request.request_type == RequestType::Migration {
				T::RegistrationPool::execute_migration_tx(txid.clone());
			}
			<FinalizedRequests<T>>::remove(&txid);
			<ExecutedRequests<T>>::insert(&txid, request);
			Self::deposit_event(Event::RequestExecuted { txid });

			Ok(().into())
		}

		#[pallet::call_index(5)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		/// Submit a rollback PSBT request.
		pub fn submit_rollback_request(
			origin: OriginFor<T>,
			msg: RollbackPsbtMessage<T::AccountId>,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;

			let RollbackPsbtMessage { who, txid: rollback_txid, vout, amount, unsigned_psbt } = msg;

			ensure!(
				T::RegistrationPool::get_service_state() == MigrationSequence::Normal,
				Error::<T>::UnderMaintenance
			);

			// verify if psbt bytes are valid
			let psbt_obj = Self::try_get_checked_psbt(&unsigned_psbt)?;
			let psbt_txid = Self::convert_txid(psbt_obj.unsigned_tx.compute_txid());

			// verify if the fee rate is set properly
			Self::try_psbt_fee_verification(&psbt_obj)?;

			// prevent double spend
			ensure!(
				!<PendingRequests<T>>::contains_key(&psbt_txid),
				Error::<T>::RequestAlreadyExists
			);
			ensure!(
				!<FinalizedRequests<T>>::contains_key(&psbt_txid),
				Error::<T>::RequestAlreadyExists
			);
			ensure!(
				!<ExecutedRequests<T>>::contains_key(&psbt_txid),
				Error::<T>::RequestAlreadyExists
			);
			ensure!(
				!<RollbackRequests<T>>::contains_key(&psbt_txid),
				Error::<T>::RequestAlreadyExists
			);
			ensure!(
				!<BondedRollbackOutputs<T>>::contains_key(&rollback_txid, &vout),
				Error::<T>::RequestAlreadyExists
			);

			// user information must exist
			let vault = T::RegistrationPool::get_vault_address(&who).ok_or(Error::<T>::UserDNE)?;
			let refund = Self::try_convert_to_address_from_vec(
				T::RegistrationPool::get_refund_address(&who).ok_or(Error::<T>::UserDNE)?,
			)?;

			// the request must not exist on-chain (=BitcoinSocket contract)
			let hash_key = Self::generate_hash_key(rollback_txid, vout, who.clone(), amount);
			let tx_info = Self::try_get_tx_info(hash_key)?;
			ensure!(tx_info.to.is_zero(), Error::<T>::RequestAlreadyExists);

			// the psbt must contain at max two outputs (system vault: if change exists, refund)
			let outputs = &psbt_obj.unsigned_tx.output;
			ensure!(outputs.len() <= 2, Error::<T>::InvalidPsbt);

			let current_round = T::RegistrationPool::get_current_round();
			let system_vault = Self::try_convert_to_address_from_vec(
				T::RegistrationPool::get_system_vault(current_round)
					.ok_or(Error::<T>::SystemVaultDNE)?,
			)?;

			for output in outputs {
				let to =
					Self::try_convert_to_address_from_script(output.script_pubkey.as_script())?;

				if to == system_vault {
					// if change exists, the psbt must contain exactly two outputs.
					ensure!(outputs.len() == 2, Error::<T>::InvalidPsbt);
					continue;
				}
				if to == refund {
					// the output amount must be less than the origin amount
					// (output.amount = origin amount - network fee)
					ensure!(
						Amount::from_sat(amount.as_u64()).checked_sub(output.value).is_some(),
						Error::<T>::InvalidPsbt
					);
					continue;
				}
				// addresses that are not either system vault or refund will be rejected.
				return Err(Error::<T>::InvalidPsbt.into());
			}

			<RollbackRequests<T>>::insert(
				&psbt_txid,
				RollbackRequest::new(unsigned_psbt, who, rollback_txid, vout, vault, amount),
			);
			<BondedRollbackOutputs<T>>::insert(rollback_txid, vout, psbt_txid);
			Self::deposit_event(Event::RollbackPsbtSubmitted { txid: psbt_txid });

			Ok(().into())
		}

		#[pallet::call_index(6)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		/// Submit a vote for a rollback request.
		pub fn submit_rollback_poll(
			origin: OriginFor<T>,
			msg: RollbackPollMessage<T::AccountId>,
			_signature: T::Signature,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			let RollbackPollMessage { authority_id, txid, is_approved } = msg;

			let mut rollback_request =
				<RollbackRequests<T>>::get(&txid).ok_or(Error::<T>::RequestDNE)?;
			ensure!(!rollback_request.is_approved, Error::<T>::RequestAlreadyApproved);

			if let Some(vote) = rollback_request.votes.get(&authority_id) {
				ensure!(*vote != is_approved, Error::<T>::NoWritingSameValue);
			}
			rollback_request
				.votes
				.try_insert(authority_id.clone(), is_approved)
				.map_err(|_| Error::<T>::OutOfRange)?;

			Self::deposit_event(Event::RollbackPollSubmitted { txid, authority_id, is_approved });

			if rollback_request.votes.iter().filter(|v| *v.1).count() as u32
				>= T::Relayers::majority()
			{
				// approve request and move the `PendingRequests`
				rollback_request.is_approved = true;
				<PendingRequests<T>>::insert(
					&txid,
					PsbtRequest::new(
						rollback_request.unsigned_psbt.clone(),
						vec![],
						RequestType::Rollback,
					),
				);
				Self::deposit_event(Event::RollbackApproved { txid });
			}
			<RollbackRequests<T>>::insert(&txid, rollback_request);

			Ok(().into())
		}

		#[pallet::call_index(7)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		/// Submit a migration PSBT request.
		pub fn submit_migration_request(
			origin: OriginFor<T>,
			psbt: UnboundedBytes,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;

			ensure!(
				T::RegistrationPool::get_service_state() == MigrationSequence::UTXOTransfer,
				Error::<T>::UnderMaintenance
			);

			// verify if psbt bytes are valid
			let psbt_obj = Self::try_get_checked_psbt(&psbt)?;
			let txid = Self::convert_txid(psbt_obj.unsigned_tx.compute_txid());

			// verify if the fee rate is set properly
			Self::try_psbt_fee_verification(&psbt_obj)?;

			// prevent storage duplication
			ensure!(!<PendingRequests<T>>::contains_key(&txid), Error::<T>::RequestAlreadyExists);
			ensure!(!<FinalizedRequests<T>>::contains_key(&txid), Error::<T>::RequestAlreadyExists);
			ensure!(!<ExecutedRequests<T>>::contains_key(&txid), Error::<T>::RequestAlreadyExists);

			// only one output for migrations (=system vault)
			let psbt_outputs = &psbt_obj.unsigned_tx.output;
			if psbt_outputs.len() != 1 {
				return Err(Error::<T>::InvalidPsbt.into());
			}

			let target_round = T::RegistrationPool::get_current_round().saturating_add(1);
			let system_vault = T::RegistrationPool::get_system_vault(target_round)
				.ok_or(Error::<T>::SystemVaultDNE)?;
			let to: BoundedBitcoinAddress = BoundedVec::try_from(
				Self::try_convert_to_address_from_script(
					psbt_outputs[0].script_pubkey.as_script(),
				)?
				.to_string()
				.as_bytes()
				.to_vec(),
			)
			.map_err(|_| Error::<T>::InvalidBitcoinAddress)?;
			if to != system_vault {
				return Err(Error::<T>::InvalidPsbt.into());
			}

			<PendingRequests<T>>::insert(
				&txid,
				PsbtRequest::new(psbt.clone(), vec![], RequestType::Migration),
			);
			T::RegistrationPool::add_migration_tx(txid.clone());
			Self::deposit_event(Event::MigrationPsbtSubmitted { txid });
			Ok(().into())
		}

		#[pallet::call_index(8)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		/// Set the maximum fee rate for the PSBT.
		pub fn set_max_fee_rate(origin: OriginFor<T>, new: u64) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;

			let old = <MaxFeeRate<T>>::get();
			ensure!(old != new, Error::<T>::NoWritingSameValue);

			// overflow check
			FeeRate::from_sat_per_vb(new).ok_or(Error::<T>::OutOfRange)?;

			<MaxFeeRate<T>>::put(new);
			Self::deposit_event(Event::MaxFeeRateSet { new });

			Ok(().into())
		}

		#[pallet::call_index(9)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		/// Submit an unsigned PSBT to Replace-by-Fee (RBF) a pending Bitcoin transaction.
		/// The `new_unsigned_psbt` must be generated by the `psbtbumpfee` RPC.
		pub fn submit_bump_fee_request(
			origin: OriginFor<T>,
			old_txid: H256,
			new_unsigned_psbt: UnboundedBytes,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;

			// the pending request (stucked in the Bitcoin mempool) should exist as an `ExecutedRequest`
			let old_request =
				<ExecutedRequests<T>>::get(&old_txid).ok_or(Error::<T>::RequestDNE)?;
			ensure!(old_request.unsigned_psbt != new_unsigned_psbt, Error::<T>::NoWritingSameValue);

			match old_request.request_type {
				RequestType::Migration => {
					ensure!(
						T::RegistrationPool::get_service_state() == MigrationSequence::UTXOTransfer,
						Error::<T>::UnderMaintenance
					);
				},
				_ => {
					ensure!(
						T::RegistrationPool::get_service_state() == MigrationSequence::Normal,
						Error::<T>::UnderMaintenance
					);
				},
			}

			// verify if psbt bytes are valid
			let old_psbt_obj = Self::try_get_checked_psbt(&old_request.unsigned_psbt)?;
			let new_psbt_obj = Self::try_get_checked_psbt(&new_unsigned_psbt)?;
			let new_txid = Self::convert_txid(new_psbt_obj.unsigned_tx.compute_txid());
			ensure!(new_txid != old_txid, Error::<T>::NoWritingSameValue);

			// verify if the fee rate is set properly
			Self::try_psbt_fee_verification(&new_psbt_obj)?;

			// verify if psbt is valid for RBF.
			Self::try_bump_fee_psbt_verification(
				&old_psbt_obj,
				&new_psbt_obj,
				&old_request.request_type,
			)?;

			// prevent storage duplication
			ensure!(
				!<PendingRequests<T>>::contains_key(&new_txid),
				Error::<T>::RequestAlreadyExists
			);
			ensure!(
				!<FinalizedRequests<T>>::contains_key(&new_txid),
				Error::<T>::RequestAlreadyExists
			);
			ensure!(
				!<ExecutedRequests<T>>::contains_key(&new_txid),
				Error::<T>::RequestAlreadyExists
			);
			ensure!(
				!<RollbackRequests<T>>::contains_key(&new_txid),
				Error::<T>::RequestAlreadyExists
			);

			match old_request.request_type {
				RequestType::Normal => {
					// replace stored `SocketMessages` to pair with the new txid
					for socket_message in old_request.socket_messages.clone() {
						let msg = Self::try_decode_socket_message(&socket_message)
							.map_err(|_| Error::<T>::InvalidSocketMessage)?;
						<SocketMessages<T>>::insert(msg.req_id.sequence, (new_txid, msg));
					}
					<BondedOutboundTx<T>>::remove(old_txid);
				},
				RequestType::Migration => {
					// update OngoingVaultMigration
					T::RegistrationPool::remove_migration_tx(old_txid);
					T::RegistrationPool::add_migration_tx(new_txid);
				},
				RequestType::Rollback => {
					// update RollbackRequests
					let mut rollback_request =
						<RollbackRequests<T>>::take(&old_txid).ok_or(Error::<T>::RequestDNE)?;
					rollback_request.unsigned_psbt = new_unsigned_psbt.clone();
					<RollbackRequests<T>>::insert(&new_txid, rollback_request.clone());

					// (re-)insert BondedRollbackOutputs
					<BondedRollbackOutputs<T>>::insert(
						rollback_request.txid,
						rollback_request.clone().vout,
						new_txid,
					);
				},
			}
			<ExecutedRequests<T>>::remove(old_txid);

			// insert to PendingRequests
			<PendingRequests<T>>::insert(
				&new_txid,
				PsbtRequest::new(
					new_unsigned_psbt.clone(),
					old_request.socket_messages,
					old_request.request_type,
				),
			);
			Self::deposit_event(Event::BumpFeePsbtSubmitted { old_txid, new_txid });

			Ok(().into())
		}

		#[pallet::call_index(10)]
		#[pallet::weight(<T as Config>::WeightInfo::default())]
		/// Drop a pending rollback request from `RollbackRequests`.
		pub fn drop_pending_rollback_request(
			origin: OriginFor<T>,
			txid: H256,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;

			ensure!(
				T::RegistrationPool::get_service_state() == MigrationSequence::Normal,
				Error::<T>::UnderMaintenance
			);

			let pending_request =
				<RollbackRequests<T>>::get(&txid).ok_or(Error::<T>::RequestDNE)?;
			ensure!(!pending_request.is_approved, Error::<T>::RequestDNE);

			<RollbackRequests<T>>::remove(&txid);

			Ok(().into())
		}
	}

	#[pallet::validate_unsigned]
	impl<T: Config> ValidateUnsigned for Pallet<T>
	where
		T::AccountId: Into<H160>,
		H160: Into<T::AccountId>,
	{
		type Call = Call<T>;

		fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
			match call {
				Call::submit_unsigned_psbt { msg, signature } => {
					let UnsignedPsbtMessage { authority_id, psbt, .. } = msg;
					Self::verify_authority(authority_id)?;

					// verify if the signature was originated from the authority_id.
					let message = [keccak_256("UnsignedPsbt".as_bytes()).as_slice(), psbt].concat();
					if !signature.verify(&*message, authority_id) {
						return InvalidTransaction::BadProof.into();
					}

					ValidTransaction::with_tag_prefix("UnsignedPsbtSubmission")
						.priority(TransactionPriority::MAX)
						.and_provides(authority_id)
						.propagate(true)
						.build()
				},
				Call::submit_signed_psbt { msg, signature } => {
					let SignedPsbtMessage { authority_id, signed_psbt, .. } = msg;

					// verify if the authority is a relay executive member.
					if !T::Executives::contains(&authority_id) {
						return InvalidTransaction::BadSigner.into();
					}

					// verify if the signature was originated from the authority.
					let message =
						[keccak_256("SignedPsbt".as_bytes()).as_slice(), signed_psbt].concat();
					if !signature.verify(&*message, authority_id) {
						return InvalidTransaction::BadProof.into();
					}

					ValidTransaction::with_tag_prefix("SignedPsbtSubmission")
						.priority(TransactionPriority::MAX)
						.and_provides(authority_id)
						.propagate(true)
						.build()
				},
				Call::submit_executed_request { msg, signature } => {
					let ExecutedPsbtMessage { authority_id, txid } = msg;
					Self::verify_authority(authority_id)?;

					// verify if the signature was originated from the authority_id.
					let message =
						[keccak_256("ExecutedPsbt".as_bytes()).as_slice(), txid.as_ref()].concat();
					if !signature.verify(&*message, authority_id) {
						return InvalidTransaction::BadProof.into();
					}

					ValidTransaction::with_tag_prefix("ExecutedPsbtSubmission")
						.priority(TransactionPriority::MAX)
						.and_provides(authority_id)
						.propagate(true)
						.build()
				},
				Call::submit_rollback_poll { msg, signature } => {
					let RollbackPollMessage { authority_id, txid, is_approved } = msg;

					// verify if the authority is a selected relayer.
					if !T::Relayers::is_authority(&authority_id) {
						return InvalidTransaction::BadSigner.into();
					}

					// verify if the signature was originated from the authority_id.
					let message = [
						keccak_256("RollbackPoll".as_bytes()).as_slice(),
						txid.as_ref(),
						&[*is_approved as u8],
					]
					.concat();
					if !signature.verify(&*message, authority_id) {
						return InvalidTransaction::BadProof.into();
					}

					ValidTransaction::with_tag_prefix("RollbackPollSubmission")
						.priority(TransactionPriority::MAX)
						.and_provides(authority_id)
						.propagate(true)
						.build()
				},
				_ => InvalidTransaction::Call.into(),
			}
		}
	}
}
