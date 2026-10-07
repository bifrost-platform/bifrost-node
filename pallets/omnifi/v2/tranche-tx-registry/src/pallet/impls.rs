use crate::{
	history, BridgeAttempt, BridgeAttempts, BridgeStatus, ChainAddress, ChainId, HistoryPage,
	LegChunk, LegChunks, PagedInvestorHistory, ProductId, RequestEntry, RequestExtraV2, RequestId,
	RequestOpening, SettlementChainEntry, SettlementExtraV2, SettlementId, SettlementStep, TxHash,
	TxRecord, WhitelistEntry, WhitelistNonce, MAX_REQUEST_EXTRA_LEN, MAX_SETTLEMENT_EXTRA_LEN,
	MAX_SETTLEMENT_REQUESTS,
};
use pallet_tranche_system::{
	AdapterInspect, FlowVersion, ProductInspect, VaultId, VaultInspect, MAX_MULTICHAIN_ADAPTERS,
	MAX_TRANCHE_CHAINS,
};

use super::pallet::*;
use core::marker::PhantomData;
use frame_support::{
	ensure,
	pallet_prelude::{BoundedVec, Decode, DispatchError, DispatchResult},
	traits::Get,
};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_core::ConstU32;
use sp_std::vec::Vec;

// Private, non-extrinsic helpers — kept in their own `impl` block, separate from
// `#[pallet::call]`, so they don't become part of the `Call` enum.
impl<T: Config> Pallet<T> {
	/// `true` iff some attempt in `attempts` resolved `Executed` — the "did
	/// this leg actually complete" question every downstream gate
	/// (`RequestQueued`, `NavReported`, `NavReceived`, `SettleApplied`,
	/// `WhitelistApplied`) asks, per `BridgeAttempts`'s doc comment. NOT the
	/// same as `!attempts.is_empty()` — a list full of `Reverted` attempts
	/// still answers `false` here (awaiting retry).
	pub(crate) fn bridge_succeeded(attempts: &BridgeAttempts<BlockNumberFor<T>>) -> bool {
		attempts.iter().any(|attempt| attempt.status == BridgeStatus::Executed)
	}

	/// Appends one observed attempt to a leg's attempt list — shared by every
	/// Bridge-phase arm across `record_request_tx`/`record_settlement_tx`/
	/// `record_whitelist_tx`. Rejects with `Error::BridgeLegAlreadySucceeded`
	/// if the leg already has an `Executed` attempt (nothing left to retry —
	/// see `BridgeAttempt`'s doc comment on the "at most one `Executed` ever"
	/// invariant), and with `Error::TooManyBridgeAttempts` if the list is
	/// already at `MAX_BRIDGE_ATTEMPTS`. Unlike the old `Option`-overwrite
	/// shape this replaces, a `Reverted` attempt is never itself an error —
	/// only a second attempt after a success is.
	pub(crate) fn push_bridge_attempt(
		attempts: &mut BridgeAttempts<BlockNumberFor<T>>,
		status: BridgeStatus,
		tx: TxRecord<BlockNumberFor<T>>,
	) -> DispatchResult {
		ensure!(!Self::bridge_succeeded(attempts), Error::<T>::BridgeLegAlreadySucceeded);
		attempts
			.try_push(BridgeAttempt { status, tx })
			.map_err(|_| Error::<T>::TooManyBridgeAttempts)?;
		Ok(())
	}

	/// Self-declares `chain_id` into `RequestAdapterChains` if it isn't already
	/// there — shared by `record_request_tx`'s `AdapterBridgeExecuted`/
	/// `AdapterApplied` arms. Adapter-leg evidence for a chain can arrive before
	/// `RequestStep::RequestQueued` ever explicitly declares it: the origin vault's
	/// own chain, or the Hub chain, can self-fulfill a weighted Adapter allocation
	/// synchronously (no Bridge leg at all — see `RequestStep`'s doc comment), so
	/// the recorder may observe that chain's `AdapterApplied` evidence before the
	/// pipeline event that would normally declare it. This lets `record_request_tx`
	/// accept `AdapterBridgeExecuted`/`AdapterApplied` calls in whatever order the
	/// recorder actually observed the underlying events, rather than requiring
	/// `RequestQueued`'s own declaration to land first. `RequestQueued` itself
	/// merges its own `adapter_chain_ids` into whatever's already here rather than
	/// overwriting, so self-declared chains survive it.
	///
	/// Requires `RequestEntries` to already exist for `(product_id, request_id)`
	/// (security-review 2026-09-18 L3, originally 08-27 M2 — v1 left unfixed,
	/// see that pallet's copy of this function, to keep existing v1 products'
	/// weight/behavior frozen) — this self-declaring path is the one gap in the
	/// pipeline that used to skip that check entirely (unlike `RequestQueued`'s
	/// own handler, which already has `entry` loaded by the time it merges
	/// `adapter_chain_ids`), letting a mistyped `request_id` in an
	/// `AdapterBridgeExecuted`/`AdapterApplied` call silently create orphaned
	/// `RequestChainEntries`/`RequestAdapterChains` for a request that was never
	/// actually opened via `RequestStep::Requested`.
	pub(crate) fn ensure_adapter_chain_declared(
		product_id: ProductId,
		request_id: RequestId,
		chain_id: ChainId,
	) -> DispatchResult {
		ensure!(
			RequestEntries::<T>::contains_key(product_id, request_id),
			Error::<T>::RequestNotOpened
		);
		let mut chains = RequestAdapterChains::<T>::get(product_id, request_id).unwrap_or_default();
		if !chains.contains(&chain_id) {
			ensure!(
				T::Adapters::adapter_chains_belong_to_product(product_id, &[chain_id]),
				Error::<T>::SpokeChainNotRegistered
			);
			chains.try_push(chain_id).map_err(|_| Error::<T>::TooManyAdapterChains)?;
			RequestAdapterChains::<T>::insert(product_id, request_id, chains);
		}
		Ok(())
	}

	/// The chain `product_id`'s own request/settlement/whitelist pipeline needs no
	/// bridge leg for — this Hub chain's own `ChainId` for a `Multichain` product
	/// (`T::Products::single_chain_id` returns `None` for it), or that product's
	/// own single chain for a `SingleChain` product (see `ProductInspect`'s doc
	/// comment for why the two models share this same "colocated with Valuation,
	/// no bridge needed" reasoning despite the chain itself differing). Used
	/// everywhere this pallet used to hardcode the literal Hub chain ID to decide
	/// "does this vault/chain need a bridge leg."
	pub(crate) fn local_chain_id(product_id: ProductId) -> ChainId {
		T::Products::single_chain_id(product_id)
			.unwrap_or_else(<T as pallet_evm::Config>::ChainId::get)
	}

	/// Rejects a `record_*` step that only exists in a `Multichain` product's
	/// pipeline (`RequestStep::RequestQueued`/`AdapterBridgeExecuted`/
	/// `AdapterApplied`, `WhitelistStep::WhitelistRequested`). A `SingleChain`
	/// product's request flow is `Requested` alone and its whitelist flow is
	/// `WhitelistApplied` alone — everything is colocated, so there's no
	/// Valuation-Contract queue step, no Adapter leg, and no Orchestrator-driven
	/// trigger (see `RequestStep`'s/`WhitelistStep`'s doc comments). The mirror
	/// of `Error::SettledStepNotSingleChain`.
	///
	/// `single_chain_id(product_id).is_none()` is also true for an unregistered
	/// `product_id` — harmless here: every call site that reaches this already
	/// rejects a nonexistent product downstream (`RequestNotOpened` /
	/// `SpokeChainNotRegistered` / `VaultNotRegistered`).
	fn ensure_multichain_product(product_id: ProductId) -> DispatchResult {
		ensure!(T::Products::single_chain_id(product_id).is_none(), Error::<T>::MultichainOnlyStep);
		Ok(())
	}

	/// Rejects a declared chain set (`adapter_chain_ids` /
	/// `collect_response_chain_ids` / `finalize_chain_ids`) that repeats a
	/// `chain_id` within itself — each is meant to be a set. A repeat would
	/// inflate `get_settlement`'s per-chain output (via `union_chain_ids` in the
	/// precompile, which preserves duplicates within its first argument) and
	/// waste re-checks in `local_settlement_complete`. Cross-set repeats
	/// (`collect_response` and `finalize` sharing a chain) are fine and expected
	/// — a chain with both an Adapter and a vault needs both legs. O(n^2) over a
	/// set bounded at `MAX_MULTICHAIN_ADAPTERS`/`MAX_TRANCHE_CHAINS` (10), same
	/// shape as `pallet_tranche_system`'s own small-slice dup checks.
	fn ensure_no_duplicate_chain(chain_ids: &[ChainId]) -> DispatchResult {
		for (i, id) in chain_ids.iter().enumerate() {
			ensure!(!chain_ids[i + 1..].contains(id), Error::<T>::DuplicateDeclaredChain);
		}
		Ok(())
	}

