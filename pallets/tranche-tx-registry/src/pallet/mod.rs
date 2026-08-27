mod impls;

use crate::{
	migrations, BridgeStatus, ChainId, ProductId, ReceiveEntry, ReceiveKind, RequestChainEntry,
	RequestEntry, RequestId, RequestOpening, RequestStep, SettlementChainEntry,
	SettlementFlowExtension, SettlementId, SettlementStep, TxRecord, WeightInfo, WhitelistEntry,
	WhitelistNonce, WhitelistStep, MAX_REQUEST_EXTRA_LEN, MAX_SETTLEMENT_EXTRA_LEN,
	MAX_SETTLEMENT_REQUESTS,
};
use pallet_tranche_system::{
	AdapterInspect, ProductInspect, VaultId, VaultInspect, MAX_MULTICHAIN_ADAPTERS,
	MAX_TRANCHE_CHAINS,
};

use frame_support::{
	pallet_prelude::*,
	traits::{OnRuntimeUpgrade, StorageVersion},
};
use frame_system::pallet_prelude::*;
use sp_core::{ConstU32, H160, H256, U256};
use sp_std::vec::Vec;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(3);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_runtime_upgrade() -> Weight {
			// Chained rather than just `MigrateToV3` alone: each `VersionedMigration`
			// self-gates on its own exact on-chain version, so this is safe regardless of
			// whether a given chain is still at v0 (runs all three, back to back, in the
			// same upgrade) or already at v2 (skips straight to v3 — the live testbed
			// case) — same pattern as
			// `pallet_tranche_system::pallet::Hooks::on_runtime_upgrade`.
			migrations::v1::MigrateToV1::<T>::on_runtime_upgrade()
				.saturating_add(migrations::v2::MigrateToV2::<T>::on_runtime_upgrade())
				.saturating_add(migrations::v3::MigrateToV3::<T>::on_runtime_upgrade())
		}
	}

	#[pallet::config]
	/// `pallet_evm::Config` supplies `<Self as pallet_evm::Config>::ChainId`, this chain's
	/// own EVM chain ID — `Pallet::local_chain_id`'s fallback for a `Multichain` product
	/// (`Config::Products` returns `None` for it), used to tell a Hub-vault request (no
	/// Inbound leg — `RequestQueued` follows `Requested` immediately) apart from a
	/// Spoke-vault one (Inbound leg required — `RequestQueued` only reachable once
	/// `RequestBridgeExecuted` has landed) — see `RequestStep`'s doc comment. A
	/// `SingleChain` product uses its own registered chain instead (`Config::Products`
	/// returns `Some`), which plays the identical "no bridge needed" role for that
	/// product alone.
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
		/// Single-chain-product inspector — implemented by pallet-tranche-system. Used to
		/// compute a per-product "local chain" (see `Pallet::local_chain_id`) everywhere
		/// this pallet used to hardcode the literal Hub chain ID to decide "does this
		/// vault/chain need a bridge leg" — a `SingleChain` product's whole stack can be
		/// colocated on any chain, not necessarily Hub, so that decision can no longer be
		/// answered by a single runtime-wide constant alone.
		type Products: ProductInspect;
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	// -----------------------------------------------------------------------
	// Errors
	// -----------------------------------------------------------------------

	#[pallet::error]
	pub enum Error<T> {
		/// `product_id` does not refer to a registered product. Most `record_*`
		/// paths reject an unknown `product_id` transitively (a request/receive/
		/// whitelist can only exist against a vault bound to a real product; a
		/// leg step needs `SettleStarted` to have run first), but
		/// `SettlementStep::SettleStarted` with both chain sets empty has no such
		/// transitive gate — this is its explicit check.
		ProductNotRegistered,
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
		/// bounded by `MAX_MULTICHAIN_ADAPTERS`. Surfaced both by `RequestQueued`'s own
		/// declared `adapter_chain_ids` and by `AdapterBridgeExecuted`/`AdapterApplied`
		/// self-declaring a not-yet-seen chain (see `Pallet::ensure_adapter_chain_declared`).
		TooManyAdapterChains,
		/// A declared chain set repeated a `chain_id` within itself —
		/// `adapter_chain_ids` (`RequestQueued`), or `collect_response_chain_ids`/
		/// `finalize_chain_ids` (`SettleStarted`). Each is a set; a chain
		/// appearing in *both* the collect/response and finalize sets is fine (it
		/// needs both legs), a chain listed twice in the *same* set is not.
		DuplicateDeclaredChain,
		/// `step == RequestStep::RequestBridgeExecuted` was recorded for a request whose
		/// vault is on its product's own local chain (`Pallet::local_chain_id`) — such a
		/// request has no Inbound leg at all (there's nothing to bridge when the vault is
		/// already colocated with Valuation — Hub for a `Multichain` product's Hub-vault
		/// request, or the product's own chain for a `SingleChain` product).
		UnexpectedInboundLeg,
		/// The step being recorded skips over an earlier, not-yet-recorded step.
		RequestStepOutOfOrder,
		/// This step has already been recorded for this request.
		RequestStepAlreadyRecorded,
		/// `step` must be one of the six recordable values — never
		/// `RequestStep::None`/`RequestCompleted`, both read-only sentinels.
		InvalidRequestStep,
		/// No `FlowVersion` has been registered yet for this `product_id` — see
		/// `pallet_tranche_system::RequestFlowVersion`'s storage doc comment
		/// (that pallet owns the storage; this one only reads it, via
		/// `T::Products::request_flow_version`). `record_request_tx` only
		/// needs this looked up for `step == RequestStep::Extended`; every
		/// other step is identical across every `FlowVersion`.
		FlowVersionNotSet,
		/// `step == RequestStep::Extended` was recorded for a product whose
		/// registered `FlowVersion` has no extended steps of its own
		/// (`FlowVersion::V1`) — its whole pipeline is the six core
		/// `RequestStep` values.
		WrongFlowVersion,
		/// `extra` must be `None` for every step other than
		/// `RequestStep::Extended`.
		UnexpectedRequestExtra,
		/// `extra` must be `Some` when `step == RequestStep::Extended`.
		RequestExtraRequired,
		/// `extra` failed to decode as the calling product's registered
		/// `FlowVersion`'s extension payload.
		BadRequestExtra,
		/// `step` must be one of the ten recordable values — never
		/// `SettlementStep::Queued`, the one read-only sentinel.
		InvalidSettlementStep,
		/// `collect_response_chain_ids` and `finalize_chain_ids` must both be `Some`
		/// when `step == SettlementStep::SettleStarted` (each may independently be
		/// empty — a chain missing from both sets needs no leg at all for this
		/// settlement; both empty means the settlement needs no cross-chain action
		/// at all).
		SpokeChainIdsRequired,
		/// `collect_response_chain_ids` and `finalize_chain_ids` must both be `None`
		/// for every step other than `SettleStarted`.
		UnexpectedSpokeChainIds,
		/// `spoke_chain_id` must be `Some` for every leg step (every step other than
		/// `SettleStarted`/`RequestsApproved`/`Settled`).
		SpokeChainIdRequired,
		/// `spoke_chain_id` must be `None` when `step == SettleStarted`,
		/// `RequestsApproved`, or `Settled`.
		UnexpectedSpokeChainId,
		/// `request_ids` must be non-empty when `step == SettlementStep::RequestsApproved`.
		RequestIdsRequired,
		/// `request_ids` must be empty for every step other than `RequestsApproved`.
		UnexpectedRequestIds,
		/// `SettleStarted` has already been recorded for this (product_id, settlement_id)
		/// — via `step == SettlementStep::SettleStarted` or, for a `SingleChain`
		/// product's settlement, `step == SettlementStep::Settled` (see that
		/// variant's doc comment).
		SettlementAlreadyTriggered,
		/// `SettleStarted` has not been recorded yet for this (product_id, settlement_id).
		SettlementNotTriggered,
		/// `step == SettlementStep::Settled` was recorded for a `product_id` that
		/// isn't registered as `SingleChain` (`T::Products::single_chain_id`
		/// returned `None`) — this step exists specifically for a `SingleChain`
		/// product's Contract, which emits `Settled` as its pipeline's only event
		/// (see that variant's doc comment). A `Multichain` product must always
		/// reach `Settled` the ordinary way instead — as a *computed*
		/// `get_settlement` status once every chain completes, never itself
		/// recorded.
		SettledStepNotSingleChain,
		/// A `record_*` step that only exists in a `Multichain` product's
		/// pipeline was recorded against a `SingleChain` `product_id` —
		/// `RequestStep::RequestQueued`/`AdapterBridgeExecuted`/`AdapterApplied`
		/// (a `SingleChain` request is `Requested` alone; Vault, Valuation and
		/// Adapters are all colocated, so there's no queue step and no Adapter
		/// leg), or `WhitelistStep::WhitelistRequested` (a `SingleChain`
		/// whitelist action is `WhitelistApplied` alone; there's no
		/// Orchestrator-driven trigger). The mirror of
		/// `SettledStepNotSingleChain`. See `RequestStep`'s/`WhitelistStep`'s
		/// doc comments and the flow docs.
		MultichainOnlyStep,
		/// `step == SettlementStep::SettleStarted`'s `collect_response_chain_ids`
		/// or `finalize_chain_ids` included the product's own local chain
		/// (`Pallet::local_chain_id` — Hub for a `Multichain` product, or the
		/// product's own chain for a `SingleChain` product). That chain's
		/// completion is tracked via `SettlementStep::NavReceived`/
		/// `try_close_local_requests` instead — it never gets a Spoke-chain leg
		/// of its own (see `SettlementCollectResponseChains`/
		/// `SettlementFinalizeChains`'s doc comments), so a chain declared here
		/// could never reach `SettleApplied`/its own `NavReceived` entry and
		/// would leave the settlement stuck at `SettleStarted` forever.
		LocalChainAsSpokeChain,
		/// `spoke_chain_id` is not among the chains registered for the leg kind
		/// being recorded — `collect_response_chain_ids` for a Collect/Response leg,
		/// `finalize_chain_ids` for a Finalize leg.
		UnknownSpokeChain,
		/// The leg step being recorded skips over an earlier, not-yet-recorded
		/// step in its own leg — its Bridge phase (for a Hooks step), or, for
		/// `NavReceived`, the Collect leg's `NavReported` that produced the NAV
		/// it delivers.
		SettlementStepOutOfOrder,
		/// This leg step has already been recorded for this chain.
		SettlementStepAlreadyRecorded,
		/// No `FlowVersion` has been registered yet for this `product_id`'s
		/// settlement pipeline — see
		/// `pallet_tranche_system::SettlementFlowVersion`'s storage doc
		/// comment. Independent of `FlowVersionNotSet` — each pipeline
		/// versions separately.
		SettlementFlowVersionNotSet,
		/// `step == SettlementStep::Extended` was recorded for a product whose
		/// registered settlement `FlowVersion` has no extended steps of its own
		/// (`FlowVersion::V1`) — its whole pipeline is the nine core
		/// `SettlementStep` values.
		WrongSettlementFlowVersion,
		/// `extra` must be `None` for every step other than
		/// `SettlementStep::Extended`.
		UnexpectedSettlementExtra,
		/// `extra` must be `Some` when `step == SettlementStep::Extended`.
		SettlementExtraRequired,
		/// `extra` failed to decode as the calling product's registered
		/// settlement `FlowVersion`'s extension payload.
		BadSettlementExtra,
		/// `set_tx_recorder` was called with the new value already equal to
		/// what's currently stored — rejected rather than silently accepted
		/// as a no-op, since a genuine call is meant to be a deliberate,
		/// auditable change; a same-value call is never that.
		NoWritingSameValue,
		/// A receive has already been recorded for this (investor, vault, tx_hash).
		ReceiveAlreadyRecorded,
		/// `tx_hash` must not be the zero hash — a zero `tx_hash` can never be a
		/// genuine attested transaction, and `TxRecord::recorded_at == 0` (not
		/// `tx_hash`) is already this pallet's own "not yet recorded" sentinel
		/// everywhere it's read (see e.g. get_request/get_settlement's read-side
		/// docs), so a zero `tx_hash` slipping into storage would be indistinguishable
		/// from a genuine attestation to any caller inspecting `tx_hash` alone.
		TxHashRequired,
		/// `bridge_status` must be `Some` when `step` is one of the Bridge-phase
		/// steps (`RequestBridgeExecuted`/`AdapterBridgeExecuted` for
		/// `record_request_tx`, `CollectBridgeExecuted`/`ResponseBridgeExecuted`/
		/// `FinalizeBridgeExecuted` for `record_settlement_tx`, `BridgeExecuted`
		/// for `record_whitelist_tx`) — there's no other way to know whether the
		/// attempt being recorded is `Executed` or `Reverted`.
		BridgeStatusRequired,
		/// `bridge_status` must be `None` for every step other than the
		/// Bridge-phase ones listed on `BridgeStatusRequired` — those steps have
		/// no attempt list to append to.
		UnexpectedBridgeStatus,
		/// A Bridge-phase leg's attempt list is already at `MAX_BRIDGE_ATTEMPTS`.
		TooManyBridgeAttempts,
		/// A Bridge-phase leg already has an `Executed` attempt — nothing left to
		/// retry, so a further attempt (`Executed` or `Reverted`) is refused
		/// rather than appended. See `BridgeAttempt`'s doc comment on the "at
		/// most one `Executed` ever" invariant.
		BridgeLegAlreadySucceeded,
		/// `step == WhitelistStep::WhitelistRequested` was recorded for a
		/// `(who, vault, nonce)` that already has an entry.
		WhitelistAlreadyTriggered,
		/// No entry exists yet for this `(who, vault, nonce)`, and it doesn't
		/// qualify to self-open one either. For a `Multichain` product,
		/// `record_whitelist_tx` must be called with
		/// `step == WhitelistStep::WhitelistRequested` first — this is the
		/// only way to reach this error there. For a `SingleChain` product,
		/// `step == WhitelistStep::WhitelistApplied` can self-open the entry
		/// instead (see that arm's dev notes); this error there specifically
		/// means `vault` resolved to a registered `Multichain` product, which
		/// doesn't qualify (`VaultNotRegistered` covers the case where `vault`
		/// isn't registered to any product at all).
		WhitelistNotTriggered,
		/// `grant` doesn't match the value this entry was opened with — every step
		/// after `WhitelistRequested` must resupply the same `grant` it was
		/// triggered with (see `WhitelistEntry::grant`'s doc comment for why this
		/// is checked rather than just trusted).
		UnexpectedWhitelistGrant,
		/// `step == WhitelistStep::BridgeExecuted` was recorded for a whitelist
		/// action whose vault is on its product's own local chain
		/// (`Pallet::local_chain_id`) — such an action has no Bridge leg at all
		/// (there's nothing to bridge when TrancheManager already applies the
		/// grant/revoke locally — Hub for a `Multichain` product's Hub-vault action,
		/// or the product's own chain for a `SingleChain` product, which has no
		/// Orchestrator-driven trigger at all — see `record_whitelist_tx`'s dev notes).
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
			adapter_chain_ids: Option<BoundedVec<ChainId, ConstU32<MAX_MULTICHAIN_ADAPTERS>>>,
			/// `Some` iff `step` was a Bridge-phase step (`RequestBridgeExecuted`/
			/// `AdapterBridgeExecuted`) — the attempt's outcome, mirroring what was
			/// just appended to the relevant `bridge_attempts` list.
			bridge_status: Option<BridgeStatus>,
			step: RequestStep,
			chain_id: ChainId,
			tx_hash: H256,
			/// `Some` iff `step == RequestStep::Extended` — the raw bytes this
			/// call decoded into the calling product's `FlowVersion` extension
			/// payload, echoed verbatim. `None` for every other step.
			extra: Option<BoundedVec<u8, ConstU32<MAX_REQUEST_EXTRA_LEN>>>,
		},
		/// One tx in a settlement's pipeline was recorded — either the single
		/// SettleStarted tx (or, for a `SingleChain` product's settlement, that same
		/// tx recorded as `Settled` instead — see that variant's doc comment), one
		/// bridge/hooks half of a per-chain Collect/Response/Finalize leg, or the
		/// (possibly batched) `SettlementStep::RequestsApproved` tx.
		SettlementTxRecorded {
			product_id: ProductId,
			settlement_id: SettlementId,
			spoke_chain_id: Option<ChainId>,
			collect_response_chain_ids:
				Option<BoundedVec<ChainId, ConstU32<MAX_MULTICHAIN_ADAPTERS>>>,
			finalize_chain_ids: Option<BoundedVec<ChainId, ConstU32<MAX_TRANCHE_CHAINS>>>,
			/// `Some` iff `step == SettlementStep::RequestsApproved` — every `request_id`
			/// this call recorded evidence for, in the order supplied.
			request_ids: Option<BoundedVec<RequestId, ConstU32<MAX_SETTLEMENT_REQUESTS>>>,
			/// `Some` iff `step` was a Bridge-phase step (`CollectBridgeExecuted`/
			/// `ResponseBridgeExecuted`/`FinalizeBridgeExecuted`) — same convention
			/// as `RequestTxRecorded::bridge_status`.
			bridge_status: Option<BridgeStatus>,
			step: SettlementStep,
			chain_id: ChainId,
			tx_hash: H256,
			/// `Some` iff `step == SettlementStep::Extended` — the raw bytes this
			/// call decoded into the calling product's settlement `FlowVersion`
			/// extension payload, echoed verbatim. `None` for every other step.
			extra: Option<BoundedVec<u8, ConstU32<MAX_SETTLEMENT_EXTRA_LEN>>>,
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
			/// `Some` iff `step == WhitelistStep::BridgeExecuted` — same convention
			/// as `RequestTxRecorded::bridge_status`.
			bridge_status: Option<BridgeStatus>,
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
	/// alike), and `bridge_attempts` appended to by `RequestBridgeExecuted`
	/// (Spoke-vault only — see `RequestEntry`'s doc comment). Adapter leg
	/// evidence lives in `RequestChainEntries` instead, not here.
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
		BoundedVec<ChainId, ConstU32<MAX_MULTICHAIN_ADAPTERS>>,
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
	/// `close_active_requests`), or for every request colocated with its
	/// product's own local chain (`Pallet::local_chain_id` — a Hub-vault request
	/// in a `Multichain` product, or *any* request in a `SingleChain` product)
	/// approved into the settlement, all at once, the moment
	/// `SettlementCollectResponseChains` has been fully responded to (every chain
	/// has reached `NavReceived` — vacuously true, and checked immediately, if
	/// that set was declared empty at `SettleStarted` time — see
	/// `try_close_local_requests`): at that point it
	/// reads `SettlementRequests::<T>::get(product_id, settlement_id)` for every
	/// request_id approved into that settlement, and removes the ones whose own
	/// origin chain (`RequestEntry::vault::chain_id`) matches the leg just
	/// closed.
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
	#[pallet::unbounded]
	/// Every `request_id` approved into a given `(product_id, settlement_id)`,
	/// in the order `record_settlement_tx`'s `SettlementStep::RequestsApproved` arm
	/// recorded them (possibly several at once, from one batch call) — this
	/// pallet's own copy of the request<->settlement linkage, read by
	/// `close_active_requests`/`try_close_local_requests`/
	/// `try_close_request` to know which `InvestorActiveRequests` entries to
	/// drop once a settlement (or one of its legs) completes. Originally this
	/// linkage was queried cross-pallet from pallet-tranche-investments (via
	/// the now-removed `RequestSettlementInspect` trait), but that pallet only
	/// ever learns of the approval secondhand — Valuation's
	/// `DepositsApproved`/`RedeemsApproved` events are the same events this
	/// pallet's own `SettlementStep::RequestsApproved` step is recorded from — so
	/// keeping an independent, event-sourced copy here removes the hard
	/// dependency entirely rather than just hiding it behind a trait.
	///
	/// Deliberately unbounded, same `#[pallet::unbounded]` rationale as
	/// `InvestorActiveRequests` — growth is bounded in practice by how many
	/// requests a single settlement cycle can genuinely batch (and, per
	/// `RequestsApproved` call, by `MAX_SETTLEMENT_REQUESTS`), not by anything this
	/// pallet caps directly here.
	pub type SettlementRequests<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		ProductId,
		Blake2_128Concat,
		SettlementId,
		Vec<RequestId>,
		ValueQuery,
	>;

	#[pallet::storage]
	/// A settlement's SettleStarted evidence — written by `step ==
	/// SettlementStep::SettleStarted`, or, for a `SingleChain` product's
	/// settlement, `step == SettlementStep::Settled` (see that variant's doc
	/// comment). Keyed by `(product_id, settlement_id)` — `settlement_id` is
	/// only unique within `product_id`'s own namespace, same rationale as
	/// `RequestEntries`' key shape. Presence of an entry here (rather than a
	/// `SettlementStep::Queued`-tagged value) is what answers "has this
	/// settlement started settling yet" — mirrors how `RequestEntries` uses
	/// entry-presence rather than an explicit sentinel step.
	pub type SettlementTriggers<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		ProductId,
		Blake2_128Concat,
		SettlementId,
		TxRecord<BlockNumberFor<T>>,
	>;

	#[pallet::storage]
	/// A settlement's settlement-wide `FlowVersion` extension state — see
	/// `SettlementFlowExtension`'s doc comment. Keyed by `(product_id,
	/// settlement_id)`, same as `SettlementTriggers`. Absent means "no
	/// settlement-wide extension recorded" — unlike `RequestEntry::extension`,
	/// this needs no backfill migration when introduced (see
	/// `SettlementFlowExtension`'s doc comment for why).
	pub type SettlementExtension<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		ProductId,
		Blake2_128Concat,
		SettlementId,
		SettlementFlowExtension<BlockNumberFor<T>>,
	>;

	#[pallet::storage]
	/// The chains registered for a settlement's Collect/Response legs at
	/// SettleStarted time (those with a registered Adapter, excluding Hub
	/// itself — an Adapter on Hub is queried locally, no Bridge&Call leg
	/// needed), in the order the recorder supplied them. A chain never
	/// reaches `NavReceived` unless it's in this set. Always written
	/// alongside `SettlementTriggers` and `SettlementFinalizeChains` (all by
	/// the same `record_settlement_tx` call for `step ==
	/// SettlementStep::SettleStarted`, or, for a `SingleChain` product's
	/// settlement, `step == SettlementStep::Settled` with both recorded
	/// empty) — kept as separate storage items rather than folded into one
	/// struct, same pattern already used by
	/// `pallet_tranche_investments`' `AdapterValuations`/`ProductNavs`/
	/// `Settlements` (three separate maps written together by one
	/// extrinsic).
	pub type SettlementCollectResponseChains<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		ProductId,
		Blake2_128Concat,
		SettlementId,
		BoundedVec<ChainId, ConstU32<MAX_MULTICHAIN_ADAPTERS>>,
	>;

	#[pallet::storage]
	/// The chains registered for a settlement's Finalize leg at SettleStarted time
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
		BoundedVec<ChainId, ConstU32<MAX_TRANCHE_CHAINS>>,
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
			ensure!(old.as_ref() != Some(&recorder), Error::<T>::NoWritingSameValue);
			TxRecorder::<T>::put(&recorder);

			Self::deposit_event(Event::TxRecorderSet { old, new: recorder });
			Ok(())
		}

		/// Attest to one tx in a request's pipeline — the single Requested tx, the
		/// single RequestQueued tx, one Bridge half of the Inbound leg (Spoke-vault
		/// requests only), or one Bridge/Applied half of a per-chain Adapter leg (see
		/// `RequestStep`'s doc comment for exactly which chains need one). A request's
		/// link to a settlement is recorded separately, via `record_settlement_tx`'s
		/// `SettlementStep::RequestsApproved` — see that variant's doc comment. Origin must be
		/// `RecorderOrigin`. `opening` MUST be `Some` iff `step ==
		/// RequestStep::Requested`, `None` otherwise. `adapter_chain_ids` MUST be
		/// `Some` iff `step == RequestStep::RequestQueued` (Hub-vault or Spoke-vault
		/// alike), `None` otherwise. `bridge_status` MUST be `Some` iff `step` is
		/// `RequestBridgeExecuted` or `AdapterBridgeExecuted` (the two Bridge-phase
		/// steps), `None` otherwise — see interface.sol's `record_request_tx` for the
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
		///
		/// `extra` MUST be `Some` iff `step == RequestStep::Extended`, `None`
		/// otherwise — its bytes are decoded according to `product_id`'s
		/// registered `RequestFlowVersion`, never interpreted by this step's
		/// own dispatch logic directly. See `RequestStep::Extended`'s doc
		/// comment for the full mechanism.
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::record_request_tx(
			extra.as_ref().map_or(0, |bytes| bytes.len() as u32)
		))]
		pub fn record_request_tx(
			origin: OriginFor<T>,
			product_id: ProductId,
			request_id: RequestId,
			opening: Option<RequestOpening>,
			adapter_chain_ids: Option<BoundedVec<ChainId, ConstU32<MAX_MULTICHAIN_ADAPTERS>>>,
			step: RequestStep,
			chain_id: ChainId,
			tx_hash: H256,
			bridge_status: Option<BridgeStatus>,
			extra: Option<BoundedVec<u8, ConstU32<MAX_REQUEST_EXTRA_LEN>>>,
		) -> DispatchResult {
			T::RecorderOrigin::ensure_origin(origin)?;
			ensure!(!tx_hash.is_zero(), Error::<T>::TxHashRequired);
			if step != RequestStep::Extended {
				ensure!(extra.is_none(), Error::<T>::UnexpectedRequestExtra);
			}

			let recorded_at = frame_system::Pallet::<T>::block_number();
			let tx = TxRecord { chain_id, tx_hash, recorded_at };
			let local_chain_id = Self::local_chain_id(product_id);

			match step {
				RequestStep::Requested => Self::handle_requested(
					product_id,
					request_id,
					opening.clone(),
					adapter_chain_ids.clone(),
					bridge_status,
					tx,
				)?,
				RequestStep::RequestBridgeExecuted => Self::handle_request_bridge_executed(
					product_id,
					request_id,
					opening.clone(),
					adapter_chain_ids.clone(),
					bridge_status,
					tx,
					local_chain_id,
				)?,
				RequestStep::RequestQueued => Self::handle_request_queued(
					product_id,
					request_id,
					opening.clone(),
					adapter_chain_ids.clone(),
					bridge_status,
					tx,
					local_chain_id,
				)?,
				RequestStep::AdapterBridgeExecuted => Self::handle_adapter_bridge_executed(
					product_id,
					request_id,
					opening.clone(),
					adapter_chain_ids.clone(),
					bridge_status,
					chain_id,
					tx,
				)?,
				RequestStep::AdapterApplied => Self::handle_adapter_applied(
					product_id,
					request_id,
					opening.clone(),
					adapter_chain_ids.clone(),
					bridge_status,
					chain_id,
					tx,
				)?,
				RequestStep::Extended => Self::handle_request_extended(
					product_id,
					request_id,
					opening.clone(),
					adapter_chain_ids.clone(),
					bridge_status,
					extra.clone(),
				)?,
				RequestStep::None | RequestStep::RequestCompleted => {
					return Err(Error::<T>::InvalidRequestStep.into());
				},
			}

			Self::deposit_event(Event::RequestTxRecorded {
				product_id,
				request_id,
				opening,
				adapter_chain_ids,
				bridge_status,
				step,
				chain_id,
				tx_hash,
				extra,
			});
			Ok(())
		}

		/// Attest to one tx in a settlement's pipeline: the single SettleStarted tx
		/// (or, for a `SingleChain` product's settlement, that same tx recorded
		/// directly as `Settled` instead — see that variant's doc comment), the
		/// (possibly batched) RequestsApproved tx, or one bridge/hooks half of a
		/// Collect/Response/Finalize leg for one chain. Origin must be
		/// `RecorderOrigin`. `collect_response_chain_ids` and `finalize_chain_ids`
		/// MUST both be `Some` iff `step == SettlementStep::SettleStarted` (each may
		/// independently be empty — see below), and MUST both be `None` for
		/// `step == SettlementStep::Settled` (its chain sets are always empty, so
		/// there's nothing for the caller to supply); `request_ids` MUST be
		/// non-empty iff `step == SettlementStep::RequestsApproved`; `spoke_chain_id`
		/// MUST be `Some` for every leg step (every step other than
		/// `SettleStarted`/`RequestsApproved`/`Settled`, all three of which are
		/// settlement-wide rather than chain-scoped).
		/// `bridge_status` MUST be `Some` iff `step` is one of the three
		/// Bridge-phase leg steps (`CollectBridgeExecuted`/`ResponseBridgeExecuted`/
		/// `FinalizeBridgeExecuted`), `None` otherwise — see interface.sol's
		/// `record_settlement_tx` for the full contract this mirrors.
		///
		/// The two chain-id sets declare, per chain, which leg kind(s) it needs —
		/// Collect/Response for a chain with a registered Adapter, Finalize for a
		/// chain with a registered vault, both for a chain with both (excluding Hub
		/// itself in either case — see `SettlementCollectResponseChains`/
		/// `SettlementFinalizeChains`'s doc comments). A chain absent from
		/// `finalize_chain_ids` never blocks completion on a Finalize leg it was
		/// never going to get — completion waits on `NavReceived` for it instead
		/// (see `SettlementStep`'s doc comment). A settlement needing no
		/// cross-chain action at all is recorded as `SettleStarted` with both sets
		/// empty — every read path already reports this correctly via vacuous
		/// truth, with no dedicated step needed for that case; a `SingleChain`
		/// product's settlement whose Contract emits `Settled` as its only event
		/// instead uses `step == SettlementStep::Settled` directly (same storage
		/// writes, same immediate completion) — see that variant's doc comment for
		/// why the two need separate step values despite doing the same thing
		/// underneath.
		///
		/// `request_ids` records every `request_id` Valuation approved into this
		/// settlement in one call — see `SettlementStep::RequestsApproved`'s doc comment
		/// for the full mechanism, including why this replaced a per-request
		/// `record_request_tx` step. Rejects the whole call (atomic, like any other
		/// extrinsic) if any `request_id` in the batch doesn't have an open
		/// `RequestEntries` entry, or already has one recorded — same behavior a
		/// caller would see repeating the old per-request step for a duplicate.
		///
		/// Side effects on `InvestorActiveRequests` (see its own storage doc comment
		/// for the full mechanism — `SettlementRequests` +
		/// `ActiveRequestClosed`): `step == SettlementStep::SettleApplied`
		/// closes every Spoke-vault request approved into this settlement whose own
		/// origin chain is `spoke_chain_id`. `step == SettlementStep::SettleStarted`,
		/// `step == SettlementStep::Settled`, and `step == SettlementStep::NavReceived`
		/// all additionally try to close every request colocated with its product's
		/// own local chain (`Pallet::local_chain_id` — a Hub-vault request in a
		/// `Multichain` product, or *any* request in a `SingleChain` product)
		/// approved into this settlement, via `try_close_local_requests` — such a
		/// request has no Finalize leg of its own to close on (see `SettlementStep`'s
		/// doc comment), so this runs instead every time `collect_response_chain_ids`
		/// might have just become fully responded (including vacuously, right at
		/// `SettleStarted`/`Settled`, if it was declared empty — always the case for
		/// a `SingleChain` product's own settlement, since its Adapters are
		/// colocated too and never need a Collect/Response leg).
		/// `step == SettlementStep::RequestsApproved` tries to
		/// close each request in the batch individually right after linking it in,
		/// same race-handling rationale as `SettlementStep::RequestsApproved`'s doc comment.
		/// `extra` MUST be `Some` iff `step == SettlementStep::Extended`, `None`
		/// otherwise — its bytes are decoded according to `product_id`'s
		/// registered `SettlementFlowVersion`, routed by whether
		/// `spoke_chain_id` is `Some` (chain-scoped) or `None`
		/// (settlement-wide). See `SettlementStep::Extended`'s doc comment for
		/// the full mechanism.
		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::record_settlement_tx(
			request_ids.as_ref().map_or(0, |ids| ids.len() as u32),
			extra.as_ref().map_or(0, |bytes| bytes.len() as u32),
		))]
		pub fn record_settlement_tx(
			origin: OriginFor<T>,
			product_id: ProductId,
			settlement_id: SettlementId,
			spoke_chain_id: Option<ChainId>,
			collect_response_chain_ids: Option<
				BoundedVec<ChainId, ConstU32<MAX_MULTICHAIN_ADAPTERS>>,
			>,
			finalize_chain_ids: Option<BoundedVec<ChainId, ConstU32<MAX_TRANCHE_CHAINS>>>,
			request_ids: Option<BoundedVec<RequestId, ConstU32<MAX_SETTLEMENT_REQUESTS>>>,
			step: SettlementStep,
			chain_id: ChainId,
			tx_hash: H256,
			bridge_status: Option<BridgeStatus>,
			extra: Option<BoundedVec<u8, ConstU32<MAX_SETTLEMENT_EXTRA_LEN>>>,
		) -> DispatchResult {
			T::RecorderOrigin::ensure_origin(origin)?;
			ensure!(!tx_hash.is_zero(), Error::<T>::TxHashRequired);
			if step != SettlementStep::Extended {
				ensure!(extra.is_none(), Error::<T>::UnexpectedSettlementExtra);
			}

			let recorded_at = frame_system::Pallet::<T>::block_number();
			let tx = TxRecord { chain_id, tx_hash, recorded_at };

			// `collect_response_chain_ids`/`finalize_chain_ids`/`request_ids` are
			// only ever genuinely *owned* by one handler each
			// (`handle_settle_started` for the first two, `handle_requests_approved`
			// for the third) — every other handler below only checks `.is_none()`
			// on them and takes a reference instead, so this router only pays for
			// a real clone where the value is actually about to be moved into
			// storage. `request_ids` is the one worth avoiding: it's bounded at
			// `MAX_SETTLEMENT_REQUESTS` (1000), unlike the two chain-id sets
			// (bounded at 10) — cloning it for a call that's just going to reject
			// it with `Error::UnexpectedRequestIds` wastes real work.
			match step {
				SettlementStep::SettleStarted => Self::handle_settle_started(
					product_id,
					settlement_id,
					spoke_chain_id,
					collect_response_chain_ids.clone(),
					finalize_chain_ids.clone(),
					&request_ids,
					bridge_status,
					tx,
				)?,
				SettlementStep::Settled => Self::handle_settled(
					product_id,
					settlement_id,
					spoke_chain_id,
					&collect_response_chain_ids,
					&finalize_chain_ids,
					&request_ids,
					bridge_status,
					tx,
				)?,
				SettlementStep::RequestsApproved => Self::handle_requests_approved(
					product_id,
					settlement_id,
					spoke_chain_id,
					&collect_response_chain_ids,
					&finalize_chain_ids,
					request_ids.clone(),
					bridge_status,
					tx,
				)?,
				SettlementStep::Extended => Self::handle_settlement_extended(
					product_id,
					settlement_id,
					&collect_response_chain_ids,
					&finalize_chain_ids,
					&request_ids,
					bridge_status,
					extra.clone(),
				)?,
				SettlementStep::CollectBridgeExecuted
				| SettlementStep::NavReported
				| SettlementStep::ResponseBridgeExecuted
				| SettlementStep::NavReceived
				| SettlementStep::FinalizeBridgeExecuted
				| SettlementStep::SettleApplied => Self::handle_settlement_leg_step(
					product_id,
					settlement_id,
					spoke_chain_id,
					&collect_response_chain_ids,
					&finalize_chain_ids,
					&request_ids,
					step,
					bridge_status,
					tx,
				)?,
				SettlementStep::Queued => {
					return Err(Error::<T>::InvalidSettlementStep.into());
				},
			}

			Self::deposit_event(Event::SettlementTxRecorded {
				product_id,
				settlement_id,
				spoke_chain_id,
				collect_response_chain_ids,
				finalize_chain_ids,
				request_ids,
				bridge_status,
				step,
				chain_id,
				tx_hash,
				extra,
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
		/// opened with — reverts on mismatch. `product_id` is not a
		/// parameter — resolved internally via `T::Vaults::product_id_for_vault(&vault)`
		/// when the entry is opened, since none of this pipeline's
		/// chain-observed evidence carries it directly the way
		/// `DepositRequested`/`DepositReceived` do.
		///
		/// `step == BridgeExecuted` reverts (`Error::UnexpectedWhitelistBridgeLeg`)
		/// if `vault` is on its product's own local chain (`Pallet::local_chain_id`)
		/// — see `WhitelistStep`'s doc comment for why such an action has no Bridge
		/// leg at all. `bridge_status` MUST be `Some` iff `step == BridgeExecuted`,
		/// `None` otherwise.
		///
		/// A `Multichain` product's action still requires `WhitelistStep::WhitelistRequested`
		/// to open the entry first (Hub-vault or Spoke-vault alike) — `WhitelistApplied`/
		/// `BridgeExecuted` both revert with `Error::WhitelistNotTriggered` otherwise. A
		/// `SingleChain` product's action has no Orchestrator-driven `WhitelistRequested`
		/// at all — its TrancheManager manages `nonce` itself and applies the grant/revoke
		/// in one local step — so `WhitelistApplied` self-opens the entry instead when
		/// none exists yet and `vault` resolves to a registered `SingleChain` product (see
		/// that arm's dev notes).
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
			bridge_status: Option<BridgeStatus>,
		) -> DispatchResult {
			T::RecorderOrigin::ensure_origin(origin)?;
			ensure!(!tx_hash.is_zero(), Error::<T>::TxHashRequired);

			let recorded_at = frame_system::Pallet::<T>::block_number();
			let tx = TxRecord { chain_id, tx_hash, recorded_at };

			let product_id = match step {
				WhitelistStep::WhitelistRequested => Self::handle_whitelist_requested(
					vault.clone(),
					who,
					grant,
					nonce,
					bridge_status,
					tx,
				)?,
				WhitelistStep::BridgeExecuted => Self::handle_whitelist_bridge_executed(
					vault.clone(),
					who,
					grant,
					nonce,
					bridge_status,
					tx,
				)?,
				WhitelistStep::WhitelistApplied => Self::handle_whitelist_applied(
					vault.clone(),
					who,
					grant,
					nonce,
					bridge_status,
					tx,
				)?,
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
				bridge_status,
				step,
				chain_id,
				tx_hash,
			});
			Ok(())
		}
	}
}
