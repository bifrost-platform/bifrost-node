use crate::{
	ChainId, ProductId, ReceiveEntry, ReceiveKind, RequestEntry, RequestId, RequestOpening,
	RequestStep, SettlementChainEntry, SettlementId, SettlementStep, TxRecord, WeightInfo,
	MAX_SPOKE_CHAINS,
};
use pallet_tranche_system::{AdapterInspect, RequestSettlementInspect, VaultId, VaultInspect};

use frame_support::{pallet_prelude::*, traits::StorageVersion};
use frame_system::pallet_prelude::*;
use sp_core::{ConstU32, H160, H256};
use sp_std::vec::Vec;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Only accepted origin for all `record_*` extrinsics. Wire as
		/// `type RecorderOrigin = pallet_tranche_tx_registry::EnsureTxRecorder<Runtime>` in
		/// the runtime — mirrors `pallet_tranche_investments::Config::ValuationOrigin`,
		/// except this checks an ordinary signed origin against the `TxRecorder` storage
		/// value directly rather than a precompile-constructed custom `Origin` variant.
		type RecorderOrigin: EnsureOrigin<Self::RuntimeOrigin>;
		/// Vault inspector — implemented by pallet-tranche-system. Used to
		/// verify a vault actually belongs to `product_id` before recording a
		/// request or receive against it.
		type Vaults: VaultInspect;
		/// Adapter inspector — implemented by pallet-tranche-system. Used to
		/// verify a settlement's spoke chains are ones `product_id` actually
		/// has a MultichainAdapter on, before recording a Trigger against them.
		type Adapters: AdapterInspect;
		/// Request-settlement linkage inspector — implemented by
		/// pallet-tranche-investments (see `RequestSettlementInspect`'s doc comment
		/// for why it's hosted in pallet-tranche-system instead). Used by
		/// `record_settlement_tx` to automatically close out `InvestorActiveRequests`
		/// entries when a settlement's Finalize-Hooks leg lands.
		type Investments: RequestSettlementInspect;
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	// -----------------------------------------------------------------------
	// Errors
	// -----------------------------------------------------------------------

	#[pallet::error]
	pub enum Error<T> {
		/// The vault does not belong to `product_id`.
		VaultNotRegistered,
		/// One of `spoke_chain_ids` is not a chain `product_id` has a
		/// MultichainAdapter registered on.
		SpokeChainNotRegistered,
		/// A registry entry already exists for this (product_id, request_id).
		RequestAlreadyOpened,
		/// No registry entry exists yet for this (product_id, request_id).
		RequestNotOpened,
		/// `opening` must be `Some` when `step == RequestStep::Requested`.
		RequestOpeningRequired,
		/// `opening` must be `None` for every step other than `Requested`.
		UnexpectedRequestOpening,
		/// The step being recorded skips over an earlier, not-yet-recorded step.
		RequestStepOutOfOrder,
		/// This step has already been recorded for this request.
		RequestStepAlreadyRecorded,
		/// `step` must be one of the seven recordable values — never
		/// `SettlementStep::Queued`/`Settled`, both read-only sentinels.
		InvalidSettlementStep,
		/// `spoke_chain_ids` must be `Some` and non-empty when
		/// `step == SettlementStep::Triggered`.
		SpokeChainIdsRequired,
		/// `spoke_chain_ids` must be `None` for every step other than `Triggered`.
		UnexpectedSpokeChainIds,
		/// `spoke_chain_id` must be `Some` for every step other than `Triggered`.
		SpokeChainIdRequired,
		/// `spoke_chain_id` must be `None` when `step == SettlementStep::Triggered`.
		UnexpectedSpokeChainId,
		/// Trigger has already been recorded for this (product_id, settlement_id).
		SettlementAlreadyTriggered,
		/// Trigger has not been recorded yet for this (product_id, settlement_id).
		SettlementNotTriggered,
		/// `spoke_chain_id` is not among the chains registered at Trigger time.
		UnknownSpokeChain,
		/// The leg step being recorded skips over its Bridge phase.
		SettlementStepOutOfOrder,
		/// This leg step has already been recorded for this chain.
		SettlementStepAlreadyRecorded,
		/// A receive has already been recorded for this (investor, vault, tx_hash).
		ReceiveAlreadyRecorded,
	}

	// -----------------------------------------------------------------------
	// Events
	// -----------------------------------------------------------------------

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// The tx recorder account was (re)configured via `set_tx_recorder`.
		TxRecorderSet { old: Option<T::AccountId>, new: T::AccountId },
		/// One tx in a request's 3-tx pipeline was recorded.
		RequestTxRecorded {
			product_id: ProductId,
			request_id: RequestId,
			opening: Option<RequestOpening>,
			step: RequestStep,
			chain_id: ChainId,
			tx_hash: H256,
		},
		/// One tx in a settlement's pipeline was recorded.
		SettlementTxRecorded {
			product_id: ProductId,
			settlement_id: SettlementId,
			spoke_chain_id: Option<ChainId>,
			spoke_chain_ids: Option<BoundedVec<ChainId, ConstU32<MAX_SPOKE_CHAINS>>>,
			step: SettlementStep,
			chain_id: ChainId,
			tx_hash: H256,
		},
		/// A claim() tx was recorded.
		ReceiveTxRecorded {
			product_id: ProductId,
			vault: VaultId,
			investor: H160,
			kind: ReceiveKind,
			chain_id: ChainId,
			tx_hash: H256,
		},
		/// `(product_id, request_id)` was automatically removed from `investor`'s
		/// `InvestorActiveRequests` list — a side effect of `record_settlement_tx`
		/// recording that request's own origin chain reaching
		/// `SettlementStep::FinalizeHooksExecuted` for the settlement it was
		/// approved into.
		ActiveRequestClosed { product_id: ProductId, request_id: RequestId, investor: H160 },
	}

	// -----------------------------------------------------------------------
	// Storage
	// -----------------------------------------------------------------------

	#[pallet::storage]
	/// The single account permitted to submit `record_*` extrinsics. Set via the
	/// Root-gated `set_tx_recorder` extrinsic — never exposed through the EVM
	/// precompile interface itself, same pattern as pallet-tranche-system's
	/// Orchestrator registration. See `crate::EnsureTxRecorder`, which reads
	/// this value.
	pub type TxRecorder<T: Config> = StorageValue<_, T::AccountId>;

	#[pallet::storage]
	/// A request's 3-tx registry entry. Keyed by `(product_id, request_id)`,
	/// NOT `request_id` alone — `request_id` is only unique within a product's
	/// own namespace (each product's Valuation Contract generates its own
	/// sequence), same rationale as
	/// `pallet_tranche_investments::RequestedInvestments`. Opened by
	/// `record_request_tx`'s `RequestStep::Requested` step; the other two
	/// steps (`BridgeExecuted`/`HooksExecuted`) fill in `bridge_tx`/`hooks_tx`
	/// on the existing entry rather than inserting a new one.
	pub type RequestEntries<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		ProductId,
		Blake2_128Concat,
		RequestId,
		RequestEntry<BlockNumberFor<T>>,
	>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// An investor's currently in-flight requests — registered here the moment
	/// their `RequestEntries` entry is opened (`RequestStep::Requested`), removed
	/// automatically by `record_settlement_tx` when it records a settlement's
	/// Finalize-Hooks leg: at that point it asks
	/// `T::Investments::settlement_requests(product_id, settlement_id)` (implemented
	/// by pallet-tranche-investments, see `RequestSettlementInspect`'s doc comment
	/// for why this doesn't require a hard dependency on that pallet) for every
	/// request_id approved into that settlement, and removes the ones whose own
	/// origin chain (`RequestEntry::vault::chain_id`) matches the leg just
	/// finalized. Bounded by `pallet_tranche_investments::MAX_SETTLEMENT_REQUESTS`
	/// on the writing side, so this stays a fixed-cost operation rather than an
	/// unbounded scan.
	///
	/// Deliberately unbounded — an investor legitimately opening requests
	/// across many products concurrently shouldn't be capped by an arbitrary
	/// limit; `#[pallet::unbounded]` is required since plain `Vec` has no
	/// `MaxEncodedLen` impl, same pattern as `pallet_tranche_system::Products`
	/// (see its own doc comment). Keyed by `H160`, not `T::AccountId` — same
	/// reasoning as `RequestEntry::investor`.
	pub type InvestorActiveRequests<T: Config> =
		StorageMap<_, Blake2_128Concat, H160, Vec<(ProductId, RequestId)>, ValueQuery>;

	#[pallet::storage]
	/// A settlement's Trigger evidence. Keyed by `(product_id, settlement_id)`
	/// — `settlement_id` is only unique within `product_id`'s own namespace,
	/// same rationale as `RequestEntries`' key shape. Presence of an entry
	/// here (rather than a `SettlementStep::Queued`-tagged value) is what
	/// answers "has this settlement been triggered yet" — mirrors how
	/// `RequestEntries` uses entry-presence rather than an explicit sentinel
	/// step.
	pub type SettlementTriggers<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		ProductId,
		Blake2_128Concat,
		SettlementId,
		TxRecord<BlockNumberFor<T>>,
	>;

	#[pallet::storage]
	/// The spoke chains registered for a settlement at Trigger time, in the
	/// order the recorder supplied them. Always written alongside
	/// `SettlementTriggers` (both by the same `record_settlement_tx` call for
	/// `step == Triggered`) — kept as a separate storage item rather than
	/// folded into one struct, same pattern already used by
	/// `pallet_tranche_investments`' `AdapterValuations`/`ProductNavs`/
	/// `TrancheSettlements` (three separate maps written together by one
	/// extrinsic).
	pub type SettlementSpokeChains<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		ProductId,
		Blake2_128Concat,
		SettlementId,
		BoundedVec<ChainId, ConstU32<MAX_SPOKE_CHAINS>>,
	>;

	#[pallet::storage]
	/// One spoke chain's full leg-by-leg registry entry within a
	/// settlement. Keyed by `(product_id, settlement_id, spoke_chain_id)`.
	/// `ValueQuery` with `SettlementChainEntry`'s `Default` impl (rather than
	/// `OptionQuery`) since every field inside is independently `Option`-typed
	/// already — a fully-empty default entry and "nothing recorded for this
	/// chain yet" are the same state, so there's no need to additionally wrap
	/// the whole entry in `Option`.
	pub type SettlementChainEntries<T: Config> = StorageNMap<
		_,
		(
			NMapKey<Blake2_128Concat, ProductId>,
			NMapKey<Blake2_128Concat, SettlementId>,
			NMapKey<Blake2_128Concat, ChainId>,
		),
		SettlementChainEntry<BlockNumberFor<T>>,
		ValueQuery,
	>;

	#[pallet::storage]
	/// Every claim() tx recorded, one storage slot per receive. Keyed by
	/// `(investor, vault, tx_hash)` — `product_id` is deliberately not part
	/// of the key at all: `VaultId` is already globally unique (enforced by
	/// pallet-tranche-system), so it would be redundant for addressing
	/// purposes. `tx_hash` is the receive's own natural unique identifier, so
	/// there's no bounded-size cap or eviction logic needed the way a
	/// `Vec`-valued map would require. TrancheManager pools receivable
	/// amounts per (investor, vault), not per request_id, so there is no
	/// single request_id a receive could be keyed by instead.
	///
	/// Enumerate one investor's receive history for one vault via
	/// `iter_prefix((investor, vault))`.
	pub type ReceiveEntries<T: Config> = StorageNMap<
		_,
		(
			NMapKey<Blake2_128Concat, H160>,
			NMapKey<Blake2_128Concat, VaultId>,
			NMapKey<Blake2_128Concat, H256>,
		),
		ReceiveEntry<BlockNumberFor<T>>,
	>;

	// -----------------------------------------------------------------------
	// Extrinsics
	// -----------------------------------------------------------------------

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Set the account permitted to submit `record_*` extrinsics, replacing any
		/// previous one. Root-gated — never exposed through the EVM precompile
		/// interface (see `crate::EnsureTxRecorder`'s doc comment).
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::set_tx_recorder())]
		pub fn set_tx_recorder(origin: OriginFor<T>, recorder: T::AccountId) -> DispatchResult {
			ensure_root(origin)?;

			let old = TxRecorder::<T>::get();
			TxRecorder::<T>::put(&recorder);

			Self::deposit_event(Event::TxRecorderSet { old, new: recorder });
			Ok(())
		}

		/// Attest to one tx in a request's 3-tx pipeline. Origin must be
		/// `RecorderOrigin`. `opening` MUST be `Some` iff `step ==
		/// RequestStep::Requested` — see interface.sol's `record_request_tx` for
		/// the full sentinel-gating/ordering contract this mirrors.
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::record_request_tx())]
		pub fn record_request_tx(
			origin: OriginFor<T>,
			product_id: ProductId,
			request_id: RequestId,
			opening: Option<RequestOpening>,
			step: RequestStep,
			chain_id: ChainId,
			tx_hash: H256,
		) -> DispatchResult {
			T::RecorderOrigin::ensure_origin(origin)?;

			let recorded_at = frame_system::Pallet::<T>::block_number();
			let tx = TxRecord { chain_id, tx_hash, recorded_at };

			match step {
				RequestStep::Requested => {
					let opening = opening.clone().ok_or(Error::<T>::RequestOpeningRequired)?;
					ensure!(
						T::Vaults::vault_belongs_to_product(product_id, &opening.vault),
						Error::<T>::VaultNotRegistered
					);
					ensure!(
						!RequestEntries::<T>::contains_key(product_id, request_id),
						Error::<T>::RequestAlreadyOpened
					);
					RequestEntries::<T>::insert(
						product_id,
						request_id,
						RequestEntry {
							product_id,
							vault: opening.vault,
							investor: opening.investor,
							amount: opening.amount,
							order_type: opening.order_type,
							request_tx: Some(tx),
							bridge_tx: None,
							hooks_tx: None,
						},
					);
					InvestorActiveRequests::<T>::mutate(opening.investor, |requests| {
						requests.push((product_id, request_id));
					});
				},
				RequestStep::BridgeExecuted => {
					ensure!(opening.is_none(), Error::<T>::UnexpectedRequestOpening);
					// `entry`'s mere existence already guarantees `request_tx.is_some()` — it's
					// only ever inserted by the `Requested` arm above, which always sets it,
					// and no path clears it afterward — so there's no separate ordering check
					// to make here beyond the duplicate check below.
					let mut entry = RequestEntries::<T>::get(product_id, request_id)
						.ok_or(Error::<T>::RequestNotOpened)?;
					ensure!(entry.bridge_tx.is_none(), Error::<T>::RequestStepAlreadyRecorded);
					entry.bridge_tx = Some(tx);
					RequestEntries::<T>::insert(product_id, request_id, entry);
				},
				RequestStep::HooksExecuted => {
					ensure!(opening.is_none(), Error::<T>::UnexpectedRequestOpening);
					let mut entry = RequestEntries::<T>::get(product_id, request_id)
						.ok_or(Error::<T>::RequestNotOpened)?;
					ensure!(entry.bridge_tx.is_some(), Error::<T>::RequestStepOutOfOrder);
					ensure!(entry.hooks_tx.is_none(), Error::<T>::RequestStepAlreadyRecorded);
					entry.hooks_tx = Some(tx);
					RequestEntries::<T>::insert(product_id, request_id, entry);
				},
			}

			Self::deposit_event(Event::RequestTxRecorded {
				product_id,
				request_id,
				opening,
				step,
				chain_id,
				tx_hash,
			});
			Ok(())
		}

		/// Attest to one tx in a settlement's pipeline: either the single Trigger
		/// tx, or one bridge/hooks half of a Collect/Response/Finalize leg for one
		/// chain. Origin must be `RecorderOrigin`. `spoke_chain_ids` MUST be `Some`
		/// (and non-empty) iff `step == SettlementStep::Triggered`; `spoke_chain_id`
		/// MUST be `Some` for every other step — see interface.sol's
		/// `record_settlement_tx` for the full contract this mirrors.
		///
		/// Side effect on `step == SettlementStep::FinalizeHooksExecuted`: also
		/// closes out `InvestorActiveRequests` for every request approved into
		/// this settlement whose own origin chain is `spoke_chain_id` — see
		/// `InvestorActiveRequests`'s storage doc comment for the full mechanism
		/// (`T::Investments::settlement_requests` + `ActiveRequestClosed`).
		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::record_settlement_tx())]
		pub fn record_settlement_tx(
			origin: OriginFor<T>,
			product_id: ProductId,
			settlement_id: SettlementId,
			spoke_chain_id: Option<ChainId>,
			spoke_chain_ids: Option<BoundedVec<ChainId, ConstU32<MAX_SPOKE_CHAINS>>>,
			step: SettlementStep,
			chain_id: ChainId,
			tx_hash: H256,
		) -> DispatchResult {
			T::RecorderOrigin::ensure_origin(origin)?;

			let recorded_at = frame_system::Pallet::<T>::block_number();
			let tx = TxRecord { chain_id, tx_hash, recorded_at };

			if step == SettlementStep::Triggered {
				ensure!(spoke_chain_id.is_none(), Error::<T>::UnexpectedSpokeChainId);
				let chains = spoke_chain_ids.clone().ok_or(Error::<T>::SpokeChainIdsRequired)?;
				ensure!(!chains.is_empty(), Error::<T>::SpokeChainIdsRequired);
				ensure!(
					T::Adapters::spoke_chains_belong_to_product(product_id, &chains),
					Error::<T>::SpokeChainNotRegistered
				);
				ensure!(
					!SettlementTriggers::<T>::contains_key(product_id, settlement_id),
					Error::<T>::SettlementAlreadyTriggered
				);
				SettlementTriggers::<T>::insert(product_id, settlement_id, tx);
				SettlementSpokeChains::<T>::insert(product_id, settlement_id, chains);
			} else {
				ensure!(spoke_chain_ids.is_none(), Error::<T>::UnexpectedSpokeChainIds);
				let spoke_chain_id = spoke_chain_id.ok_or(Error::<T>::SpokeChainIdRequired)?;
				let chains = SettlementSpokeChains::<T>::get(product_id, settlement_id)
					.ok_or(Error::<T>::SettlementNotTriggered)?;
				ensure!(chains.contains(&spoke_chain_id), Error::<T>::UnknownSpokeChain);

				let mut entry =
					SettlementChainEntries::<T>::get((product_id, settlement_id, spoke_chain_id));
				match step {
					SettlementStep::CollectBridgeExecuted => {
						ensure!(
							entry.collect_bridge_tx.is_none(),
							Error::<T>::SettlementStepAlreadyRecorded
						);
						entry.collect_bridge_tx = Some(tx);
					},
					SettlementStep::CollectHooksExecuted => {
						ensure!(
							entry.collect_bridge_tx.is_some(),
							Error::<T>::SettlementStepOutOfOrder
						);
						ensure!(
							entry.collect_hooks_tx.is_none(),
							Error::<T>::SettlementStepAlreadyRecorded
						);
						entry.collect_hooks_tx = Some(tx);
					},
					SettlementStep::ResponseBridgeExecuted => {
						ensure!(
							entry.response_bridge_tx.is_none(),
							Error::<T>::SettlementStepAlreadyRecorded
						);
						entry.response_bridge_tx = Some(tx);
					},
					SettlementStep::ResponseHooksExecuted => {
						ensure!(
							entry.response_bridge_tx.is_some(),
							Error::<T>::SettlementStepOutOfOrder
						);
						ensure!(
							entry.response_hooks_tx.is_none(),
							Error::<T>::SettlementStepAlreadyRecorded
						);
						entry.response_hooks_tx = Some(tx);
					},
					SettlementStep::FinalizeBridgeExecuted => {
						ensure!(
							entry.finalize_bridge_tx.is_none(),
							Error::<T>::SettlementStepAlreadyRecorded
						);
						entry.finalize_bridge_tx = Some(tx);
					},
					SettlementStep::FinalizeHooksExecuted => {
						ensure!(
							entry.finalize_bridge_tx.is_some(),
							Error::<T>::SettlementStepOutOfOrder
						);
						ensure!(
							entry.finalize_hooks_tx.is_none(),
							Error::<T>::SettlementStepAlreadyRecorded
						);
						entry.finalize_hooks_tx = Some(tx);
					},
					SettlementStep::Queued
					| SettlementStep::Triggered
					| SettlementStep::Settled => {
						return Err(Error::<T>::InvalidSettlementStep.into());
					},
				}
				SettlementChainEntries::<T>::insert(
					(product_id, settlement_id, spoke_chain_id),
					entry,
				);

				if step == SettlementStep::FinalizeHooksExecuted {
					for request_id in T::Investments::settlement_requests(product_id, settlement_id)
					{
						let Some(request_entry) = RequestEntries::<T>::get(product_id, request_id)
						else {
							continue;
						};
						if request_entry.vault.chain_id != spoke_chain_id {
							continue;
						}
						let mut requests = InvestorActiveRequests::<T>::get(request_entry.investor);
						let Some(pos) =
							requests.iter().position(|r| *r == (product_id, request_id))
						else {
							continue;
						};
						requests.swap_remove(pos);
						InvestorActiveRequests::<T>::insert(request_entry.investor, requests);
						Self::deposit_event(Event::ActiveRequestClosed {
							product_id,
							request_id,
							investor: request_entry.investor,
						});
					}
				}
			}

			Self::deposit_event(Event::SettlementTxRecorded {
				product_id,
				settlement_id,
				spoke_chain_id,
				spoke_chain_ids,
				step,
				chain_id,
				tx_hash,
			});
			Ok(())
		}

		/// Attest to an investor's claim() tx on a vault — a plain local Spoke-chain
		/// tx, not part of the Bridge&Call request/settlement pipelines above.
		/// Origin must be `RecorderOrigin`. See interface.sol's `record_receive_tx`
		/// for the full contract this mirrors.
		#[pallet::call_index(3)]
		#[pallet::weight(<T as Config>::WeightInfo::record_receive_tx())]
		pub fn record_receive_tx(
			origin: OriginFor<T>,
			product_id: ProductId,
			vault: VaultId,
			investor: H160,
			kind: ReceiveKind,
			chain_id: ChainId,
			tx_hash: H256,
		) -> DispatchResult {
			T::RecorderOrigin::ensure_origin(origin)?;

			ensure!(
				T::Vaults::vault_belongs_to_product(product_id, &vault),
				Error::<T>::VaultNotRegistered
			);
			ensure!(
				!ReceiveEntries::<T>::contains_key((investor, vault.clone(), tx_hash)),
				Error::<T>::ReceiveAlreadyRecorded
			);

			let recorded_at = frame_system::Pallet::<T>::block_number();
			let tx = TxRecord { chain_id, tx_hash, recorded_at };
			ReceiveEntries::<T>::insert(
				(investor, vault.clone(), tx_hash),
				ReceiveEntry { tx, kind },
			);

			Self::deposit_event(Event::ReceiveTxRecorded {
				product_id,
				vault,
				investor,
				kind,
				chain_id,
				tx_hash,
			});
			Ok(())
		}
	}
}