	/// Removes `(product_id, request_id)` from `investor`'s
	/// `InvestorActiveRequests` list and emits `ActiveRequestClosed`, if it's
	/// still there — a no-op otherwise (already closed by an earlier call).
	/// Shared by `close_active_requests` (settlement-wide, chain-filtered) and
	/// `try_close_request` (single request, opportunistic) so the removal logic
	/// only lives in one place. `tx` is this call's own attestation — attributed
	/// as the request's `settled_tx` (`RequestEntry::settled_tx`) iff that field
	/// isn't already set (first writer wins; see that field's doc comment for
	/// why an earlier, more precise attribution from opportunistic per-chunk
	/// close — §6.7 of the design doc — is never overwritten by a later,
	/// less-precise whole-leg backstop attribution). A no-op, cheaply, if
	/// `(product_id, request_id)` isn't in `investor`'s `InvestorActiveRequests`
	/// at all (already closed, or never opened) — safe to call more than once
	/// for the same request.
	fn close_one_active_request(
		product_id: ProductId,
		request_id: RequestId,
		investor: ChainAddress,
		tx: TxRecord<BlockNumberFor<T>>,
	) {
		let mut requests = InvestorActiveRequests::<T>::get(investor);
		let Some(pos) = requests.iter().position(|r| *r == (product_id, request_id)) else {
			return;
		};
		requests.swap_remove(pos);
		InvestorActiveRequests::<T>::insert(investor, requests);
		RequestEntries::<T>::mutate(product_id, request_id, |maybe_entry| {
			if let Some(entry) = maybe_entry {
				if entry.settled_tx.is_none() {
					entry.settled_tx = Some(tx);
				}
			}
		});
		Self::deposit_event(Event::ActiveRequestClosed { product_id, request_id, investor });
	}

	/// Shared by `record_settlement_tx`'s `SettleApplied` leg arm (called
	/// with `chain_id = Some(spoke_chain_id)`) and `try_close_local_requests`
	/// (called with `chain_id = Some(local_chain_id)`): closes out
	/// `InvestorActiveRequests` for every request approved into `settlement_id`
	/// whose own origin chain is `chain_id` — see `InvestorActiveRequests`'s
	/// storage doc comment for the full mechanism. `chain_id = None` closes every
	/// request approved into `settlement_id` unconditionally; no current caller
	/// uses this (every real chain, including each product's own local chain, is
	/// always known at the call site), kept only because it costs nothing to
	/// leave the filter optional. `tx` — see `close_one_active_request`'s own
	/// doc comment.
	/// Reads `SettlementRequests` — this pallet's own event-sourced copy of the
	/// request<->settlement linkage, written by `record_settlement_tx`'s
	/// `SettlementStep::RequestsApproved` arm (see that storage's own doc comment
	/// for why it's no longer queried cross-pallet from
	/// pallet-tranche-investments).
	///
	/// Returns the number of `SettlementRequests` entries scanned (not the
	/// number actually closed — the `chain_id`/`RequestNotOpened` skips below
	/// are cheap relative to a full `close_one_active_request`, but the read
	/// of each entry itself is not) — this is the weight-relevant quantity
	/// `record_settlement_tx`'s dispatchable uses to compute its actual
	/// `WeightInfo::close_cascade` post-dispatch weight (security-review C1;
	/// this cascade previously ran uncharged, see `benchmarking.rs`'s own doc
	/// comment on the fix).
	pub(crate) fn close_active_requests(
		product_id: ProductId,
		settlement_id: SettlementId,
		chain_id: Option<ChainId>,
		tx: TxRecord<BlockNumberFor<T>>,
	) -> u32 {
		let requests = SettlementRequests::<T>::get(product_id, settlement_id);
		let scanned = requests.len() as u32;
		for request_id in requests {
			let Some(request_entry) = RequestEntries::<T>::get(product_id, request_id) else {
				continue;
			};
			if let Some(chain_id) = chain_id {
				if request_entry.vault.chain_id != chain_id {
					continue;
				}
			}
			Self::close_one_active_request(
				product_id,
				request_id,
				request_entry.investor,
				tx.clone(),
			);
		}
		scanned
	}

	/// A request whose vault is colocated with its product's own Valuation
	/// Contract — a Hub-vault request in a `Multichain` product, or *any*
	/// request in a `SingleChain` product (see `local_chain_id`'s doc comment) —
	/// has its settlement complete once every one of
	/// `SettlementCollectResponseChains` has reached `NavReceived` — NAV must be
	/// fully known before the colocated vault's own payout/allocation can be
	/// computed, which then happens synchronously with no Finalize leg of its
	/// own (see `SettlementStep`'s doc comment). Vacuously true the moment
	/// `collect_response_chain_ids` is declared empty at `SettleStarted`/`Settled`
	/// time — always the case for a `SingleChain` product, since its Adapters are
	/// colocated too — so this is safe to call unconditionally right after
	/// writing that storage, as well as after every `NavReceived` — closes
	/// `InvestorActiveRequests` for every such request approved into
	/// `settlement_id` once true, via
	/// `close_active_requests(chain_id = Some(local_chain_id))`. A no-op (does
	/// nothing, cheaply) if the condition isn't met yet. `tx` — see
	/// `close_one_active_request`'s own doc comment; every call site here
	/// passes the tx of whichever step just made this condition true
	/// (`SettleStarted`/`Settled`'s own tx when vacuously true immediately, or
	/// the completing chunk's tx for a `NavReceived` trigger).
	/// `Some(n)` (`n` = `close_active_requests`' scan size, itself possibly
	/// `0` if `SettlementRequests` happened to be empty) iff the cascade
	/// actually ran; `None` iff the condition wasn't met (no-op) — the `None`
	/// case is deliberately distinct from `Some(0)` so `record_settlement_tx`
	/// can charge exactly `close_cascade(0)`'s real cost only when
	/// `close_active_requests` was actually called, not on every `NavReceived`/
	/// `SettleStarted`/`Settled` call regardless (most of which never reach
	/// completion and so never call it at all).
	pub(crate) fn try_close_local_requests(
		product_id: ProductId,
		settlement_id: SettlementId,
		tx: TxRecord<BlockNumberFor<T>>,
	) -> Option<u32> {
		if Self::local_settlement_complete(product_id, settlement_id) {
			let local_chain_id = Self::local_chain_id(product_id);
			Some(Self::close_active_requests(product_id, settlement_id, Some(local_chain_id), tx))
		} else {
			None
		}
	}

	/// The condition `try_close_local_requests` waits on, factored out so
	/// `try_close_request` can check it for a single request without re-running
	/// `close_active_requests`' settlement-wide scan.
	///
	/// Requires `SettlementTriggers` to actually exist first — without this,
	/// `SettlementCollectResponseChains::get(..).unwrap_or_default()` below
	/// can't tell "not yet triggered at all" from "triggered with an
	/// intentionally empty set," since both read back as absent/empty the
	/// same way, and would otherwise vacuously return `true` for either.
	/// That distinction matters because `try_close_request` (unlike
	/// `try_close_local_requests`'s other two call sites, both of which only
	/// ever run once `SettlementTriggers` is already known to exist) can run
	/// from `SettlementStep::RequestsApproved`, which is deliberately allowed
	/// to land *before* `SettleStarted`/`Settled` for a `SingleChain` SYNC
	/// product (see `SettlementStep::RequestsApproved`'s doc comment) — without
	/// this check, that ordering would let a request be closed out of
	/// `InvestorActiveRequests` before its settlement was ever recorded as
	/// started at all, permanently so if the `SettleStarted`/`Settled` call
	/// never follows.
	fn local_settlement_complete(product_id: ProductId, settlement_id: SettlementId) -> bool {
		if !SettlementTriggers::<T>::contains_key(product_id, settlement_id) {
			return false;
		}
		let collect_response_chains =
			SettlementCollectResponseChains::<T>::get(product_id, settlement_id)
				.unwrap_or_default();
		collect_response_chains.iter().all(|chain_id| {
			Self::response_leg_complete(&SettlementChainEntries::<T>::get((
				product_id,
				settlement_id,
				*chain_id,
			)))
		})
	}

