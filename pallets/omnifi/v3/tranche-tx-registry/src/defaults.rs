//! Built-in default templates and the default flow maps every newly
//! registered product receives (seeded at genesis and by `migrations::v1`).
//!
//! Slot names follow the contract event the recorder observes for that step
//! (`…Bridged` for a CCCP bridge tx, `…Received` for a hook tx with no domain
//! event of its own). Slot ids are contiguous in typical chronological order;
//! changing a flow means a new template version, renumbered from scratch.
//! The defaults cover products whose transfer method is CCTP or none. A
//! product that moves assets over CCCP instead (`BRIDGE_CCCP`, e.g. one with a
//! spoke CCTP doesn't reach) is assigned the opt-in [`mc_settle_cccp`] for its
//! settlement flow. Step-by-step recorder guidance lives in
//! `docs/tranche-tx-registry-v3/default-templates.md`.

use crate::{
	slug, Config, DefaultFlows, FlowId, Pallet, ProductModel, SlotDef, SlotId, Template,
	TemplateId, Templates, TrackDef,
};
use frame_support::{traits::Get, weights::Weight, BoundedVec};
use sp_std::{vec, vec::Vec};

// ---- flow ids (product-scoped; bytes16, ASCII left-aligned, zero-padded) ----
pub const FLOW_DEPOSIT: FlowId = slug("deposit"); // 0x6465706f736974000000000000000000
pub const FLOW_REDEEM: FlowId = slug("redeem"); // 0x72656465656d00000000000000000000
pub const FLOW_SETTLEMENT: FlowId = slug("settlement"); // 0x736574746c656d656e74000000000000
pub const FLOW_WHITELIST: FlowId = slug("whitelist"); // 0x77686974656c69737400000000000000
pub const FLOW_RECEIVE: FlowId = slug("receive"); // 0x72656365697665000000000000000000

// ---- template ids (name is bytes16, same padding as flow ids) ----
pub const MC_DEPOSIT_V1: TemplateId = TemplateId { name: slug("mc-deposit"), version: 1 }; // 0x6d632d6465706f736974000000000000
pub const MC_REDEEM_V1: TemplateId = TemplateId { name: slug("mc-redeem"), version: 1 }; // 0x6d632d72656465656d00000000000000
pub const MC_DEPOSIT_V2: TemplateId = TemplateId { name: slug("mc-deposit"), version: 2 }; // 0x6d632d6465706f736974000000000000
pub const MC_REDEEM_V2: TemplateId = TemplateId { name: slug("mc-redeem"), version: 2 }; // 0x6d632d72656465656d00000000000000
pub const MC_SETTLEMENT_V1: TemplateId = TemplateId { name: slug("mc-settlement"), version: 1 }; // 0x6d632d736574746c656d656e74000000
/// Opt-in settlement template for `BRIDGE_CCCP` products — never a default flow.
pub const MC_SETTLE_CCCP_V1: TemplateId = TemplateId { name: slug("mc-settle-cccp"), version: 1 }; // 0x6d632d736574746c652d636363700000
pub const MC_WHITELIST_V1: TemplateId = TemplateId { name: slug("mc-whitelist"), version: 1 }; // 0x6d632d77686974656c69737400000000
pub const SC_DEPOSIT_V1: TemplateId = TemplateId { name: slug("sc-deposit"), version: 1 }; // 0x73632d6465706f736974000000000000
pub const SC_REDEEM_V1: TemplateId = TemplateId { name: slug("sc-redeem"), version: 1 }; // 0x73632d72656465656d00000000000000
pub const SC_SETTLEMENT_V1: TemplateId = TemplateId { name: slug("sc-settlement"), version: 1 }; // 0x73632d736574746c656d656e74000000
pub const SC_WHITELIST_V1: TemplateId = TemplateId { name: slug("sc-whitelist"), version: 1 }; // 0x73632d77686974656c69737400000000
pub const RECEIVE_V1: TemplateId = TemplateId { name: slug("receive"), version: 1 }; // 0x72656365697665000000000000000000

fn slot(id: SlotId, name: &str, required: bool) -> SlotDef {
	SlotDef { id, name: BoundedVec::truncate_from(name.as_bytes().to_vec()), required }
}

