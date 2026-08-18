use crate::{ChainId, ProductId, RequestId, SettlementId};
use pallet_tranche_system::AdapterInspect;

use super::pallet::*;
use frame_support::{ensure, pallet_prelude::DispatchResult, traits::Get};
use sp_core::H160;

// Private, non-extrinsic helpers — kept in their own `impl` block, separate from
// `#[pallet::call]`, so they don't become part of the `Call` enum.
impl<T: Config> Pallet<T> {
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
	pub(crate) fn ensure_adapter_chain_declared(
		product_id: ProductId,
		request_id: RequestId,
		chain_id: ChainId,
	) -> DispatchResult {
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

	/// Removes `(product_id, request_id)` from `investor`'s
	/// `InvestorActiveRequests` list and emits `ActiveRequestClosed`, if it's
	/// still there — a no-op otherwise (already closed by an earlier call).
	/// Shared by `close_active_requests` (settlement-wide, chain-filtered) and
	/// `try_close_request` (single request, opportunistic) so the removal logic
	/// only lives in one place.
	fn close_one_active_request(product_id: ProductId, request_id: RequestId, investor: H160) {
		let mut requests = InvestorActiveRequests::<T>::get(investor);
		let Some(pos) = requests.iter().position(|r| *r == (product_id, request_id)) else {
			return;
		};
		requests.swap_remove(pos);
		InvestorActiveRequests::<T>::insert(investor, requests);
		Self::deposit_event(Event::ActiveRequestClosed { product_id, request_id, investor });
	}

	/// Shared by `record_settlement_tx`'s `SettleApplied` leg arm (called
	/// with `chain_id = Some(spoke_chain_id)`) and `try_close_hub_vault_requests`
	/// (called with `chain_id = Some(hub_chain_id)`): closes out
	/// `InvestorActiveRequests` for every request approved into `settlement_id`
	/// whose own origin chain is `chain_id` — see `InvestorActiveRequests`'s
	/// storage doc comment for the full mechanism. `chain_id = None` closes every
	/// request approved into `settlement_id` unconditionally; no current caller
	/// uses this (every real chain, including Hub, is always known at the call
	/// site), kept only because it costs nothing to leave the filter optional.
	/// Reads `SettlementRequests` — this pallet's own event-sourced copy of the
	/// request<->settlement linkage, written by `record_request_tx`'s
	/// `RequestStep::SettlementApproved` arm (see that storage's own doc comment
	/// for why it's no longer queried cross-pallet from
	/// pallet-tranche-investments).
	pub(crate) fn close_active_requests(
		product_id: ProductId,
		settlement_id: SettlementId,
		chain_id: Option<ChainId>,
	) {
		for request_id in SettlementRequests::<T>::get(product_id, settlement_id) {
			let Some(request_entry) = RequestEntries::<T>::get(product_id, request_id) else {
				continue;
			};
			if let Some(chain_id) = chain_id {
				if request_entry.vault.chain_id != chain_id {
					continue;
				}
			}
			Self::close_one_active_request(product_id, request_id, request_entry.investor);
		}
	}

	/// A Hub-vault request's settlement completes once every one of
	/// `SettlementCollectResponseChains` has reached `NavReceived` —
	/// NAV must be fully known before the Hub vault's own payout/allocation can
	/// be computed, which then happens synchronously with no Finalize leg of its
	/// own (see `SettlementStep`'s doc comment). Vacuously true the moment
	/// `collect_response_chain_ids` is declared empty at Trigger (no Adapter
	/// anywhere off-Hub), so this is safe to call unconditionally right after
	/// writing that storage, as well as after every `NavReceived`
	/// — closes `InvestorActiveRequests` for every Hub-vault request approved
	/// into `settlement_id` once true, via `close_active_requests(chain_id =
	/// Some(hub_chain_id))`. A no-op (does nothing, cheaply) if the condition
	/// isn't met yet.
	pub(crate) fn try_close_hub_vault_requests(product_id: ProductId, settlement_id: SettlementId) {
		if Self::hub_vault_settlement_complete(product_id, settlement_id) {
			let hub_chain_id = <T as pallet_evm::Config>::ChainId::get();
			Self::close_active_requests(product_id, settlement_id, Some(hub_chain_id));
		}
	}

	/// The condition `try_close_hub_vault_requests` waits on, factored out so
	/// `try_close_request` can check it for a single Hub-vault request without
	/// re-running `close_active_requests`' settlement-wide scan.
	fn hub_vault_settlement_complete(product_id: ProductId, settlement_id: SettlementId) -> bool {
		let collect_response_chains =
			SettlementCollectResponseChains::<T>::get(product_id, settlement_id)
				.unwrap_or_default();
		collect_response_chains.iter().all(|chain_id| {
			SettlementChainEntries::<T>::get((product_id, settlement_id, *chain_id))
				.nav_received_tx
				.is_some()
		})
	}

	/// Called right after `record_request_tx`'s `RequestStep::SettlementApproved`
	/// arm links `request_id` into `SettlementRequests` — closes just this one
	/// request out of `InvestorActiveRequests` immediately if its settlement's
	/// completion condition has *already* landed by the time `SettlementApproved`
	/// is recorded. This exists because `SettlementApproved` races with the
	/// settlement-side completion trigger (`try_close_hub_vault_requests`/
	/// `close_active_requests` at `SettleApplied`) — both can fire at essentially
	/// the same moment (see `RequestStep::SettlementApproved`'s doc comment), via
	/// separate, independently-ordered `record_request_tx`/`record_settlement_tx`
	/// calls. If the settlement-side trigger already ran before this request was
	/// linked into `SettlementRequests`, nothing would otherwise ever re-close
	/// it — the settlement-side trigger only iterates whatever was in
	/// `SettlementRequests` *at the time it ran*, and doesn't re-fire once its
	/// own condition has already been satisfied. A no-op if the condition isn't
	/// met yet — the settlement-side trigger will close this request once it is
	/// (this request is now linked into `SettlementRequests`, so it'll be found).
	pub(crate) fn try_close_request(
		product_id: ProductId,
		settlement_id: SettlementId,
		request_id: RequestId,
	) {
		let Some(request_entry) = RequestEntries::<T>::get(product_id, request_id) else {
			return;
		};
		let hub_chain_id = <T as pallet_evm::Config>::ChainId::get();
		let complete = if request_entry.vault.chain_id == hub_chain_id {
			Self::hub_vault_settlement_complete(product_id, settlement_id)
		} else {
			SettlementChainEntries::<T>::get((
				product_id,
				settlement_id,
				request_entry.vault.chain_id,
			))
			.settle_applied_tx
			.is_some()
		};
		if complete {
			Self::close_one_active_request(product_id, request_id, request_entry.investor);
		}
	}
}
