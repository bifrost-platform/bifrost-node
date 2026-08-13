use crate::{ChainId, ProductId, SettlementId};
use pallet_tranche_system::RequestSettlementInspect;

use super::pallet::*;
use frame_support::traits::Get;

// Private, non-extrinsic helpers — kept in their own `impl` block, separate from
// `#[pallet::call]`, so they don't become part of the `Call` enum.
impl<T: Config> Pallet<T> {
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