fn track(name: &str, slots: Vec<SlotDef>) -> TrackDef {
	TrackDef {
		name: BoundedVec::truncate_from(name.as_bytes().to_vec()),
		slots: BoundedVec::truncate_from(slots),
		required_count: 0,
	}
}

fn template(investor_scoped: bool, main: Vec<SlotDef>, tracks: Vec<TrackDef>) -> Template {
	Template {
		investor_scoped,
		main: BoundedVec::truncate_from(main),
		main_required_count: 0,
		tracks: BoundedVec::truncate_from(tracks),
	}
}

/// Multichain deposit. The vault spoke holds the cash until the first round
/// close after `DepositQueued` places every held deposit of that chain in one
/// ROUTE (`BatchRouted`, ref = settlement id) — that placement is recorded on
/// the settlement instance (`hold` lane, plus `hold-cctp` — or `hold-cccp` on
/// a `BRIDGE_CCCP` product's `mc-settle-cccp` — for legs to other chains) and
/// linked from slot 3.
/// No lanes: nothing after queueing is per request until approval.
pub fn mc_deposit() -> Template {
	template(
		true,
		vec![
			slot(0, "DepositRequested", true),
			slot(1, "DepositBridged", false),
			slot(2, "DepositQueued", true),
			slot(3, "BatchRouted", false),
			slot(4, "DepositsApproved", true),
			slot(5, "SettleApplied", true),
		],
		vec![],
	)
}

/// `mc-deposit` v2 = v1 + slot 6 `RequestClaimed`: the vault spoke Manager's
/// `RequestClaimed(requestId, …)` in the claim tx that minted this request's
/// shares (2026-10-19, mainnet deployments only). Optional — seed deposits are
/// never claimed, and the claim usually lands after the instance completed
/// (a closed instance still takes evidence). One claim tx carries one
/// `RequestClaimed` per request it claimed, so the same tx hash appears on
/// several instances.
pub fn mc_deposit_v2() -> Template {
	let mut t = mc_deposit();
	t.main.force_push(slot(6, "RequestClaimed", false));
	t
}

/// Multichain redeem. Queueing only records the request; the payout is pulled
/// by the round-close batch ROUTE (per chain, not per request — recorded on
/// the settlement instance's `pull` / `hold` lanes) and the request is
/// usually approved one settle later (sooner if its chain already holds
/// enough payout). Every round that defers it leaves a
/// `RedeemCarried` (chain short of payout) or `RedeemHeldForCoverage` (JR
/// coverage) attempt linked to that settlement.
pub fn mc_redeem() -> Template {
	template(
		true,
		vec![
			slot(0, "RedeemRequested", true),
			slot(1, "RedeemBridged", false),
			slot(2, "RedeemQueued", true),
			slot(3, "RedeemCarried", false),
			slot(4, "RedeemHeldForCoverage", false),
			slot(5, "RedeemsApproved", true),
			slot(6, "SettleApplied", true),
		],
		vec![],
	)
}

/// `mc-redeem` v2 = v1 + slot 7 `RequestClaimed`: the claim tx that paid out
/// this request's assets. Same rules as `mc_deposit_v2`'s slot 6.
pub fn mc_redeem_v2() -> Template {
	let mut t = mc_redeem();
	t.main.force_push(slot(7, "RequestClaimed", false));
	t
}

/// Multichain settlement. `collect-response` lanes = every spoke asked for a
/// NAV; `finalize` lanes = the chains of the FINALIZE plan (requests + fee
/// chain). The round-close tx sends up to three ROUTEs per chain — payout
/// move (`PayoutMoveOrdered.fromChain`), hold batch and adapter pull
/// (`BatchRouted.fromHold` / `.pulled` > 0) — each its own track with lane =
/// the chain that executes it. A hook tx that burns CCTP declares a `*-cctp`
/// lane for the same chain whose one slot carries a part per burn.
pub fn mc_settlement() -> Template {
	template(
		false,
		vec![
			slot(0, "SettleStarted", true),
			slot(1, "CollectAborted", false),
			slot(9, "Settled", true),
		],
		vec![
			track(
				"collect-response",
				vec![
					slot(2, "CollectBridged", false),
					slot(3, "DepositGateSet", false),
					slot(4, "NavReported", true),
					slot(5, "ResponseBridged", false),
					slot(6, "ResponseChunkApplied", false),
					slot(7, "NavReceived", true),
				],
			),
			track(
				"finalize",
				vec![
					slot(8, "FinalizeStarted", false),
					slot(10, "FinalizeBridged", false),
					slot(11, "SettleApplied", true),
				],
			),
			track(
				"payout-move",
				vec![slot(12, "PayoutRouteBridged", false), slot(13, "PayoutRouteReceived", true)],
			),
			track("payout-move-cctp", vec![slot(14, "PayoutCctpReceived", true)]),
			track(
				"hold",
				vec![slot(15, "HoldRouteBridged", false), slot(16, "HoldRouteReceived", true)],
			),
			track("hold-cctp", vec![slot(17, "HoldCctpReceived", true)]),
			track(
				"pull",
				vec![slot(18, "PullRouteBridged", false), slot(19, "WithdrawRequested", true)],
			),
		],
	)
}