	/// Called right after `record_settlement_tx`'s `SettlementStep::RequestsApproved` arm
	/// links `request_id` into `SettlementRequests` (once per entry in that
	/// call's batch) — closes just this one request out of
	/// `InvestorActiveRequests` immediately if its settlement's completion
	/// condition has *already* landed by the time `RequestsApproved` is recorded. This
	/// exists because `RequestsApproved` races with the settlement-side completion
	/// trigger (`try_close_local_requests`/`close_active_requests` at
	/// `SettleApplied`) — both can fire at essentially the same moment (see
	/// `SettlementStep::RequestsApproved`'s doc comment), via separate,
	/// independently-ordered `record_settlement_tx` calls (RequestsApproved for one
	/// settlement, a leg step for another chain). If the settlement-side trigger
	/// already ran before this request was linked into `SettlementRequests`,
	/// nothing would otherwise ever re-close it — the settlement-side trigger
	/// only iterates whatever was in `SettlementRequests` *at the time it ran*,
	/// and doesn't re-fire once its own condition has already been satisfied. A
	/// no-op if the condition isn't met yet — the settlement-side trigger will
	/// close this request once it is (this request is now linked into
	/// `SettlementRequests`, so it'll be found).
	///
	/// `local_chain_id`/`local_settlement_complete` are passed in rather than
	/// recomputed here: this runs once per `request_id` in a `RequestsApproved`
	/// batch (up to `MAX_SETTLEMENT_REQUESTS`), and both values are invariant
	/// across that whole loop while each is expensive to recompute —
	/// `Self::local_chain_id(product_id)` decodes the entire `ProductDetails`
	/// just to read one field, and `Self::local_settlement_complete(product_id,
	/// settlement_id)` scans up to `MAX_MULTICHAIN_ADAPTERS` `SettlementChainEntries`.
	/// `handle_requests_approved` computes each once, before the loop.
	pub(crate) fn try_close_request(
		product_id: ProductId,
		settlement_id: SettlementId,
		request_id: RequestId,
		local_chain_id: ChainId,
		local_settlement_complete: bool,
		tx: TxRecord<BlockNumberFor<T>>,
	) {
		let Some(request_entry) = RequestEntries::<T>::get(product_id, request_id) else {
			return;
		};
		let complete = if request_entry.vault.chain_id == local_chain_id {
			local_settlement_complete
		} else {
			Self::finalize_leg_complete(&SettlementChainEntries::<T>::get((
				product_id,
				settlement_id,
				request_entry.vault.chain_id,
			)))
		};
		if complete {
			Self::close_one_active_request(product_id, request_id, request_entry.investor, tx);
		}
	}

	// -----------------------------------------------------------------------
	// record_request_tx — one function per `RequestStep`
	// -----------------------------------------------------------------------

