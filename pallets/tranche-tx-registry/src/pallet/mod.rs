mod impls;

use crate::{
	ChainId, ProductId, ReceiveEntry, ReceiveKind, RequestChainEntry, RequestEntry, RequestId,
	RequestOpening, RequestStep, SettlementChainEntry, SettlementId, SettlementStep, TxRecord,
	WeightInfo, WhitelistEntry, WhitelistNonce, WhitelistStep, MAX_SPOKE_CHAINS,
};
use pallet_tranche_system::{AdapterInspect, RequestSettlementInspect, VaultId, VaultInspect};

use frame_support::{pallet_prelude::*, traits::StorageVersion};
use frame_system::pallet_prelude::*;
use sp_core::{ConstU32, H160, H256, U256};
use sp_std::vec::Vec;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	/// `pallet_evm::Config` supplies `<Self as pallet_evm::Config>::ChainId`, this chain's
	/// own EVM chain ID — needed by `record_request_tx` to tell a Hub-vault request (no
	/// Inbound leg — `RequestQueued` follows `Requested` immediately) apart from a
	/// Spoke-vault one (Inbound leg required — `RequestQueued` only reachable once
	/// `RequestBridgeExecuted` has landed) — see `RequestStep`'s doc comment.
	pub trait Config: frame_system::Config + pallet_evm::Config {
		/// Only accepted origin for all `record_*` extrinsics. Wire as
		/// `type RecorderOrigin = pallet_tranche_tx_registry::EnsureTxRecorder<Runtime>` in
		/// the runtime — mirrors `pallet_tranche_investments::Config::ValuationOrigin`,
		/// except this checks an ordinary signed origin against the `TxRecorder` storage
		/// value directly rather than a precompile-constructed custom `Origin` variant.
		type RecorderOrigin: EnsureOrigin<Self::RuntimeOrigin>;
		/// Vault inspector — implemented by pallet-tranche-system. Used to verify a
		/// vault actually belongs to `product_id` before recording a request or
		/// receive against it, and to verify a settlement's declared
		/// `finalize_chain_ids` each have a registered vault.
		type Vaults: VaultInspect;
		/// Adapter inspector — implemented by pallet-tranche-system. Used to verify
		/// a request's declared `adapter_chain_ids`, or a settlement's
		/// declared `collect_response_chain_ids`, each have a registered
		/// MultichainAdapter, before recording against them.
		type Adapters: AdapterInspect;
		/// Request-settlement linkage inspector — implemented by
		/// pallet-tranche-investments (see `RequestSettlementInspect`'s doc comment
		/// for why it's hosted in pallet-tranche-system instead). Used by
		/// `record_settlement_tx` to automatically close out `InvestorActiveRequests`
		/// entries when a settlement's `SettleApplied`/`NavReceived` leg lands.
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
		/// One of the declared chains doesn't have the role required for the set it
		/// was declared in: `adapter_chain_ids` (Request) or
		/// `collect_response_chain_ids` (Settlement) requires a registered
		/// MultichainAdapter on that chain; `finalize_chain_ids` (Settlement)
		/// requires a registered tranche vault.
		SpokeChainNotRegistered,
		/// A registry entry already exists for this (product_id, request_id).
		RequestAlreadyOpened,
		/// No registry entry exists yet for this (product_id, request_id).
		RequestNotOpened,
		/// `opening` must be `Some` when `step == RequestStep::Requested`.
		RequestOpeningRequired,
		/// `opening` must be `None` for every step other than `Requested`.
		UnexpectedRequestOpening,
		/// `adapter_chain_ids` must be `Some` when `step == RequestStep::RequestQueued` —
		/// the moment the request's capital is confirmed at the Valuation Contract,
		/// Hub-vault or Spoke-vault alike (for Spoke-vault, only reachable once the
		/// Inbound leg's own `RequestBridgeExecuted` has landed). See `RequestStep`'s doc
		/// comment for why it's never known any earlier, at `Requested` itself.
		RequestAdapterChainsRequired,
		/// `adapter_chain_ids` must be `None` everywhere else — including
		/// `step == RequestStep::Requested`, Hub-vault or Spoke-vault alike (deferred to
		/// `RequestQueued` instead).
		UnexpectedRequestAdapterChains,
		/// A request's `RequestAdapterChains` can't hold any more distinct chains —
		/// bounded by `MAX_SPOKE_CHAINS`. Surfaced both by `RequestQueued`'s own
		/// declared `adapter_chain_ids` and by `AdapterBridgeExecuted`/`AdapterApplied`
		/// self-declaring a not-yet-seen chain (see `Pallet::ensure_adapter_chain_declared`).
		TooManyAdapterChains,
		/// `step == RequestStep::RequestBridgeExecuted` was recorded for a request whose
		/// vault is on Hub — a Hub-vault request has no Inbound leg at all (there's
		/// nothing to bridge when the vault is already on Hub).
		UnexpectedInboundLeg,
		/// The step being recorded skips over an earlier, not-yet-recorded step.
		RequestStepOutOfOrder,
		/// This step has already been recorded for this request.
		RequestStepAlreadyRecorded,
		/// `step` must be one of the five recordable values — never
		/// `RequestStep::None`/`Completed`, both read-only sentinels.
		InvalidRequestStep,
		/// `step` must be one of the seven recordable values — never
		/// `SettlementStep::Queued`/`Settled`, both read-only sentinels.
		InvalidSettlementStep,
		/// `collect_response_chain_ids` and `finalize_chain_ids` must both be `Some`
		/// when `step == SettlementStep::Triggered` (each may independently be
		/// empty — a chain missing from both sets needs no leg at all for this
		/// settlement; both empty means the settlement needs no cross-chain action
		/// at all).
		SpokeChainIdsRequired,
		/// `collect_response_chain_ids` and `finalize_chain_ids` must both be `None`
		/// for every step other than `Triggered`.
		UnexpectedSpokeChainIds,
		/// `spoke_chain_id` must be `Some` for every leg step (every step other than
		/// `Triggered`).
		SpokeChainIdRequired,
		/// `spoke_chain_id` must be `None` when `step == Triggered`.
		UnexpectedSpokeChainId,
		/// Trigger has already been recorded for this (product_id, settlement_id).
		SettlementAlreadyTriggered,
		/// Trigger has not been recorded yet for this (product_id, settlement_id).
		SettlementNotTriggered,
		/// `spoke_chain_id` is not among the chains registered for the leg kind
		/// being recorded — `collect_response_chain_ids` for a Collect/Response leg,
		/// `finalize_chain_ids` for a Finalize leg.
		UnknownSpokeChain,
		/// The leg step being recorded skips over its Bridge phase.
		SettlementStepOutOfOrder,
		/// This leg step has already been recorded for this chain.
		SettlementStepAlreadyRecorded,
		/// A receive has already been recorded for this (investor, vault, tx_hash).
		ReceiveAlreadyRecorded,
		/// `tx_hash` must not be the zero hash — a zero `tx_hash` can never be a
		/// genuine attested transaction, and `TxRecord::recorded_at == 0` (not
		/// `tx_hash`) is already this pallet's own "not yet recorded" sentinel
		/// everywhere it's read (see e.g. get_request/get_settlement's read-side
		/// docs), so a zero `tx_hash` slipping into storage would be indistinguishable
		/// from a genuine attestation to any caller inspecting `tx_hash` alone.
		TxHashRequired,
		/// `step == WhitelistStep::WhitelistRequested` was recorded for a
		/// `(who, vault, nonce)` that already has an entry.
		WhitelistAlreadyTriggered,
		/// No entry exists yet for this `(who, vault, nonce)` — `record_whitelist_tx`
		/// must be called with `step == WhitelistStep::WhitelistRequested` first.
		WhitelistNotTriggered,
		/// `grant` doesn't match the value this entry was opened with — every step
		/// after `WhitelistRequested` must resupply the same `grant` it was
		/// triggered with (see `WhitelistEntry::grant`'s doc comment for why this
		/// is checked rather than just trusted).
		UnexpectedWhitelistGrant,
		/// `step == WhitelistStep::BridgeExecuted` was recorded for a whitelist
		/// action whose vault is on Hub — a Hub-vault action has no Bridge leg at
		/// all (there's nothing to bridge when TrancheManager already applies the
		/// grant/revoke locally on Hub).
		UnexpectedWhitelistBridgeLeg,
		/// The step being recorded skips over an earlier, not-yet-recorded step.
		WhitelistStepOutOfOrder,
		/// This step has already been recorded for this `(who, vault, nonce)`.
		WhitelistStepAlreadyRecorded,
		/// `step` must be one of the three recordable values — never
		/// `WhitelistStep::None`, a read-only sentinel.
		InvalidWhitelistStep,
	}

	// -----------------------------------------------------------------------
	// Events
	// -----------------------------------------------------------------------

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// The tx recorder account was (re)configured via `set_tx_recorder`.
		TxRecorderSet { old: Option<T::AccountId>, new: T::AccountId },
		/// One tx in a request's pipeline was recorded.
		RequestTxRecorded {
			product_id: ProductId,
			request_id: RequestId,
			opening: Option<RequestOpening>,
			adapter_chain_ids: Option<BoundedVec<ChainId, ConstU32<MAX_SPOKE_CHAINS>>>,
			step: RequestStep,
			chain_id: ChainId,
			tx_hash: H256,
		},
		/// One tx in a settlement's pipeline was recorded.
		SettlementTxRecorded {
			product_id: ProductId,
			settlement_id: SettlementId,
			spoke_chain_id: Option<ChainId>,
			collect_response_chain_ids: Option<BoundedVec<ChainId, ConstU32<MAX_SPOKE_CHAINS>>>,
			finalize_chain_ids: Option<BoundedVec<ChainId, ConstU32<MAX_SPOKE_CHAINS>>>,
			step: SettlementStep,
			chain_id: ChainId,
			tx_hash: H256,
		},
		/// A receive() tx was recorded.
		ReceiveTxRecorded {
			product_id: ProductId,
			vault: VaultId,
			investor: H160,
			receiver: H160,
			amount: U256,
			kind: ReceiveKind,
			chain_id: ChainId,
			tx_hash: H256,
		},
		/// `(product_id, request_id)` was automatically removed from `investor`'s
		/// `InvestorActiveRequests` list — a side effect of `record_settlement_tx`
		/// recording that request's own origin chain reaching
		/// `SettlementStep::SettleApplied` for the settlement it was
		/// approved into.
		ActiveRequestClosed { product_id: ProductId, request_id: RequestId, investor: H160 },
		/// One tx in a whitelist grant/revoke action's pipeline was recorded.
		WhitelistTxRecorded {
			product_id: ProductId,
			vault: VaultId,
			who: H160,
			grant: bool,
			nonce: WhitelistNonce,
			step: WhitelistStep,
			chain_id: ChainId,
			tx_hash: H256,
		},
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
	/// A request's registry entry. Keyed by `(product_id, request_id)`, NOT
	/// `request_id` alone — `request_id` is only unique within a product's
	/// own namespace (each product's Valuation Contract generates its own
	/// sequence), same rationale as
	/// `pallet_tranche_investments::RequestedInvestments`. Opened by
	/// `record_request_tx`'s `RequestStep::Requested` step; `queued_tx` is
	/// filled in afterward by `RequestQueued` (every request, Hub or Spoke
	/// alike), and `bridge_tx` by `RequestBridgeExecuted` (Spoke-vault only —
	/// see `RequestEntry`'s doc comment). Adapter leg evidence lives in
	/// `RequestChainEntries` instead, not here.
	pub type RequestEntries<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		ProductId,
		Blake2_128Concat,
		RequestId,
		RequestEntry<BlockNumberFor<T>>,
	>;

	#[pallet::storage]
	/// The chains a request's capital gets distributed out to, in the order
	/// they were declared — every one of them will need its own
	/// `AdapterBridgeExecuted`/`AdapterApplied` leg in `RequestChainEntries`
	/// before the request is `Completed`. Empty means no Adapter is needed at
	/// all. Can include Hub itself, or the origin vault's own chain, when
	/// either self-fulfills a weighted allocation synchronously (no Bridge leg
	/// — see `RequestStep`'s doc comment).
	///
	/// Populated two ways, which can interleave in either order:
	/// `record_request_tx` at `step == RequestQueued` writes whatever
	/// `adapter_chain_ids` the Valuation Contract explicitly declared at that
	/// point (merged, not overwritten, into whatever's already here — see
	/// below), while `AdapterBridgeExecuted`/`AdapterApplied` self-declare a
	/// not-yet-seen `chain_id` on first touch (see
	/// `Pallet::ensure_adapter_chain_declared`) — needed because a
	/// self-fulfilling chain's `AdapterApplied` evidence can arrive before
	/// `RequestQueued` ever runs, and this pallet accepts `record_request_tx`
	/// calls in whatever order the recorder actually observed the underlying
	/// events, not a pipeline-assumed order.
	///
	/// Kept as a separate storage item rather than folded into `RequestEntry`,
	/// same pattern as `SettlementCollectResponseChains`/
	/// `SettlementFinalizeChains` alongside `SettlementTriggers`.
	pub type RequestAdapterChains<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		ProductId,
		Blake2_128Concat,
		RequestId,
		BoundedVec<ChainId, ConstU32<MAX_SPOKE_CHAINS>>,
	>;

	#[pallet::storage]
	/// One chain's Adapter leg (Bridge+Hooks) registry entry within a
	/// request. Keyed by `(product_id, request_id, chain_id)`. `ValueQuery`
	/// with `RequestChainEntry`'s `Default` impl (rather than `OptionQuery`),
	/// same rationale as `SettlementChainEntries` — every field inside is
	/// already independently `Option`-typed, so a fully-empty default entry
	/// and "nothing recorded for this chain yet" are the same state.
	pub type RequestChainEntries<T: Config> = StorageNMap<
		_,
		(
			NMapKey<Blake2_128Concat, ProductId>,
			NMapKey<Blake2_128Concat, RequestId>,
			NMapKey<Blake2_128Concat, ChainId>,
		),
		RequestChainEntry<BlockNumberFor<T>>,
		ValueQuery,
	>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// An investor's currently in-flight requests — registered here the moment
	/// their `RequestEntries` entry is opened (`RequestStep::Requested`), removed
	/// automatically by `record_settlement_tx` for a Spoke-vault request's own
	/// origin chain when it records that chain's `SettleApplied` leg (via
	/// `close_active_requests`), or for every Hub-vault request approved into the
	/// settlement, all at once, the moment `SettlementCollectResponseChains` has
	/// been fully responded to (every chain has reached `NavReceived` —
	/// vacuously true, and checked immediately, if that set was declared empty
	/// at Trigger time — see `try_close_hub_vault_requests`): at that point it
	/// asks `T::Investments::settlement_requests(product_id, settlement_id)`
	/// (implemented by pallet-tranche-investments, see
	/// `RequestSettlementInspect`'s doc comment for why this doesn't require a
	/// hard dependency on that pallet) for every request_id approved into that
	/// settlement, and removes the ones whose own origin chain
	/// (`RequestEntry::vault::chain_id`) matches the leg just closed. Bounded by
	/// `pallet_tranche_investments::MAX_SETTLEMENT_REQUESTS` on the writing side, so
	/// this stays a fixed-cost operation rather than an unbounded scan.
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
	#[pallet::unbounded]
	/// Every `request_id` an investor has ever opened for a given product, in the
	/// order opened — written once, at `RequestStep::Requested`, alongside
	/// `InvestorActiveRequests`, but (unlike that storage) **never removed from**.
	/// A request's presence here says nothing about whether it's still in
	/// flight — cross-reference against `InvestorActiveRequests` (or
	/// `get_request`'s own `status`/`settled`) for that; this storage exists
	/// purely so a completed request's `request_id` isn't lost once it drops out
	/// of `InvestorActiveRequests`, giving `get_investor_request_history` (see
	/// the precompile) something to page through for a "past requests" screen.
	///
	/// Deliberately unbounded and never pruned, same `#[pallet::unbounded]`
	/// rationale as `InvestorActiveRequests` — growth is bounded in practice by
	/// how many real Requested calls a genuine investor generates over a
	/// product's lifetime (each one traces back to a real on-chain Vault
	/// request, itself gas-costed on its own origin chain), not by anything
	/// this pallet caps directly. Keyed by `(H160, ProductId)` rather than
	/// folding `product_id` into the value alongside every other investor's
	/// products (unlike `InvestorActiveRequests`'s cross-product `Vec`) so a
	/// single-product history read never has to decode entries for products
	/// the caller doesn't care about.
	pub type InvestorRequestHistory<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		H160,
		Blake2_128Concat,
		ProductId,
		Vec<RequestId>,
		ValueQuery,
	>;

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
	/// The chains registered for a settlement's Collect/Response legs at Trigger
	/// time (those with a registered Adapter, excluding Hub itself — an Adapter
	/// on Hub is queried locally, no Bridge&Call leg needed), in the order the
	/// recorder supplied them. A chain never reaches `NavReceived`
	/// unless it's in this set. Always written alongside `SettlementTriggers`
	/// and `SettlementFinalizeChains` (all by the same `record_settlement_tx`
	/// call for `step == Triggered`) — kept as separate storage items rather
	/// than folded into one struct, same pattern already used by
	/// `pallet_tranche_investments`' `AdapterValuations`/`ProductNavs`/
	/// `Settlements` (three separate maps written together by one
	/// extrinsic).
	pub type SettlementCollectResponseChains<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		ProductId,
		Blake2_128Concat,
		SettlementId,
		BoundedVec<ChainId, ConstU32<MAX_SPOKE_CHAINS>>,
	>;

	#[pallet::storage]
	/// The chains registered for a settlement's Finalize leg at Trigger time
	/// (those with a registered tranche vault, excluding Hub itself — a Hub
	/// vault's result is delivered locally, no Bridge&Call leg needed), in the
	/// order the recorder supplied them. A chain never reaches
	/// `SettleApplied` unless it's in this set — this is also the set
	/// `get_settlement`/`get_request` wait on for their own
	/// `status`/`settled` completion, since a chain in
	/// `SettlementCollectResponseChains` but not here has no vault to deliver a
	/// result to (it terminates at `NavReceived` instead — see
	/// `SettlementStep`'s doc comment). Written alongside `SettlementTriggers`
	/// and `SettlementCollectResponseChains`, same rationale as that storage's
	/// doc comment. A chain may appear in both sets (Type 3-style: it has both a
	/// vault and an Adapter).
	pub type SettlementFinalizeChains<T: Config> = StorageDoubleMap<
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
	/// Every receive() tx recorded, one storage slot per receive. Keyed by
	/// `(investor, vault, tx_hash)` — `investor` is the controller (see
	/// `ReceiveEntry`'s own doc comment). `vault` stays in the key alongside
	/// `tx_hash` because `tx_hash` alone is only unique within its own chain,
	/// not globally — `vault`'s `chain_id` is what rules out two different
	/// chains coincidentally producing the same hash. `product_id` is
	/// deliberately not part of the key: `VaultId` is already globally unique
	/// (enforced by pallet-tranche-system), so it would be redundant for
	/// addressing purposes. TrancheManager pools receivable amounts per
	/// (investor, vault), not per request_id, so there is no single
	/// request_id a receive could be keyed by instead.
	///
	/// Enumerate one investor's receive history for one vault via
	/// `iter_prefix((investor, vault))`, or use `InvestorReceiveHistory` to
	/// page through it scoped to one product instead.
	pub type ReceiveEntries<T: Config> = StorageNMap<
		_,
		(
			NMapKey<Blake2_128Concat, H160>,
			NMapKey<Blake2_128Concat, VaultId>,
			NMapKey<Blake2_128Concat, H256>,
		),
		ReceiveEntry<BlockNumberFor<T>>,
	>;

	#[pallet::storage]
	#[pallet::unbounded]
	/// Every `(vault, tx_hash)` an investor has ever had recorded via
	/// `record_receive_tx` for a given product, in the order recorded —
	/// append-only, never pruned. Written alongside `ReceiveEntries` by
	/// `record_receive_tx`. Mirrors `InvestorRequestHistory`'s shape/rationale
	/// exactly (see that storage's own doc comment) — the parallel structure is
	/// deliberate: a receive isn't linked to a specific `request_id`
	/// (TrancheManager pools receivable amounts per (investor, vault) rather
	/// than per request), so this is the receive-side equivalent of "give me
	/// this investor's history for this product" that `InvestorRequestHistory`
	/// provides for requests. `vault` has to travel alongside `tx_hash` here
	/// (unlike `InvestorRequestHistory`'s bare `Vec<RequestId>`) since
	/// `ReceiveEntries`' own key needs `vault` too — see that storage's doc
	/// comment for why `tx_hash` alone isn't a safe enough identifier.
	pub type InvestorReceiveHistory<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		H160,
		Blake2_128Concat,
		ProductId,
		Vec<(VaultId, H256)>,
		ValueQuery,
	>;

	#[pallet::storage]
	/// A whitelist grant/revoke action's registry entry. Keyed by `(who, vault,
	/// nonce)`, NOT `product_id` — `VaultId` is already globally unique (enforced
	/// by pallet-tranche-system), same rationale as `ReceiveEntries`. Opened by
	/// `record_whitelist_tx`'s `WhitelistStep::WhitelistRequested` step; presence
	/// of an entry here (rather than a `WhitelistStep::None`-tagged value) is what
	/// answers "has this action been triggered yet" — mirrors how `RequestEntries`
	/// uses entry-presence rather than an explicit sentinel step.
	pub type WhitelistEntries<T: Config> = StorageNMap<
		_,
		(
			NMapKey<Blake2_128Concat, H160>,
			NMapKey<Blake2_128Concat, VaultId>,
			NMapKey<Blake2_128Concat, WhitelistNonce>,
		),
		WhitelistEntry<BlockNumberFor<T>>,
	>;

	#[pallet::storage]
	/// The most recent whitelist action's `nonce` for a given `(who, vault)` —
	/// updated (not appended) at `WhitelistStep::WhitelistRequested`, but only
	/// when the newly submitted `nonce` is strictly greater than whatever's
	/// already stored here; a lower/equal nonce leaves this untouched. That
	/// guard matters because "which `WhitelistRequested` call the recorder
	/// happens to submit last" isn't guaranteed to match on-chain nonce order
	/// (retries, catching up on missed blocks out of sequence, etc.) — without
	/// it, a late-arriving call for an older action could clobber this back
	/// to a stale value. Deliberately no history array: nonces are
	/// permanently readable via `WhitelistEntries` for anyone who already has
	/// one (e.g. from watching `WhitelistTxRecorded`), but only the latest is
	/// discoverable on-chain without already knowing it — full history
	/// browsing is an indexer's job, not something this pallet carries itself.
	pub type LatestWhitelistNonce<T: Config> =
		StorageDoubleMap<_, Blake2_128Concat, H160, Blake2_128Concat, VaultId, WhitelistNonce>;

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

		/// Attest to one tx in a request's pipeline — the single Requested tx, the
		/// single RequestQueued tx, one Bridge half of the Inbound leg (Spoke-vault
		/// requests only), or one Bridge/Applied half of a per-chain Adapter leg (see
		/// `RequestStep`'s doc comment for exactly which chains need one). Origin must
		/// be `RecorderOrigin`. `opening` MUST be `Some` iff `step ==
		/// RequestStep::Requested`, `None` otherwise. `adapter_chain_ids` MUST be
		/// `Some` iff `step == RequestStep::RequestQueued` (Hub-vault or Spoke-vault
		/// alike), `None` otherwise — see interface.sol's `record_request_tx` for the
		/// full sentinel-gating/ordering contract this mirrors.
		///
		/// Steps do NOT need to be recorded in the pipeline's own conceptual order —
		/// only in the order the recorder actually observed the underlying events
		/// on-chain. In particular, `AdapterBridgeExecuted`/`AdapterApplied` for a
		/// chain that self-fulfills (the origin vault's own chain, or Hub) can arrive
		/// before `Requested`/`RequestQueued` ever run, since that chain's Adapter
		/// Contract call happens locally, in the same tx as (and possibly logged
		/// before) the domain event that would otherwise open/advance this request —
		/// see `RequestStep`'s doc comment and `Pallet::ensure_adapter_chain_declared`.
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::record_request_tx())]
		pub fn record_request_tx(
			origin: OriginFor<T>,
			product_id: ProductId,
			request_id: RequestId,
			opening: Option<RequestOpening>,
			adapter_chain_ids: Option<BoundedVec<ChainId, ConstU32<MAX_SPOKE_CHAINS>>>,
			step: RequestStep,
			chain_id: ChainId,
			tx_hash: H256,
		) -> DispatchResult {
			T::RecorderOrigin::ensure_origin(origin)?;
			ensure!(!tx_hash.is_zero(), Error::<T>::TxHashRequired);

			let recorded_at = frame_system::Pallet::<T>::block_number();
			let tx = TxRecord { chain_id, tx_hash, recorded_at };
			let hub_chain_id = <T as pallet_evm::Config>::ChainId::get();

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
					// `adapter_chain_ids` is never known yet at `Requested`, Hub-vault or
					// Spoke-vault alike — that's `RequestQueued`'s job, one step later.
					ensure!(
						adapter_chain_ids.is_none(),
						Error::<T>::UnexpectedRequestAdapterChains
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
							queued_tx: None,
						},
					);
					InvestorActiveRequests::<T>::mutate(opening.investor, |requests| {
						requests.push((product_id, request_id));
					});
					InvestorRequestHistory::<T>::mutate(opening.investor, product_id, |history| {
						history.push(request_id);
					});
				},
				RequestStep::RequestBridgeExecuted => {
					ensure!(opening.is_none(), Error::<T>::UnexpectedRequestOpening);
					ensure!(
						adapter_chain_ids.is_none(),
						Error::<T>::UnexpectedRequestAdapterChains
					);
					let mut entry = RequestEntries::<T>::get(product_id, request_id)
						.ok_or(Error::<T>::RequestNotOpened)?;
					ensure!(entry.vault.chain_id != hub_chain_id, Error::<T>::UnexpectedInboundLeg);
					ensure!(entry.bridge_tx.is_none(), Error::<T>::RequestStepAlreadyRecorded);
					entry.bridge_tx = Some(tx);
					RequestEntries::<T>::insert(product_id, request_id, entry);
				},
				RequestStep::RequestQueued => {
					ensure!(opening.is_none(), Error::<T>::UnexpectedRequestOpening);
					let mut entry = RequestEntries::<T>::get(product_id, request_id)
						.ok_or(Error::<T>::RequestNotOpened)?;
					if entry.vault.chain_id != hub_chain_id {
						// Spoke-vault — only reachable once the Inbound leg's own Bridge
						// phase has landed.
						ensure!(entry.bridge_tx.is_some(), Error::<T>::RequestStepOutOfOrder);
					}
					// Hub-vault has no Inbound leg to wait on — RequestQueued can follow
					// Requested immediately (typically the same tx, always a separate call).
					ensure!(entry.queued_tx.is_none(), Error::<T>::RequestStepAlreadyRecorded);
					// The request's arrival at the Valuation Contract — this is where the
					// Adapter decision first becomes knowable, Hub-vault or Spoke-vault alike.
					let chains = adapter_chain_ids
						.clone()
						.ok_or(Error::<T>::RequestAdapterChainsRequired)?;
					ensure!(
						T::Adapters::adapter_chains_belong_to_product(product_id, &chains),
						Error::<T>::SpokeChainNotRegistered
					);
					// Merge, not overwrite: `AdapterBridgeExecuted`/`AdapterApplied` may
					// already have self-declared a chain here (the origin vault's own
					// chain, or Hub, self-fulfilling with no Bridge leg at all — see
					// `Pallet::ensure_adapter_chain_declared`) before this step ever ran,
					// since the recorder may observe events out of the order this
					// pipeline model would otherwise assume. Overwriting would silently
					// drop that already-recorded evidence's declaration.
					let mut merged =
						RequestAdapterChains::<T>::get(product_id, request_id).unwrap_or_default();
					for declared_chain_id in chains.iter() {
						if !merged.contains(declared_chain_id) {
							merged
								.try_push(*declared_chain_id)
								.map_err(|_| Error::<T>::TooManyAdapterChains)?;
						}
					}
					RequestAdapterChains::<T>::insert(product_id, request_id, merged);
					entry.queued_tx = Some(tx);
					RequestEntries::<T>::insert(product_id, request_id, entry);
				},
				RequestStep::AdapterBridgeExecuted => {
					ensure!(opening.is_none(), Error::<T>::UnexpectedRequestOpening);
					ensure!(
						adapter_chain_ids.is_none(),
						Error::<T>::UnexpectedRequestAdapterChains
					);
					Self::ensure_adapter_chain_declared(product_id, request_id, chain_id)?;
					let mut entry =
						RequestChainEntries::<T>::get((product_id, request_id, chain_id));
					ensure!(entry.bridge_tx.is_none(), Error::<T>::RequestStepAlreadyRecorded);
					entry.bridge_tx = Some(tx);
					RequestChainEntries::<T>::insert((product_id, request_id, chain_id), entry);
				},
				RequestStep::AdapterApplied => {
					ensure!(opening.is_none(), Error::<T>::UnexpectedRequestOpening);
					ensure!(
						adapter_chain_ids.is_none(),
						Error::<T>::UnexpectedRequestAdapterChains
					);
					Self::ensure_adapter_chain_declared(product_id, request_id, chain_id)?;
					let mut entry =
						RequestChainEntries::<T>::get((product_id, request_id, chain_id));
					// No `entry.bridge_tx.is_some()` precondition here (unlike every other
					// Bridge-then-Applied/Hooks pair in this pallet) — a chain that
					// self-fulfills locally (origin vault's own chain, or Hub) never gets
					// a Bridge leg at all, so `AdapterApplied` must be recordable on its
					// own. See `Pallet::ensure_adapter_chain_declared`'s doc comment.
					ensure!(entry.applied_tx.is_none(), Error::<T>::RequestStepAlreadyRecorded);
					entry.applied_tx = Some(tx);
					RequestChainEntries::<T>::insert((product_id, request_id, chain_id), entry);
				},
				RequestStep::None | RequestStep::Completed => {
					return Err(Error::<T>::InvalidRequestStep.into());
				},
			}

			Self::deposit_event(Event::RequestTxRecorded {
				product_id,
				request_id,
				opening,
				adapter_chain_ids,
				step,
				chain_id,
				tx_hash,
			});
			Ok(())
		}

		/// Attest to one tx in a settlement's pipeline: either the single Trigger
		/// tx, or one bridge/hooks half of a Collect/Response/Finalize leg for one
		/// chain. Origin must be `RecorderOrigin`. `collect_response_chain_ids` and
		/// `finalize_chain_ids` MUST both be `Some` iff `step ==
		/// SettlementStep::Triggered` (each may independently be empty — see below);
		/// `spoke_chain_id` MUST be `Some` for every leg step (every step other than
		/// `Triggered`) — see interface.sol's `record_settlement_tx` for the full
		/// contract this mirrors.
		///
		/// The two sets declare, per chain, which leg kind(s) it needs — Collect/
		/// Response for a chain with a registered Adapter, Finalize for a chain with
		/// a registered vault, both for a chain with both (excluding Hub itself in
		/// either case — see `SettlementCollectResponseChains`/`SettlementFinalizeChains`'s
		/// doc comments). A chain absent from `finalize_chain_ids` never blocks
		/// completion on a Finalize leg it was never going to get — completion waits
		/// on `NavReceived` for it instead (see `SettlementStep`'s doc
		/// comment). A settlement needing no cross-chain action at all is recorded as
		/// `Triggered` with both sets empty — every read path already reports this
		/// correctly via vacuous truth, with no dedicated step needed for it.
		///
		/// Side effects on `InvestorActiveRequests` (see its own storage doc comment
		/// for the full mechanism — `T::Investments::settlement_requests` +
		/// `ActiveRequestClosed`): `step == SettlementStep::SettleApplied`
		/// closes every Spoke-vault request approved into this settlement whose own
		/// origin chain is `spoke_chain_id`. `step == SettlementStep::Triggered` and
		/// `step == SettlementStep::NavReceived` both additionally try to
		/// close every *Hub*-vault request approved into this settlement, via
		/// `try_close_hub_vault_requests` — a Hub-vault request has no Finalize leg
		/// of its own to trigger on (see `SettlementStep`'s doc comment), so this
		/// runs instead every time `collect_response_chain_ids` might have just
		/// become fully responded (including vacuously, right at Trigger, if it was
		/// declared empty).
		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::record_settlement_tx())]
		pub fn record_settlement_tx(
			origin: OriginFor<T>,
			product_id: ProductId,
			settlement_id: SettlementId,
			spoke_chain_id: Option<ChainId>,
			collect_response_chain_ids: Option<BoundedVec<ChainId, ConstU32<MAX_SPOKE_CHAINS>>>,
			finalize_chain_ids: Option<BoundedVec<ChainId, ConstU32<MAX_SPOKE_CHAINS>>>,
			step: SettlementStep,
			chain_id: ChainId,
			tx_hash: H256,
		) -> DispatchResult {
			T::RecorderOrigin::ensure_origin(origin)?;
			ensure!(!tx_hash.is_zero(), Error::<T>::TxHashRequired);

			let recorded_at = frame_system::Pallet::<T>::block_number();
			let tx = TxRecord { chain_id, tx_hash, recorded_at };

			if step == SettlementStep::Triggered {
				ensure!(spoke_chain_id.is_none(), Error::<T>::UnexpectedSpokeChainId);
				let collect_response_chains =
					collect_response_chain_ids.clone().ok_or(Error::<T>::SpokeChainIdsRequired)?;
				let finalize_chains =
					finalize_chain_ids.clone().ok_or(Error::<T>::SpokeChainIdsRequired)?;
				ensure!(
					T::Adapters::adapter_chains_belong_to_product(
						product_id,
						&collect_response_chains
					),
					Error::<T>::SpokeChainNotRegistered
				);
				ensure!(
					T::Vaults::vault_chains_belong_to_product(product_id, &finalize_chains),
					Error::<T>::SpokeChainNotRegistered
				);
				ensure!(
					!SettlementTriggers::<T>::contains_key(product_id, settlement_id),
					Error::<T>::SettlementAlreadyTriggered
				);
				SettlementTriggers::<T>::insert(product_id, settlement_id, tx);
				SettlementCollectResponseChains::<T>::insert(
					product_id,
					settlement_id,
					collect_response_chains.clone(),
				);
				SettlementFinalizeChains::<T>::insert(product_id, settlement_id, finalize_chains);

				// Closes every Hub-vault request approved into this settlement if
				// `collect_response_chains` is already fully responded — vacuously true
				// right away when it's empty (no Adapter anywhere off-Hub, or a fully local
				// settlement), same as a leg-by-leg `NavReceived` reaching this
				// state later would.
				Self::try_close_hub_vault_requests(product_id, settlement_id);
			} else {
				ensure!(
					collect_response_chain_ids.is_none() && finalize_chain_ids.is_none(),
					Error::<T>::UnexpectedSpokeChainIds
				);
				let spoke_chain_id = spoke_chain_id.ok_or(Error::<T>::SpokeChainIdRequired)?;
				let is_finalize_step = matches!(
					step,
					SettlementStep::FinalizeBridgeExecuted | SettlementStep::SettleApplied
				);
				let chains = if is_finalize_step {
					SettlementFinalizeChains::<T>::get(product_id, settlement_id)
				} else {
					SettlementCollectResponseChains::<T>::get(product_id, settlement_id)
				}
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
					SettlementStep::NavReported => {
						ensure!(
							entry.collect_bridge_tx.is_some(),
							Error::<T>::SettlementStepOutOfOrder
						);
						ensure!(
							entry.nav_reported_tx.is_none(),
							Error::<T>::SettlementStepAlreadyRecorded
						);
						entry.nav_reported_tx = Some(tx);
					},
					SettlementStep::ResponseBridgeExecuted => {
						ensure!(
							entry.response_bridge_tx.is_none(),
							Error::<T>::SettlementStepAlreadyRecorded
						);
						entry.response_bridge_tx = Some(tx);
					},
					SettlementStep::NavReceived => {
						ensure!(
							entry.response_bridge_tx.is_some(),
							Error::<T>::SettlementStepOutOfOrder
						);
						ensure!(
							entry.nav_received_tx.is_none(),
							Error::<T>::SettlementStepAlreadyRecorded
						);
						entry.nav_received_tx = Some(tx);
					},
					SettlementStep::FinalizeBridgeExecuted => {
						ensure!(
							entry.finalize_bridge_tx.is_none(),
							Error::<T>::SettlementStepAlreadyRecorded
						);
						entry.finalize_bridge_tx = Some(tx);
					},
					SettlementStep::SettleApplied => {
						ensure!(
							entry.finalize_bridge_tx.is_some(),
							Error::<T>::SettlementStepOutOfOrder
						);
						ensure!(
							entry.settle_applied_tx.is_none(),
							Error::<T>::SettlementStepAlreadyRecorded
						);
						entry.settle_applied_tx = Some(tx);
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

				if step == SettlementStep::SettleApplied {
					Self::close_active_requests(product_id, settlement_id, Some(spoke_chain_id));
				} else if step == SettlementStep::NavReceived {
					Self::try_close_hub_vault_requests(product_id, settlement_id);
				}
			}

			Self::deposit_event(Event::SettlementTxRecorded {
				product_id,
				settlement_id,
				spoke_chain_id,
				collect_response_chain_ids,
				finalize_chain_ids,
				step,
				chain_id,
				tx_hash,
			});
			Ok(())
		}

		/// Attest to an investor's receive() tx on a vault — a plain local Spoke-chain
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
			receiver: H160,
			amount: U256,
			kind: ReceiveKind,
			chain_id: ChainId,
			tx_hash: H256,
		) -> DispatchResult {
			T::RecorderOrigin::ensure_origin(origin)?;
			ensure!(!tx_hash.is_zero(), Error::<T>::TxHashRequired);

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
				ReceiveEntry { investor, vault: vault.clone(), receiver, amount, tx, kind },
			);
			InvestorReceiveHistory::<T>::mutate(investor, product_id, |history| {
				history.push((vault.clone(), tx_hash));
			});

			Self::deposit_event(Event::ReceiveTxRecorded {
				product_id,
				vault,
				investor,
				receiver,
				amount,
				kind,
				chain_id,
				tx_hash,
			});
			Ok(())
		}

		/// Attest to one tx in a whitelist grant/revoke action's pipeline — the
		/// Trigger tx (Orchestrator's `WhitelistRequested`), the Bridge phase, or
		/// the Applied tx (TrancheManager's `WhitelistApplied`). Origin must be
		/// `RecorderOrigin`. See interface.sol's `record_whitelist_tx` for the
		/// full contract this mirrors.
		///
		/// `grant` must be resupplied at every step (Solidity has no
		/// `Option<bool>` to sentinel-gate it the way `RequestOpening`-style
		/// fields are gated) and is checked against the value the entry was
		/// opened with at `WhitelistRequested` — reverts on mismatch.
		/// `product_id` is not a parameter — resolved internally via
		/// `T::Vaults::product_id_for_vault(&vault)` at `WhitelistRequested`
		/// time, since none of this pipeline's chain-observed evidence carries
		/// it directly the way `DepositRequested`/`DepositReceived` do.
		///
		/// `step == BridgeExecuted` reverts (`Error::UnexpectedWhitelistBridgeLeg`)
		/// if `vault` is on Hub — see `WhitelistStep`'s doc comment for why a
		/// Hub-vault action has no Bridge leg at all.
		#[pallet::call_index(4)]
		#[pallet::weight(<T as Config>::WeightInfo::record_whitelist_tx())]
		pub fn record_whitelist_tx(
			origin: OriginFor<T>,
			vault: VaultId,
			who: H160,
			grant: bool,
			nonce: WhitelistNonce,
			step: WhitelistStep,
			chain_id: ChainId,
			tx_hash: H256,
		) -> DispatchResult {
			T::RecorderOrigin::ensure_origin(origin)?;
			ensure!(!tx_hash.is_zero(), Error::<T>::TxHashRequired);
			let hub_chain_id = <T as pallet_evm::Config>::ChainId::get();

			let recorded_at = frame_system::Pallet::<T>::block_number();
			let tx = TxRecord { chain_id, tx_hash, recorded_at };
			let key = (who, vault.clone(), nonce);

			let product_id = match step {
				WhitelistStep::WhitelistRequested => {
					ensure!(
						!WhitelistEntries::<T>::contains_key(key.clone()),
						Error::<T>::WhitelistAlreadyTriggered
					);
					let product_id = T::Vaults::product_id_for_vault(&vault)
						.ok_or(Error::<T>::VaultNotRegistered)?;
					WhitelistEntries::<T>::insert(
						key,
						WhitelistEntry {
							product_id,
							vault: vault.clone(),
							who,
							grant,
							request_tx: Some(tx),
							bridge_tx: None,
							applied_tx: None,
						},
					);
					let is_newer = match LatestWhitelistNonce::<T>::get(who, vault.clone()) {
						Some(latest) => nonce > latest,
						None => true,
					};
					if is_newer {
						LatestWhitelistNonce::<T>::insert(who, vault.clone(), nonce);
					}
					product_id
				},
				WhitelistStep::BridgeExecuted => {
					let mut entry = WhitelistEntries::<T>::get(key.clone())
						.ok_or(Error::<T>::WhitelistNotTriggered)?;
					ensure!(entry.grant == grant, Error::<T>::UnexpectedWhitelistGrant);
					ensure!(
						entry.vault.chain_id != hub_chain_id,
						Error::<T>::UnexpectedWhitelistBridgeLeg
					);
					ensure!(entry.bridge_tx.is_none(), Error::<T>::WhitelistStepAlreadyRecorded);
					entry.bridge_tx = Some(tx);
					let product_id = entry.product_id;
					WhitelistEntries::<T>::insert(key, entry);
					product_id
				},
				WhitelistStep::WhitelistApplied => {
					let mut entry = WhitelistEntries::<T>::get(key.clone())
						.ok_or(Error::<T>::WhitelistNotTriggered)?;
					ensure!(entry.grant == grant, Error::<T>::UnexpectedWhitelistGrant);
					if entry.vault.chain_id != hub_chain_id {
						ensure!(entry.bridge_tx.is_some(), Error::<T>::WhitelistStepOutOfOrder);
					}
					ensure!(entry.applied_tx.is_none(), Error::<T>::WhitelistStepAlreadyRecorded);
					entry.applied_tx = Some(tx);
					let product_id = entry.product_id;
					WhitelistEntries::<T>::insert(key, entry);
					product_id
				},
				WhitelistStep::None => {
					return Err(Error::<T>::InvalidWhitelistStep.into());
				},
			};

			Self::deposit_event(Event::WhitelistTxRecorded {
				product_id,
				vault,
				who,
				grant,
				nonce,
				step,
				chain_id,
				tx_hash,
			});
			Ok(())
		}
	}
}