/// Multichain settlement for a product whose transfer method is CCCP
/// (`BRIDGE_CCCP` — every spoke's product asset is the CCCP token). Same as
/// [`mc_settlement`] up to the ROUTE hook txs; a leg to another chain then
/// leaves the sending spoke as a CCCP BRIDGE message (`CccpBridgeSent`), the
/// hub Orchestrator relays it in its hook tx (`BridgeRelayed`) and the
/// destination spoke's hook tx lands it (`CccpBridgeReceived`). Each move is
/// two CCCP messages (sender → hub → destination), so a `*-cccp` lane — lane =
/// the SENDING chain, declared by a ROUTE hook tx that emits `CccpBridgeSent`
/// — carries a part per `CccpBridgeSent` on every slot. A leg below the hub
/// route's minimum stays put (`BridgeKeptLocal`) and is not a part.
pub fn mc_settle_cccp() -> Template {
	template(
		false,
		vec![
			slot(0, "SettleStarted", true),
			slot(1, "CollectAborted", false),
			slot(9, "Settled", true),
		],
		vec![
			track(
				"collect-response",
				vec![
					slot(2, "CollectBridged", false),
					slot(3, "DepositGateSet", false),
					slot(4, "NavReported", true),
					slot(5, "ResponseBridged", false),
					slot(6, "ResponseChunkApplied", false),
					slot(7, "NavReceived", true),
				],
			),
			track(
				"finalize",
				vec![
					slot(8, "FinalizeStarted", false),
					slot(10, "FinalizeBridged", false),
					slot(11, "SettleApplied", true),
				],
			),
			track(
				"payout-move",
				vec![slot(12, "PayoutRouteBridged", false), slot(13, "PayoutRouteReceived", true)],
			),
			track(
				"payout-move-cccp",
				vec![
					slot(14, "PayoutHubBridged", false),
					slot(15, "PayoutBridgeRelayed", false),
					slot(16, "PayoutDstBridged", false),
					slot(17, "PayoutCccpBridgeReceived", true),
				],
			),
			track(
				"hold",
				vec![slot(18, "HoldRouteBridged", false), slot(19, "HoldRouteReceived", true)],
			),
			track(
				"hold-cccp",
				vec![
					slot(20, "HoldHubBridged", false),
					slot(21, "HoldBridgeRelayed", false),
					slot(22, "HoldDstBridged", false),
					slot(23, "HoldCccpBridgeReceived", true),
				],
			),
			track(
				"pull",
				vec![slot(24, "PullRouteBridged", false), slot(25, "WithdrawRequested", true)],
			),
		],
	)
}

/// Multichain whitelist grant/revoke, scoped to the investor it targets
/// (`investor = who`, `scope = vault`). The message goes to the one chain the
/// vault lives on, so there are no lanes.
pub fn mc_whitelist() -> Template {
	template(
		true,
		vec![
			slot(0, "WhitelistRequested", true),
			slot(1, "WhitelistBridged", false),
			slot(2, "WhitelistApplied", true),
		],
		vec![],
	)
}

/// Single-chain deposit — Manager and Valuation live on the product's own
/// chain, no messaging. SYNC orders approve in the request tx (the approval
/// is logged before `DepositRequested`; record the opening event first),
/// ASYNC ones in a later `settle*` tx.
pub fn sc_deposit() -> Template {
	template(
		true,
		vec![slot(0, "DepositRequested", true), slot(1, "DepositsApproved", true)],
		vec![],
	)
}