	/// `RequestStep::Requested` — opens a fresh `RequestEntries` entry.
	pub(crate) fn handle_requested(
		product_id: ProductId,
		request_id: RequestId,
		opening: Option<RequestOpening>,
		adapter_chain_ids: Option<BoundedVec<ChainId, ConstU32<MAX_MULTICHAIN_ADAPTERS>>>,
		bridge_status: Option<BridgeStatus>,
		tx: TxRecord<BlockNumberFor<T>>,
	) -> DispatchResult {
		let opening = opening.ok_or(Error::<T>::RequestOpeningRequired)?;
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
		ensure!(adapter_chain_ids.is_none(), Error::<T>::UnexpectedRequestAdapterChains);
		ensure!(bridge_status.is_none(), Error::<T>::UnexpectedBridgeStatus);
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
				bridge_attempts: Default::default(),
				queued_tx: None,
				settlement_id: None,
				approved_tx: None,
				settled_tx: None,
				extension: Default::default(),
			},
		);
		InvestorActiveRequests::<T>::mutate(opening.investor, |requests| {
			requests.push((product_id, request_id));
		});
		Self::push_request_history(opening.investor, product_id, request_id);
		Ok(())
	}

	/// `RequestStep::RequestBridgeExecuted` — Inbound leg, Bridge phase
	/// (Spoke-vault requests only).
	pub(crate) fn handle_request_bridge_executed(
		product_id: ProductId,
		request_id: RequestId,
		opening: Option<RequestOpening>,
		adapter_chain_ids: Option<BoundedVec<ChainId, ConstU32<MAX_MULTICHAIN_ADAPTERS>>>,
		bridge_status: Option<BridgeStatus>,
		tx: TxRecord<BlockNumberFor<T>>,
		local_chain_id: ChainId,
	) -> DispatchResult {
		ensure!(opening.is_none(), Error::<T>::UnexpectedRequestOpening);
		ensure!(adapter_chain_ids.is_none(), Error::<T>::UnexpectedRequestAdapterChains);
		let bridge_status = bridge_status.ok_or(Error::<T>::BridgeStatusRequired)?;
		let mut entry =
			RequestEntries::<T>::get(product_id, request_id).ok_or(Error::<T>::RequestNotOpened)?;
		ensure!(entry.vault.chain_id != local_chain_id, Error::<T>::UnexpectedInboundLeg);
		Self::push_bridge_attempt(&mut entry.bridge_attempts, bridge_status, tx)?;
		RequestEntries::<T>::insert(product_id, request_id, entry);
		Ok(())
	}

	/// `RequestStep::RequestQueued` — the request's capital confirmed at the
	/// Hub Valuation Contract, Hub-vault or Spoke-vault alike. First point
	/// `adapter_chain_ids` is genuinely knowable.
	pub(crate) fn handle_request_queued(
		product_id: ProductId,
		request_id: RequestId,
		opening: Option<RequestOpening>,
		adapter_chain_ids: Option<BoundedVec<ChainId, ConstU32<MAX_MULTICHAIN_ADAPTERS>>>,
		bridge_status: Option<BridgeStatus>,
		tx: TxRecord<BlockNumberFor<T>>,
		local_chain_id: ChainId,
	) -> DispatchResult {
		ensure!(opening.is_none(), Error::<T>::UnexpectedRequestOpening);
		ensure!(bridge_status.is_none(), Error::<T>::UnexpectedBridgeStatus);
		// A `SingleChain` product's request is `Requested` alone — no
		// Valuation-Contract queue step (everything is colocated).
		Self::ensure_multichain_product(product_id)?;
		let mut entry =
			RequestEntries::<T>::get(product_id, request_id).ok_or(Error::<T>::RequestNotOpened)?;
		if entry.vault.chain_id != local_chain_id {
			// Spoke-vault — only reachable once the Inbound leg's own Bridge
			// phase has landed with an `Executed` attempt.
			ensure!(
				Self::bridge_succeeded(&entry.bridge_attempts),
				Error::<T>::RequestStepOutOfOrder
			);
		}
		ensure!(entry.queued_tx.is_none(), Error::<T>::RequestStepAlreadyRecorded);
		// The request's arrival at the Valuation Contract — this is where the
		// Adapter decision first becomes knowable, Hub-vault or Spoke-vault alike.
		let chains = adapter_chain_ids.ok_or(Error::<T>::RequestAdapterChainsRequired)?;
		Self::ensure_no_duplicate_chain(&chains)?;
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
		let mut merged = RequestAdapterChains::<T>::get(product_id, request_id).unwrap_or_default();
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
		Ok(())
	}

	/// `RequestStep::AdapterBridgeExecuted` — Adapter leg, Bridge phase, one
	/// remote Adapter chain at a time.
	pub(crate) fn handle_adapter_bridge_executed(
		product_id: ProductId,
		request_id: RequestId,
		opening: Option<RequestOpening>,
		adapter_chain_ids: Option<BoundedVec<ChainId, ConstU32<MAX_MULTICHAIN_ADAPTERS>>>,
		bridge_status: Option<BridgeStatus>,
		chain_id: ChainId,
		tx: TxRecord<BlockNumberFor<T>>,
	) -> DispatchResult {
		ensure!(opening.is_none(), Error::<T>::UnexpectedRequestOpening);
		ensure!(adapter_chain_ids.is_none(), Error::<T>::UnexpectedRequestAdapterChains);
		let bridge_status = bridge_status.ok_or(Error::<T>::BridgeStatusRequired)?;
		// A `SingleChain` product has no Adapter leg — its Adapters are
		// colocated with the vault.
		Self::ensure_multichain_product(product_id)?;
		Self::ensure_adapter_chain_declared(product_id, request_id, chain_id)?;
		let mut entry = RequestChainEntries::<T>::get((product_id, request_id, chain_id));
		Self::push_bridge_attempt(&mut entry.bridge_attempts, bridge_status, tx)?;
		RequestChainEntries::<T>::insert((product_id, request_id, chain_id), entry);
		Ok(())
	}

	/// `RequestStep::AdapterApplied` — Adapter leg, Hooks phase (or the
	/// self-fulfilling case with no Bridge phase at all — see
	/// `ensure_adapter_chain_declared`'s doc comment for why no
	/// `bridge_succeeded` precondition is checked here, unlike every other
	/// Bridge-then-Applied/Hooks pair in this pallet).
	pub(crate) fn handle_adapter_applied(
		product_id: ProductId,
		request_id: RequestId,
		opening: Option<RequestOpening>,
		adapter_chain_ids: Option<BoundedVec<ChainId, ConstU32<MAX_MULTICHAIN_ADAPTERS>>>,
		bridge_status: Option<BridgeStatus>,
		chain_id: ChainId,
		tx: TxRecord<BlockNumberFor<T>>,
	) -> DispatchResult {
		ensure!(opening.is_none(), Error::<T>::UnexpectedRequestOpening);
		ensure!(adapter_chain_ids.is_none(), Error::<T>::UnexpectedRequestAdapterChains);
		ensure!(bridge_status.is_none(), Error::<T>::UnexpectedBridgeStatus);
		// A `SingleChain` product has no Adapter leg — see
		// `handle_adapter_bridge_executed`.
		Self::ensure_multichain_product(product_id)?;
		Self::ensure_adapter_chain_declared(product_id, request_id, chain_id)?;
		let mut entry = RequestChainEntries::<T>::get((product_id, request_id, chain_id));
		ensure!(entry.applied_tx.is_none(), Error::<T>::RequestStepAlreadyRecorded);
		entry.applied_tx = Some(tx);
		RequestChainEntries::<T>::insert((product_id, request_id, chain_id), entry);
		Ok(())
	}

	/// `RequestStep::Extended` — `FlowVersion`-routed escape hatch. The five
	/// core steps above are the only ones that ever give
	/// `opening`/`adapter_chain_ids`/`bridge_status` meaning; this step's own
	/// payload lives entirely in `extra`.
	pub(crate) fn handle_request_extended(
		product_id: ProductId,
		request_id: RequestId,
		opening: Option<RequestOpening>,
		adapter_chain_ids: Option<BoundedVec<ChainId, ConstU32<MAX_MULTICHAIN_ADAPTERS>>>,
		bridge_status: Option<BridgeStatus>,
		extra: Option<BoundedVec<u8, ConstU32<MAX_REQUEST_EXTRA_LEN>>>,
	) -> DispatchResult {
		ensure!(opening.is_none(), Error::<T>::UnexpectedRequestOpening);
		ensure!(adapter_chain_ids.is_none(), Error::<T>::UnexpectedRequestAdapterChains);
		ensure!(bridge_status.is_none(), Error::<T>::UnexpectedBridgeStatus);
		let flow_version =
			T::Products::request_flow_version(product_id).ok_or(Error::<T>::FlowVersionNotSet)?;
		let extra_bytes = extra.ok_or(Error::<T>::RequestExtraRequired)?;
		ensure!(
			RequestEntries::<T>::contains_key(product_id, request_id),
			Error::<T>::RequestNotOpened
		);
		match flow_version {
			// `V1`'s whole pipeline is the six core `RequestStep` values —
			// it never has anything to route through `Extended`.
			FlowVersion::V1 => Err(Error::<T>::WrongFlowVersion.into()),
			FlowVersion::V2 => {
				let request_extra = RequestExtraV2::decode(&mut &extra_bytes[..])
					.map_err(|_| Error::<T>::BadRequestExtra)?;
				// No sub-steps exist yet — see `RequestSubStepV2`'s doc
				// comment. Real arms go here, each fetching+mutating
				// `RequestEntries`, updating the specific
				// `RequestFlowExtensionV2` field(s) that sub-step owns,
				// then inserting the entry back — same pattern the six
				// core `RequestStep` arms above already use.
				match request_extra.step {}
			},
		}
	}

	// -----------------------------------------------------------------------
	// record_settlement_tx — one function per settlement-wide `SettlementStep`,
	// plus one shared function for all six chain-scoped leg steps
	// -----------------------------------------------------------------------

	/// `SettlementStep::SettleStarted` — declares `collect_response_chain_ids`/
	/// `finalize_chain_ids` and opens the settlement. A settlement needing no
	/// cross-chain action at all is recorded this way with both sets empty.
	/// Neither set may include the product's own local chain
	/// (`Error::LocalChainAsSpokeChain` otherwise) — that chain never gets a
	/// Spoke-chain leg of its own (see `SettlementCollectResponseChains`/
	/// `SettlementFinalizeChains`'s doc comments), so declaring it here would
	/// leave the settlement stuck at `SettleStarted` forever (no path to
	/// `SettleApplied`/its own `NavReceived` entry).
	/// Returns the `close_active_requests` scan size if `try_close_local_requests`
	/// ran the cascade (empty chain sets — vacuously complete right away), `0`
	/// otherwise — see `close_active_requests`'s own doc comment.
	pub(crate) fn handle_settle_started(
		product_id: ProductId,
		settlement_id: SettlementId,
		spoke_chain_id: Option<ChainId>,
		collect_response_chain_ids: Option<BoundedVec<ChainId, ConstU32<MAX_MULTICHAIN_ADAPTERS>>>,
		finalize_chain_ids: Option<BoundedVec<ChainId, ConstU32<MAX_TRANCHE_CHAINS>>>,
		request_ids: &Option<BoundedVec<RequestId, ConstU32<MAX_SETTLEMENT_REQUESTS>>>,
		bridge_status: Option<BridgeStatus>,
		tx: TxRecord<BlockNumberFor<T>>,
	) -> Result<Option<u32>, DispatchError> {
		ensure!(bridge_status.is_none(), Error::<T>::UnexpectedBridgeStatus);
		ensure!(spoke_chain_id.is_none(), Error::<T>::UnexpectedSpokeChainId);
		ensure!(request_ids.is_none(), Error::<T>::UnexpectedRequestIds);
		// The `belong_to_product` checks below are vacuously satisfied when both
		// chain sets are empty, and nothing else here reads product state — so
		// without this, `SettleStarted` with empty chain sets would happily open
		// a settlement (and orphan `SettlementTriggers`/chain-set entries) for a
		// `product_id` that was never registered. Every other `record_*` path is
		// gated on an existing product transitively; this step is the exception.
		ensure!(T::Products::is_registered(product_id), Error::<T>::ProductNotRegistered);
		let collect_response_chains =
			collect_response_chain_ids.ok_or(Error::<T>::SpokeChainIdsRequired)?;
		let finalize_chains = finalize_chain_ids.ok_or(Error::<T>::SpokeChainIdsRequired)?;
		// Each set must be dup-free on its own (a chain in both sets is fine —
		// it needs both a Collect/Response and a Finalize leg).
		Self::ensure_no_duplicate_chain(&collect_response_chains)?;
		Self::ensure_no_duplicate_chain(&finalize_chains)?;
		let local_chain_id = Self::local_chain_id(product_id);
		ensure!(
			!collect_response_chains.contains(&local_chain_id)
				&& !finalize_chains.contains(&local_chain_id),
			Error::<T>::LocalChainAsSpokeChain
		);
		ensure!(
			T::Adapters::adapter_chains_belong_to_product(product_id, &collect_response_chains),
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
		SettlementTriggers::<T>::insert(product_id, settlement_id, tx.clone());
		SettlementCollectResponseChains::<T>::insert(
			product_id,
			settlement_id,
			collect_response_chains,
		);
		SettlementFinalizeChains::<T>::insert(product_id, settlement_id, finalize_chains);

		// Closes every request colocated with this product's own local chain
		// approved into this settlement if `collect_response_chains` is already
		// fully responded — vacuously true right away when it's empty (no Adapter
		// anywhere off the local chain, or a fully local settlement — always the
		// case for a `SingleChain` product), same as a leg-by-leg `NavReceived`
		// reaching this state later would.
		Ok(Self::try_close_local_requests(product_id, settlement_id, tx))
	}

	/// `SettlementStep::Settled` — the one case this step is valid as
	/// extrinsic input, only for a `SingleChain` product (see that variant's
	/// doc comment). Same effect as `SettleStarted` with both chain sets
	/// empty — chain sets aren't parameters here since they're always empty
	/// for this case, so after this function's own two `Settled`-specific
	/// checks (below), it delegates the rest entirely to
	/// `handle_settle_started` with `Some(BoundedVec::default())` for both —
	/// every one of that function's own checks (bridge_status/spoke_chain_id/
	/// request_ids all `None`, no double-trigger, local-chain-as-spoke-chain,
	/// adapter/vault chain ownership) is either identical to what `Settled`
	/// itself requires, or vacuously satisfied by an empty chain set, so
	/// there's nothing left here for this function to re-check by hand. Both
	/// `SettlementCollectResponseChains`/`SettlementFinalizeChains` still end
	/// up written explicitly (not left absent) via that same shared path —
	/// `get_request`'s own `settled` computation (precompile-side) treats a
	/// *missing* entry as "not settled", not vacuously empty, unlike
	/// `try_close_local_requests`/`get_settlement`'s `unwrap_or_default`
	/// reads — so an absent entry here would leave a SingleChain request
	/// permanently reporting `settled == false` despite this settlement
	/// having completed.
	pub(crate) fn handle_settled(
		product_id: ProductId,
		settlement_id: SettlementId,
		spoke_chain_id: Option<ChainId>,
		collect_response_chain_ids: &Option<BoundedVec<ChainId, ConstU32<MAX_MULTICHAIN_ADAPTERS>>>,
		finalize_chain_ids: &Option<BoundedVec<ChainId, ConstU32<MAX_TRANCHE_CHAINS>>>,
		request_ids: &Option<BoundedVec<RequestId, ConstU32<MAX_SETTLEMENT_REQUESTS>>>,
		bridge_status: Option<BridgeStatus>,
		tx: TxRecord<BlockNumberFor<T>>,
	) -> Result<Option<u32>, DispatchError> {
		ensure!(
			collect_response_chain_ids.is_none() && finalize_chain_ids.is_none(),
			Error::<T>::UnexpectedSpokeChainIds
		);
		ensure!(
			T::Products::single_chain_id(product_id).is_some(),
			Error::<T>::SettledStepNotSingleChain
		);
		Self::handle_settle_started(
			product_id,
			settlement_id,
			spoke_chain_id,
			Some(BoundedVec::default()),
			Some(BoundedVec::default()),
			request_ids,
			bridge_status,
			tx,
		)
	}

	/// `SettlementStep::RequestsApproved` — records evidence for every
	/// `request_id` Valuation approved into this settlement, in one batch per
	/// call. For a `Multichain` product this is now typically called *multiple
	/// times* per settlement — once per Finalize-triggering `continueFinalize`
	/// tx, each batch scoped to just that chunk's requests (2026-09-17 contract
	/// response, §4) — not once for the whole settlement right after
	/// `NavReceived` as originally assumed. Already safe to call repeatedly:
	/// each `request_id` is individually guarded by `approved_tx.is_none()`
	/// below, and `SettlementRequests` accumulates additively across calls.
	/// Deliberately has no `SettlementTriggers` precondition (unlike every
	/// leg step) — a SingleChain SYNC product's Valuation Contract emits
	/// `DepositsApproved`/`RedeemsApproved` *before* `Settled`, so this step
	/// can genuinely land before `SettleStarted`/`Settled` for the same
	/// `settlement_id`.
	pub(crate) fn handle_requests_approved(
		product_id: ProductId,
		settlement_id: SettlementId,
		spoke_chain_id: Option<ChainId>,
		collect_response_chain_ids: &Option<BoundedVec<ChainId, ConstU32<MAX_MULTICHAIN_ADAPTERS>>>,
		finalize_chain_ids: &Option<BoundedVec<ChainId, ConstU32<MAX_TRANCHE_CHAINS>>>,
		request_ids: Option<BoundedVec<RequestId, ConstU32<MAX_SETTLEMENT_REQUESTS>>>,
		bridge_status: Option<BridgeStatus>,
		tx: TxRecord<BlockNumberFor<T>>,
	) -> DispatchResult {
		ensure!(bridge_status.is_none(), Error::<T>::UnexpectedBridgeStatus);
		ensure!(spoke_chain_id.is_none(), Error::<T>::UnexpectedSpokeChainId);
		ensure!(
			collect_response_chain_ids.is_none() && finalize_chain_ids.is_none(),
			Error::<T>::UnexpectedSpokeChainIds
		);
		let approved_request_ids = request_ids.ok_or(Error::<T>::RequestIdsRequired)?;
		ensure!(!approved_request_ids.is_empty(), Error::<T>::RequestIdsRequired);
		for request_id in approved_request_ids.iter() {
			let mut entry = RequestEntries::<T>::get(product_id, *request_id)
				.ok_or(Error::<T>::RequestNotOpened)?;
			ensure!(entry.approved_tx.is_none(), Error::<T>::RequestStepAlreadyRecorded);
			entry.settlement_id = Some(settlement_id);
			entry.approved_tx = Some(tx.clone());
			RequestEntries::<T>::insert(product_id, *request_id, entry);
		}
		SettlementRequests::<T>::try_mutate(product_id, settlement_id, |requests| {
			for request_id in approved_request_ids.iter() {
				requests
					.try_push(*request_id)
					.map_err(|_| Error::<T>::TooManySettlementRequests)?;
			}
			Ok::<(), Error<T>>(())
		})?;
		// Opportunistically self-close each request individually: this can race
		// with the settlement-side completion trigger
		// (`try_close_local_requests`/`close_active_requests`) — see
		// `SettlementStep::RequestsApproved`'s doc comment. `local_chain_id` and
		// `local_settlement_complete` are hoisted out of the loop: both are
		// invariant across the batch and each is expensive (`local_chain_id`
		// decodes the whole `ProductDetails`; `local_settlement_complete` scans
		// every collect/response chain), and this loop runs once per approved
		// `request_id`, up to `MAX_SETTLEMENT_REQUESTS`.
		let local_chain_id = Self::local_chain_id(product_id);
		let local_settlement_complete = Self::local_settlement_complete(product_id, settlement_id);
		for request_id in approved_request_ids.iter() {
			Self::try_close_request(
				product_id,
				settlement_id,
				*request_id,
				local_chain_id,
				local_settlement_complete,
				tx.clone(),
			);
		}
		Ok(())
	}

	/// `SettlementStep::Extended` — `FlowVersion`-routed escape hatch,
	/// settlement-wide or chain-scoped depending on `spoke_chain_id`
	/// (undeclared here since no real sub-step exists yet to route on it).
	pub(crate) fn handle_settlement_extended(
		product_id: ProductId,
		settlement_id: SettlementId,
		collect_response_chain_ids: &Option<BoundedVec<ChainId, ConstU32<MAX_MULTICHAIN_ADAPTERS>>>,
		finalize_chain_ids: &Option<BoundedVec<ChainId, ConstU32<MAX_TRANCHE_CHAINS>>>,
		request_ids: &Option<BoundedVec<RequestId, ConstU32<MAX_SETTLEMENT_REQUESTS>>>,
		bridge_status: Option<BridgeStatus>,
		extra: Option<BoundedVec<u8, ConstU32<MAX_SETTLEMENT_EXTRA_LEN>>>,
	) -> DispatchResult {
		ensure!(bridge_status.is_none(), Error::<T>::UnexpectedBridgeStatus);
		ensure!(
			collect_response_chain_ids.is_none() && finalize_chain_ids.is_none(),
			Error::<T>::UnexpectedSpokeChainIds
		);
		ensure!(request_ids.is_none(), Error::<T>::UnexpectedRequestIds);
		ensure!(
			SettlementTriggers::<T>::contains_key(product_id, settlement_id),
			Error::<T>::SettlementNotTriggered
		);
		let flow_version = T::Products::settlement_flow_version(product_id)
			.ok_or(Error::<T>::SettlementFlowVersionNotSet)?;
		let extra_bytes = extra.ok_or(Error::<T>::SettlementExtraRequired)?;
		match flow_version {
			// `V1`'s whole pipeline is the nine core `SettlementStep` values —
			// it never has anything to route through `Extended`.
			FlowVersion::V1 => Err(Error::<T>::WrongSettlementFlowVersion.into()),
			FlowVersion::V2 => {
				let settlement_extra = SettlementExtraV2::decode(&mut &extra_bytes[..])
					.map_err(|_| Error::<T>::BadSettlementExtra)?;
				// No sub-steps exist yet — see `SettlementSubStepV2`'s doc
				// comment. Real arms go here: match `spoke_chain_id` (as
				// every other step here already does) to decide whether
				// this sub-step updates `SettlementExtension`
				// (settlement-wide, `None`) or the named chain's own
				// `SettlementChainEntry::extension` (chain-scoped,
				// `Some(_)`), then fetch/mutate/insert the relevant
				// storage, same pattern the leg steps above use.
				match settlement_extra.step {}
			},
		}
	}

	/// Chunk-aware Bridge-phase leg arm of `handle_settlement_leg_step`
	/// (`CollectBridgeExecuted`/`ResponseBridgeExecuted`/`FinalizeBridgeExecuted`)
	/// — shared since all three differ only in which leg's own
	/// `chunk_count`/`chunks` pair this attempt is appended to. Fixes
	/// `chunk_count_slot` the first time any chunk of this leg is observed
	/// (Bridge or Hooks phase alike — see `record_hooks_leg_chunk`); every
	/// later chunk must agree (`Error::ChunkCountMismatch` otherwise). Opens a
	/// fresh `LegChunk` on first sight of `chunk_index`, inserted in
	/// `chunk_index`-ascending order (`Error::TooManyChunks` if that would
	/// exceed `MAX_CHUNKS`) — chunks can be observed out of order (no
	/// ordering guarantee between chunks, cross-chain or not), so insertion
	/// position can't just be "append". See
	/// `docs/tranche-tx-registry/settlement-leg-chunking-design.md` §6.3.
	fn record_bridge_leg_chunk(
		chunk_count_slot: &mut Option<u32>,
		chunks: &mut LegChunks<BlockNumberFor<T>>,
		chunk_index: u32,
		chunk_count: u32,
		bridge_status: Option<BridgeStatus>,
		tx: TxRecord<BlockNumberFor<T>>,
	) -> DispatchResult {
		ensure!(chunk_count >= 1 && chunk_index < chunk_count, Error::<T>::InvalidChunkIndex);
		let bridge_status = bridge_status.ok_or(Error::<T>::BridgeStatusRequired)?;
		match chunk_count_slot {
			None => *chunk_count_slot = Some(chunk_count),
			Some(n) if *n != chunk_count => return Err(Error::<T>::ChunkCountMismatch.into()),
			_ => {},
		}
		match chunks.iter_mut().find(|chunk| chunk.chunk_index == chunk_index) {
			Some(chunk) => Self::push_bridge_attempt(&mut chunk.bridge_attempts, bridge_status, tx),
			None => {
				let mut chunk = LegChunk { chunk_index, ..Default::default() };
				Self::push_bridge_attempt(&mut chunk.bridge_attempts, bridge_status, tx)?;
				let insert_at =
					chunks.iter().position(|c| c.chunk_index > chunk_index).unwrap_or(chunks.len());
				chunks.try_insert(insert_at, chunk).map_err(|_| Error::<T>::TooManyChunks)?;
				Ok(())
			},
		}
	}

	/// Chunk-aware Hooks-phase leg arm of `handle_settlement_leg_step`
	/// (`NavReported`/`NavReceived`/`SettleApplied`) — shared since all three
	/// differ only in which *preceding* leg's `chunk_count`/`chunks` this
	/// chunk's own Bridge phase must already have succeeded in, and which of
	/// `SettlementChainEntry`'s own `{leg}_chunks` fields this evidence lands
	/// in. Requires *that exact* `chunk_index` to already exist with a
	/// succeeded Bridge-phase attempt — narrows the pre-existing "Hooks needs
	/// Bridge to have landed first" principle (this pallet's original,
	/// leg-wide `record_hooks_leg_tx`) to per-chunk scope, so there's still no
	/// way for a Hooks step to self-fulfill a chunk its own Bridge phase never
	/// reached.
	fn record_hooks_leg_chunk(
		chunk_count_slot: &Option<u32>,
		chunks: &mut LegChunks<BlockNumberFor<T>>,
		chunk_index: u32,
		chunk_count: u32,
		bridge_status: Option<BridgeStatus>,
		tx: TxRecord<BlockNumberFor<T>>,
	) -> DispatchResult {
		ensure!(bridge_status.is_none(), Error::<T>::UnexpectedBridgeStatus);
		ensure!(chunk_count >= 1 && chunk_index < chunk_count, Error::<T>::InvalidChunkIndex);
		match chunk_count_slot {
			// This leg's Bridge phase has never landed a single chunk yet —
			// Hooks can't come first.
			None => return Err(Error::<T>::SettlementStepOutOfOrder.into()),
			Some(n) if *n != chunk_count => return Err(Error::<T>::ChunkCountMismatch.into()),
			_ => {},
		}
		let chunk = chunks
			.iter_mut()
			.find(|chunk| chunk.chunk_index == chunk_index)
			// `chunk_count_slot` is `Some` (some chunk of this leg has
			// started), but this exact `chunk_index` isn't in `chunks` yet —
			// this chunk's own Bridge phase hasn't landed (a different
			// chunk_index's may already have, out of order).
			.ok_or(Error::<T>::SettlementStepOutOfOrder)?;
		ensure!(
			Self::bridge_succeeded(&chunk.bridge_attempts),
			Error::<T>::SettlementStepOutOfOrder
		);
		ensure!(chunk.applied_tx.is_none(), Error::<T>::SettlementStepAlreadyRecorded);
		chunk.applied_tx = Some(tx);
		Ok(())
	}

	/// `true` iff a leg's `chunk_count` is fixed and every `chunk_index` in
	/// `0..chunk_count` is present with its Hooks-phase evidence landed.
	/// Checks presence by `chunk_index` value, not storage position — chunks
	/// can be observed (and therefore stored) out of order, so position `i`
	/// isn't guaranteed to hold `chunk_index == i`. Shared by
	/// `collect_leg_complete`/`response_leg_complete`/`finalize_leg_complete`.
	fn leg_complete(chunk_count: Option<u32>, chunks: &[LegChunk<BlockNumberFor<T>>]) -> bool {
		let Some(count) = chunk_count else { return false };
		if chunks.len() as u32 != count {
			return false;
		}
		(0..count).all(|chunk_index| {
			chunks
				.iter()
				.find(|chunk| chunk.chunk_index == chunk_index)
				.is_some_and(|chunk| chunk.applied_tx.is_some())
		})
	}

	/// This chain's Collect leg (`NavReported`) is fully complete — every
	/// chunk observed, every one with its Hooks-phase evidence landed.
	pub(crate) fn collect_leg_complete(entry: &SettlementChainEntry<BlockNumberFor<T>>) -> bool {
		Self::leg_complete(entry.collect_chunk_count, &entry.collect_chunks)
	}

	/// This chain's Response leg (`NavReceived`) is fully complete — replaces
	/// the old scalar `nav_received_tx.is_some()` check everywhere it was
	/// used (`local_settlement_complete`, and read-side `get_settlement`/
	/// `get_request`).
	pub(crate) fn response_leg_complete(entry: &SettlementChainEntry<BlockNumberFor<T>>) -> bool {
		Self::leg_complete(entry.response_chunk_count, &entry.response_chunks)
	}

	/// This chain's Finalize leg (`SettleApplied`) is fully complete —
	/// replaces the old scalar `settle_applied_tx.is_some()` check everywhere
	/// it was used (`try_close_request`, and read-side `get_settlement`/
	/// `get_request`).
	pub(crate) fn finalize_leg_complete(entry: &SettlementChainEntry<BlockNumberFor<T>>) -> bool {
		Self::leg_complete(entry.finalize_chunk_count, &entry.finalize_chunks)
	}

	/// The six chain-scoped `SettlementStep` leg steps
	/// (`CollectBridgeExecuted`/`NavReported`/`ResponseBridgeExecuted`/
	/// `NavReceived`/`FinalizeBridgeExecuted`/`SettleApplied`) — a
	/// Collect/Response/Finalize leg crossed with a Bridge/Hooks phase,
	/// handled together since they all share the same
	/// `spoke_chain_id`-scoped shape: resolve which chain-id set (`collect_response`
	/// vs `finalize`) the step belongs to, confirm `spoke_chain_id` is
	/// registered in it, then apply the one sub-step that actually differs.
	pub(crate) fn handle_settlement_leg_step(
		product_id: ProductId,
		settlement_id: SettlementId,
		spoke_chain_id: Option<ChainId>,
		collect_response_chain_ids: &Option<BoundedVec<ChainId, ConstU32<MAX_MULTICHAIN_ADAPTERS>>>,
		finalize_chain_ids: &Option<BoundedVec<ChainId, ConstU32<MAX_TRANCHE_CHAINS>>>,
		request_ids: &Option<BoundedVec<RequestId, ConstU32<MAX_SETTLEMENT_REQUESTS>>>,
		step: SettlementStep,
		bridge_status: Option<BridgeStatus>,
		chunk_index: u32,
		chunk_count: u32,
		tx: TxRecord<BlockNumberFor<T>>,
	) -> Result<Option<u32>, DispatchError> {
		ensure!(
			collect_response_chain_ids.is_none() && finalize_chain_ids.is_none(),
			Error::<T>::UnexpectedSpokeChainIds
		);
		// `request_ids` is now optionally meaningful for `SettleApplied` only
		// (§6.7 of the design doc — opportunistic per-chunk close) — every
		// other leg step still forbids it, same as before.
		if step != SettlementStep::SettleApplied {
			ensure!(request_ids.is_none(), Error::<T>::UnexpectedRequestIds);
		}
		let spoke_chain_id = spoke_chain_id.ok_or(Error::<T>::SpokeChainIdRequired)?;
		let is_finalize_step =
			matches!(step, SettlementStep::FinalizeBridgeExecuted | SettlementStep::SettleApplied);
		let chains = if is_finalize_step {
			SettlementFinalizeChains::<T>::get(product_id, settlement_id)
		} else {
			SettlementCollectResponseChains::<T>::get(product_id, settlement_id)
		}
		.ok_or(Error::<T>::SettlementNotTriggered)?;
		ensure!(chains.contains(&spoke_chain_id), Error::<T>::UnknownSpokeChain);

		// Finalize (both its Bridge and Hooks phases) can't start until the
		// *whole settlement's* Collect/Response legs are done — settlement-wide,
		// unlike every other gate here (including `collect_leg_complete`
		// below), which only ever reads this one chain's own `entry`. Added
		// 2026-09-16 per the contract team.
		//
		// 2026-09-17 correction: earlier contract-team guidance said all
		// finalize-target chains start together, in the same tx as the last
		// `NavReceived`. That's no longer how it works — Finalize is now
		// manually/keeper-triggered via separate `continueFinalize` txs (one
		// or more per settlement, chunk by chunk), decoupled from whichever tx
		// completed Response. This `ensure!` is, and always was, the pallet's
		// *entire* order enforcement for Finalize — a pure storage-state
		// predicate (`local_settlement_complete`, itself just "did every
		// declared `collect_response_chain_ids` chain finish its Response
		// leg") with no notion of "same tx" at all — there is no way for a
		// pallet extrinsic to check what other call landed in the same
		// transaction, so that was never actually enforced here regardless of
		// what the contract intended. Nothing needed adjusting on this side
		// once the trigger model changed. See
		// `docs/tranche-tx-registry/settlement-leg-chunking-design.md` §6.3
		// and `chunking-contract-requirements.md` §3/§4.
		if is_finalize_step {
			ensure!(
				Self::local_settlement_complete(product_id, settlement_id),
				Error::<T>::SettlementStepOutOfOrder
			);
		}

		// Cloned up front — `tx` itself is moved into whichever match arm below
		// records it as this chunk's own evidence, but the same attestation is
		// also this call's `settled_tx` attribution for any request closed
		// below (opportunistic per-request close, or the leg-completion
		// backstop) — see `close_one_active_request`'s doc comment.
		let close_tx = tx.clone();
		let mut entry =
			SettlementChainEntries::<T>::get((product_id, settlement_id, spoke_chain_id));
		match step {
			SettlementStep::CollectBridgeExecuted => Self::record_bridge_leg_chunk(
				&mut entry.collect_chunk_count,
				&mut entry.collect_chunks,
				chunk_index,
				chunk_count,
				bridge_status,
				tx,
			)?,
			SettlementStep::NavReported => Self::record_hooks_leg_chunk(
				&entry.collect_chunk_count,
				&mut entry.collect_chunks,
				chunk_index,
				chunk_count,
				bridge_status,
				tx,
			)?,
			SettlementStep::ResponseBridgeExecuted => {
				// Response's own payload (NAV + AdapterValuation[]) is Collect
				// leg's own output — no chunk of Response, Bridge or Hooks
				// phase alike, can start until this chain's Collect leg is
				// fully done (2026-09-14 — narrows the pre-existing
				// `NavReceived`-only gate below to also cover the Bridge
				// phase, since chunking makes "the leg's own start" a real
				// event now, not just its completion).
				ensure!(Self::collect_leg_complete(&entry), Error::<T>::SettlementStepOutOfOrder);
				Self::record_bridge_leg_chunk(
					&mut entry.response_chunk_count,
					&mut entry.response_chunks,
					chunk_index,
					chunk_count,
					bridge_status,
					tx,
				)?
			},
			SettlementStep::NavReceived => {
				// Same Collect-must-finish-first principle as
				// `ResponseBridgeExecuted` above — this replaces the old
				// scalar `entry.nav_reported_tx.is_some()` check.
				ensure!(Self::collect_leg_complete(&entry), Error::<T>::SettlementStepOutOfOrder);
				Self::record_hooks_leg_chunk(
					&entry.response_chunk_count,
					&mut entry.response_chunks,
					chunk_index,
					chunk_count,
					bridge_status,
					tx,
				)?
			},
			SettlementStep::FinalizeBridgeExecuted => Self::record_bridge_leg_chunk(
				&mut entry.finalize_chunk_count,
				&mut entry.finalize_chunks,
				chunk_index,
				chunk_count,
				bridge_status,
				tx,
			)?,
			SettlementStep::SettleApplied => Self::record_hooks_leg_chunk(
				&entry.finalize_chunk_count,
				&mut entry.finalize_chunks,
				chunk_index,
				chunk_count,
				bridge_status,
				tx,
			)?,
			SettlementStep::Queued
			| SettlementStep::SettleStarted
			| SettlementStep::RequestsApproved
			| SettlementStep::Settled
			| SettlementStep::Extended => {
				return Err(Error::<T>::InvalidSettlementStep.into());
			},
		}

		// Finalize's investor-request close is Strategy A (bulk, at leg
		// completion, see the design doc's Appendix) — fires exactly once,
		// the instant this call makes `finalize_leg_complete` true, reusing
		// this pallet's own `SettlementRequests` state rather than trusting
		// any externally supplied request list. Correct without needing a
		// separate "was it complete before this call" snapshot: the
		// `SettlementStepAlreadyRecorded` guard inside `record_hooks_leg_chunk`
		// above already guarantees this call's own chunk had `applied_tx ==
		// None` going in, so if `finalize_leg_complete` reads `true` *after*
		// this call, this chunk is the one that just completed it — it
		// couldn't have already been `true`.
		let just_completed_finalize =
			step == SettlementStep::SettleApplied && Self::finalize_leg_complete(&entry);

		SettlementChainEntries::<T>::insert((product_id, settlement_id, spoke_chain_id), entry);

		// Opportunistic per-request close (§6.7 of the design doc) — this
		// chunk's own `request_ids`, if any, name exactly which requests this
		// chunk's contract call actually paid out. Validated (Level 1
		// membership only — exists, belongs to this settlement, belongs to
		// this chain) before closing; the leg-completion backstop right below
		// still runs regardless, so completeness of this list was never
		// required (see the Appendix's amended comparison).
		if let Some(request_ids) = request_ids {
			for request_id in request_ids.iter() {
				let request_entry = RequestEntries::<T>::get(product_id, *request_id)
					.ok_or(Error::<T>::RequestNotOpened)?;
				ensure!(
					request_entry.settlement_id == Some(settlement_id)
						&& request_entry.vault.chain_id == spoke_chain_id,
					Error::<T>::RequestNotForChain
				);
				Self::close_one_active_request(
					product_id,
					*request_id,
					request_entry.investor,
					close_tx.clone(),
				);
			}
		}

		let cascade_scanned = if just_completed_finalize {
			Some(Self::close_active_requests(
				product_id,
				settlement_id,
				Some(spoke_chain_id),
				close_tx,
			))
		} else if step == SettlementStep::NavReceived {
			Self::try_close_local_requests(product_id, settlement_id, close_tx)
		} else {
			None
		};
		Ok(cascade_scanned)
	}

	// -----------------------------------------------------------------------
	// record_whitelist_tx — one function per `WhitelistStep`
	// -----------------------------------------------------------------------

	/// Bumps `LatestWhitelistNonce(who, vault)` to `nonce` if it's strictly
	/// newer than what's currently stored (or nothing is stored yet) —
	/// shared by `WhitelistRequested` and `WhitelistApplied`'s self-open
	/// case, the two steps that can open a fresh `WhitelistEntries` entry.
	fn bump_latest_whitelist_nonce(who: ChainAddress, vault: VaultId, nonce: WhitelistNonce) {
		let is_newer = match LatestWhitelistNonce::<T>::get(who, vault.clone()) {
			Some(latest) => nonce > latest,
			None => true,
		};
		if is_newer {
			LatestWhitelistNonce::<T>::insert(who, vault, nonce);
		}
	}

	/// `WhitelistStep::WhitelistRequested` — OrchestratorHub's trigger event,
	/// opens a fresh `WhitelistEntries` entry.
	pub(crate) fn handle_whitelist_requested(
		vault: VaultId,
		who: ChainAddress,
		grant: bool,
		nonce: WhitelistNonce,
		bridge_status: Option<BridgeStatus>,
		tx: TxRecord<BlockNumberFor<T>>,
	) -> Result<ProductId, DispatchError> {
		ensure!(bridge_status.is_none(), Error::<T>::UnexpectedBridgeStatus);
		let key = (who, vault.clone(), nonce);
		ensure!(
			!WhitelistEntries::<T>::contains_key(key.clone()),
			Error::<T>::WhitelistAlreadyTriggered
		);
		let product_id =
			T::Vaults::product_id_for_vault(&vault).ok_or(Error::<T>::VaultNotRegistered)?;
		// A `SingleChain` product has no Orchestrator-driven trigger — its
		// whitelist action is `WhitelistApplied` alone (self-opening — see
		// `handle_whitelist_applied`).
		Self::ensure_multichain_product(product_id)?;
		WhitelistEntries::<T>::insert(
			key,
			WhitelistEntry {
				product_id,
				vault: vault.clone(),
				who,
				grant,
				request_tx: Some(tx),
				bridge_attempts: Default::default(),
				applied_tx: None,
			},
		);
		Self::bump_latest_whitelist_nonce(who, vault, nonce);
		Ok(product_id)
	}

	/// `WhitelistStep::BridgeExecuted` — Hub-CCCP -> Spoke-CCCP, Spoke-vault
	/// actions only (reverts for a Hub-vault action — `Error::WhitelistNotTriggered`
	/// from the missing entry, or the chain-mismatch ensure below if one
	/// somehow exists).
	pub(crate) fn handle_whitelist_bridge_executed(
		vault: VaultId,
		who: ChainAddress,
		grant: bool,
		nonce: WhitelistNonce,
		bridge_status: Option<BridgeStatus>,
		tx: TxRecord<BlockNumberFor<T>>,
	) -> Result<ProductId, DispatchError> {
		let bridge_status = bridge_status.ok_or(Error::<T>::BridgeStatusRequired)?;
		let key = (who, vault, nonce);
		let mut entry =
			WhitelistEntries::<T>::get(key.clone()).ok_or(Error::<T>::WhitelistNotTriggered)?;
		ensure!(entry.grant == grant, Error::<T>::UnexpectedWhitelistGrant);
		ensure!(
			entry.vault.chain_id != Self::local_chain_id(entry.product_id),
			Error::<T>::UnexpectedWhitelistBridgeLeg
		);
		Self::push_bridge_attempt(&mut entry.bridge_attempts, bridge_status, tx)?;
		let product_id = entry.product_id;
		WhitelistEntries::<T>::insert(key, entry);
		Ok(product_id)
	}

	/// `WhitelistStep::WhitelistApplied` — MultichainTrancheManager's
	/// grant/revoke-applied event. Self-opens the entry (see the `None` arm)
	/// when a `SingleChain` product's TrancheManager applies the grant/revoke
	/// in one local step with no preceding `WhitelistRequested` — only valid
	/// if `vault` actually belongs to a registered `SingleChain` product; a
	/// `Multichain` product's vault reaching here via `WhitelistApplied` with
	/// no prior `WhitelistRequested` is a genuine ordering error, not a
	/// self-open case.
	pub(crate) fn handle_whitelist_applied(
		vault: VaultId,
		who: ChainAddress,
		grant: bool,
		nonce: WhitelistNonce,
		bridge_status: Option<BridgeStatus>,
		tx: TxRecord<BlockNumberFor<T>>,
	) -> Result<ProductId, DispatchError> {
		let key = (who, vault.clone(), nonce);
		match WhitelistEntries::<T>::get(key.clone()) {
			Some(mut entry) => {
				ensure!(bridge_status.is_none(), Error::<T>::UnexpectedBridgeStatus);
				ensure!(entry.grant == grant, Error::<T>::UnexpectedWhitelistGrant);
				if entry.vault.chain_id != Self::local_chain_id(entry.product_id) {
					ensure!(
						Self::bridge_succeeded(&entry.bridge_attempts),
						Error::<T>::WhitelistStepOutOfOrder
					);
				}
				ensure!(entry.applied_tx.is_none(), Error::<T>::WhitelistStepAlreadyRecorded);
				entry.applied_tx = Some(tx);
				let product_id = entry.product_id;
				WhitelistEntries::<T>::insert(key, entry);
				Ok(product_id)
			},
			None => {
				ensure!(bridge_status.is_none(), Error::<T>::UnexpectedBridgeStatus);
				let product_id = T::Vaults::product_id_for_vault(&vault)
					.ok_or(Error::<T>::VaultNotRegistered)?;
				ensure!(
					T::Products::single_chain_id(product_id).is_some(),
					Error::<T>::WhitelistNotTriggered
				);
				WhitelistEntries::<T>::insert(
					key,
					WhitelistEntry {
						product_id,
						vault: vault.clone(),
						who,
						grant,
						request_tx: None,
						bridge_attempts: Default::default(),
						applied_tx: Some(tx),
					},
				);
				Self::bump_latest_whitelist_nonce(who, vault, nonce);
				Ok(product_id)
			},
		}
	}

	// ---------------------------------------------------------------------
	// investor request / receive history (paged — see `bp_tranche::history`)
	// ---------------------------------------------------------------------

	/// Append one `request_id` to `(investor, product_id)`'s paged request
	/// history (`RequestStep::Requested`).
	pub fn push_request_history(
		investor: ChainAddress,
		product_id: ProductId,
		request_id: RequestId,
	) {
		history::history_push::<RequestHistoryIndex<T>>((investor, product_id), request_id);
	}

	/// Read a page of `(investor, product_id)`'s request history,
	/// most-recent-first: up to `limit` entries after skipping the newest
	/// `offset`, plus the full history length. `offset >= total` ⇒ empty.
	pub fn read_request_history(
		investor: ChainAddress,
		product_id: ProductId,
		offset: u32,
		limit: u32,
	) -> (Vec<RequestId>, u32) {
		history::history_read::<RequestHistoryIndex<T>>((investor, product_id), offset, limit)
	}

	/// Append one `(vault, tx_hash)` to `(investor, product_id)`'s paged receive
	/// history (`record_receive_tx`).
	pub fn push_receive_history(
		investor: ChainAddress,
		product_id: ProductId,
		entry: (VaultId, TxHash),
	) {
		history::history_push::<ReceiveHistoryIndex<T>>((investor, product_id), entry);
	}

	/// Read a page of `(investor, product_id)`'s receive history,
	/// most-recent-first. Same contract as [`Self::read_request_history`].
	pub fn read_receive_history(
		investor: ChainAddress,
		product_id: ProductId,
		offset: u32,
		limit: u32,
	) -> (Vec<(VaultId, TxHash)>, u32) {
		history::history_read::<ReceiveHistoryIndex<T>>((investor, product_id), offset, limit)
	}
}

