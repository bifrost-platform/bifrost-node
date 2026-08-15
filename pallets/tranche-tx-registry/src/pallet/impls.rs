use crate::{ChainId, ProductId, RequestId, SettlementId};
use pallet_tranche_system::{AdapterInspect, RequestSettlementInspect};

use super::pallet::*;
use frame_support::{ensure, pallet_prelude::DispatchResult, traits::Get};

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

	/// Shared by `record_settlement_tx`'s `SettleApplied` leg arm (called
	/// with `chain_id = Some(spoke_chain_id)`) and `try_close_hub_vault_requests`
	/// (called with `chain_id = Some(hub_chain_id)`): closes out
	/// `InvestorActiveRequests` for every request approved into `settlement_id`
	/// whose own origin chain is `chain_id` — see `InvestorActiveRequests`'s
	/// storage doc comment for the full mechanism. `chain_id = None` closes every
	/// request approved into `settlement_id` unconditionally; no current caller
	/// uses this (every real chain, including Hub, is always known at the call
	/// site), kept only because it costs nothing to leave the filter optional.
	pub(crate) fn close_active_requests(
		product_id: ProductId,
		settlement_id: SettlementId,
		chain_id: Option<ChainId>,
	) {
		for request_id in T::Investments::settlement_requests(product_id, settlement_id) {
			let Some(request_entry) = RequestEntries::<T>::get(product_id, request_id) else {
				continue;
			};
			if let Some(chain_id) = chain_id {
				if request_entry.vault.chain_id != chain_id {
					continue;
				}
			}
			let mut requests = InvestorActiveRequests::<T>::get(request_entry.investor);
			let Some(pos) = requests.iter().position(|r| *r == (product_id, request_id)) else {
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
		let collect_response_chains =
			SettlementCollectResponseChains::<T>::get(product_id, settlement_id)
				.unwrap_or_default();
		let all_responded = collect_response_chains.iter().all(|chain_id| {
			SettlementChainEntries::<T>::get((product_id, settlement_id, *chain_id))
				.nav_received_tx
				.is_some()
		});
		if all_responded {
			let hub_chain_id = <T as pallet_evm::Config>::ChainId::get();
			Self::close_active_requests(product_id, settlement_id, Some(hub_chain_id));
		}
	}
}