/// Single-chain redeem. `RedeemInitiated` is the ASYNC first `settle` that
/// started adapter withdrawals still pending.
pub fn sc_redeem() -> Template {
	template(
		true,
		vec![
			slot(0, "RedeemRequested", true),
			slot(1, "RedeemInitiated", false),
			slot(2, "RedeemsApproved", true),
		],
		vec![],
	)
}

/// Single-chain settlement round — one `Settled` per round id; it opens and
/// completes the instance. Approvals are recorded on the request instances.
pub fn sc_settlement() -> Template {
	template(false, vec![slot(0, "Settled", true)], vec![])
}

pub fn sc_whitelist() -> Template {
	template(true, vec![slot(0, "WhitelistApplied", true)], vec![])
}

/// A claim — opens and completes in one attestation. The one slot covers both
/// `DepositReceived` and `RedeemReceived` (the kind goes in the instance
/// metadata), shared by both product models.
pub fn receive() -> Template {
	template(true, vec![slot(0, "Received", true)], vec![])
}

pub fn templates() -> Vec<(TemplateId, Template)> {
	vec![
		(MC_DEPOSIT_V1, mc_deposit()),
		(MC_REDEEM_V1, mc_redeem()),
		(MC_DEPOSIT_V2, mc_deposit_v2()),
		(MC_REDEEM_V2, mc_redeem_v2()),
		(MC_SETTLEMENT_V1, mc_settlement()),
		(MC_SETTLE_CCCP_V1, mc_settle_cccp()),
		(MC_WHITELIST_V1, mc_whitelist()),
		(SC_DEPOSIT_V1, sc_deposit()),
		(SC_REDEEM_V1, sc_redeem()),
		(SC_SETTLEMENT_V1, sc_settlement()),
		(SC_WHITELIST_V1, sc_whitelist()),
		(RECEIVE_V1, receive()),
	]
}

pub fn default_flows() -> Vec<(ProductModel, FlowId, TemplateId)> {
	vec![
		(ProductModel::Multichain, FLOW_DEPOSIT, MC_DEPOSIT_V2),
		(ProductModel::Multichain, FLOW_REDEEM, MC_REDEEM_V2),
		(ProductModel::Multichain, FLOW_SETTLEMENT, MC_SETTLEMENT_V1),
		(ProductModel::Multichain, FLOW_WHITELIST, MC_WHITELIST_V1),
		(ProductModel::Multichain, FLOW_RECEIVE, RECEIVE_V1),
		(ProductModel::SingleChain, FLOW_DEPOSIT, SC_DEPOSIT_V1),
		(ProductModel::SingleChain, FLOW_REDEEM, SC_REDEEM_V1),
		(ProductModel::SingleChain, FLOW_SETTLEMENT, SC_SETTLEMENT_V1),
		(ProductModel::SingleChain, FLOW_WHITELIST, SC_WHITELIST_V1),
		(ProductModel::SingleChain, FLOW_RECEIVE, RECEIVE_V1),
	]
}

/// Registers every built-in template that isn't registered yet, and fills in
/// every default flow not already set. Idempotent; never overwrites. Returns
/// the weight used and how many templates were newly registered.
pub fn seed<T: Config>() -> (Weight, u32) {
	let db = T::DbWeight::get();
	let (mut weight, registered) = register_templates::<T>();
	for (model, flow_id, template) in default_flows() {
		weight = weight.saturating_add(db.reads_writes(1, 1));
		DefaultFlows::<T>::mutate(model, |defaults| {
			if !defaults.contains_key(&flow_id) {
				let _ = defaults.try_insert(flow_id, template);
			}
		});
	}
	(weight, registered)
}

/// Registers every built-in template that isn't registered yet; leaves the
/// default flow maps alone. Returns the weight used and how many templates
/// were newly registered.
pub fn register_templates<T: Config>() -> (Weight, u32) {
	let db = T::DbWeight::get();
	let mut weight = Weight::zero();
	let mut registered = 0u32;
	for (id, template) in templates() {
		weight = weight.saturating_add(db.reads(1));
		if Templates::<T>::contains_key(id) {
			continue;
		}
		let template = Pallet::<T>::finalize_template(template)
			.expect("built-in default templates are well-formed; qed");
		Templates::<T>::insert(id, template);
		registered = registered.saturating_add(1);
		weight = weight.saturating_add(db.writes(1));
	}
	(weight, registered)
}