/// Wires `InvestorRequestHistoryLen` / `InvestorRequestHistoryPage` onto the
/// shared paged-history logic in [`bp_tranche::history`].
pub struct RequestHistoryIndex<T>(PhantomData<T>);

impl<T: Config> PagedInvestorHistory for RequestHistoryIndex<T> {
	type Key = (ChainAddress, ProductId);
	type Entry = RequestId;

	fn len((investor, product_id): Self::Key) -> u32 {
		InvestorRequestHistoryLen::<T>::get(investor, product_id)
	}

	fn set_len((investor, product_id): Self::Key, len: u32) {
		InvestorRequestHistoryLen::<T>::insert(investor, product_id, len);
	}

	fn page((investor, product_id): Self::Key, page: u32) -> HistoryPage<Self::Entry> {
		InvestorRequestHistoryPage::<T>::get((investor, product_id, page))
	}

	fn append_to_page((investor, product_id): Self::Key, page: u32, entry: Self::Entry) {
		InvestorRequestHistoryPage::<T>::mutate((investor, product_id, page), |entries| {
			// Caller guarantees `page` is the tail page and not full.
			let _ = entries.try_push(entry);
		});
	}
}

/// Wires `InvestorReceiveHistoryLen` / `InvestorReceiveHistoryPage` onto the
/// shared paged-history logic in [`bp_tranche::history`].
pub struct ReceiveHistoryIndex<T>(PhantomData<T>);

impl<T: Config> PagedInvestorHistory for ReceiveHistoryIndex<T> {
	type Key = (ChainAddress, ProductId);
	type Entry = (VaultId, TxHash);

	fn len((investor, product_id): Self::Key) -> u32 {
		InvestorReceiveHistoryLen::<T>::get(investor, product_id)
	}

	fn set_len((investor, product_id): Self::Key, len: u32) {
		InvestorReceiveHistoryLen::<T>::insert(investor, product_id, len);
	}

	fn page((investor, product_id): Self::Key, page: u32) -> HistoryPage<Self::Entry> {
		InvestorReceiveHistoryPage::<T>::get((investor, product_id, page))
	}

	fn append_to_page((investor, product_id): Self::Key, page: u32, entry: Self::Entry) {
		InvestorReceiveHistoryPage::<T>::mutate((investor, product_id, page), |entries| {
			// Caller guarantees `page` is the tail page and not full.
			let _ = entries.try_push(entry);
		});
	}
}
