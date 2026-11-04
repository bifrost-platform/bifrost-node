//! Paged storage for an investor's append-only, never-pruned history lists —
//! `pallet_tranche_custom_flows::InvestorFlowHistory` and
//! `pallet_tranche_tx_registry::{InvestorRequestHistory, InvestorReceiveHistory}`.
//!
//! Each of those was originally one unbounded `Vec` per key: every append
//! decoded and re-encoded the whole list (O(n) in the list length, unmetered),
//! and the read-side precompile getters decoded the whole list to return a
//! single page. This module splits the list across many bounded storage values
//! ("pages") so an append only ever touches the tail page and a read only
//! touches the pages it actually returns.
//!
//! A pallet supplies a zero-sized [`PagedInvestorHistory`] impl wiring the four
//! accessors to its own `Key -> u32` length header and `(Key.., page) ->
//! HistoryPage<Entry>` page map; the append / read logic lives here once
//! ([`history_push`] / [`history_read`]).

use sp_core::ConstU32;
use sp_runtime::BoundedVec;
use sp_std::vec::Vec;

/// Entries per stored history page, and also the upper bound the history-reading
/// precompile getters (`get_investor_flow_history` /
/// `get_investor_request_history` / `get_investor_receive_history`) accept for
/// `limit` — a read of up to this many entries touches at most two stored pages,
/// and an append only ever decodes / re-encodes one.
pub const HISTORY_PAGE_SIZE: u32 = 128;

/// One page of an investor's append-only history list. Bounded, so the page map
/// has a `MaxEncodedLen` and needs no `#[pallet::unbounded]`.
pub type HistoryPage<Entry> = BoundedVec<Entry, ConstU32<HISTORY_PAGE_SIZE>>;

/// Glue a pallet's length-header + page-map storage onto the shared
/// paged-append / paged-read logic. Implemented by a zero-sized type per history
/// storage; all methods are static (they key straight into the pallet's
/// storage). `Key` is the list identity — `(H160, ProductId)` for the
/// tx-registry histories, `(H160, ProductId, FlowId)` for custom-flows (whose
/// getter is always flow-scoped).
pub trait PagedInvestorHistory {
	/// Identifies one logical list (everything but the page index).
	type Key: Copy;
	/// One history entry (`RequestId`, `InstanceKey`, `(VaultId, H256)`, …).
	type Entry: Clone;

	/// The list's logical length — total entries ever appended.
	fn len(key: Self::Key) -> u32;

	/// Overwrite the logical length.
	fn set_len(key: Self::Key, len: u32);

	/// Read one page (an absent page is empty).
	fn page(key: Self::Key, page: u32) -> HistoryPage<Self::Entry>;

	/// Append `entry` to `page`. The caller guarantees `page` is the tail page
	/// and is not full, so the underlying `try_push` cannot fail.
	fn append_to_page(key: Self::Key, page: u32, entry: Self::Entry);
}

/// Append one entry to the tail page — O(1) in the list length (touches one
/// page + the length header, regardless of how long the list already is).
pub fn history_push<H: PagedInvestorHistory>(key: H::Key, entry: H::Entry) {
	let len = H::len(key);
	// Page `len / SIZE` currently holds `len % SIZE` entries — always `< SIZE`,
	// so it is the tail page and it is not full.
	H::append_to_page(key, len / HISTORY_PAGE_SIZE, entry);
	H::set_len(key, len.saturating_add(1));
}

/// Read up to `limit` entries most-recent-first, skipping the newest `offset`.
/// Returns `(entries, total)` where `total` is the full list length; `offset >=
/// total` yields an empty `entries`. With `limit <= HISTORY_PAGE_SIZE` this
/// touches at most two page reads.
pub fn history_read<H: PagedInvestorHistory>(
	key: H::Key,
	offset: u32,
	limit: u32,
) -> (Vec<H::Entry>, u32) {
	let total = H::len(key);
	if offset >= total {
		return (Vec::new(), total);
	}
	let take = limit.min(total - offset);
	if take == 0 {
		return (Vec::new(), total);
	}

	// Logical index window to emit, newest-first: `hi` down to `lo` inclusive.
	// `offset < total` ⇒ `hi < total`; `take <= total - offset` ⇒ `lo >= 0`.
	let hi = total - 1 - offset;
	let lo = hi + 1 - take;

	let mut out = Vec::with_capacity(take as usize);
	for page_idx in (lo / HISTORY_PAGE_SIZE..=hi / HISTORY_PAGE_SIZE).rev() {
		let page = H::page(key, page_idx);
		let page_len = page.len() as u32;
		if page_len == 0 {
			continue;
		}
		let base = page_idx * HISTORY_PAGE_SIZE;
		// Intersect the emit window with the logical indices this page holds.
		let from = core::cmp::max(lo, base);
		let upto = core::cmp::min(hi, base + page_len - 1);
		if from > upto {
			continue;
		}
		for logical in (from..=upto).rev() {
			out.push(page[(logical - base) as usize].clone());
		}
	}
	(out, total)
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::{cell::RefCell, collections::BTreeMap};

	thread_local! {
		static LEN: RefCell<BTreeMap<u8, u32>> = RefCell::new(BTreeMap::new());
		static PAGES: RefCell<BTreeMap<(u8, u32), Vec<u32>>> = RefCell::new(BTreeMap::new());
	}

	/// In-memory `PagedInvestorHistory` for exercising the pagination maths
	/// without pallet-mock machinery. `Key` is a list id; `Entry` is `u32` and
	/// every test pushes `0, 1, 2, …` so a value equals its own logical index.
	struct Mock;

	impl PagedInvestorHistory for Mock {
		type Key = u8;
		type Entry = u32;

		fn len(key: u8) -> u32 {
			LEN.with(|l| l.borrow().get(&key).copied().unwrap_or(0))
		}
		fn set_len(key: u8, len: u32) {
			LEN.with(|l| {
				l.borrow_mut().insert(key, len);
			});
		}
		fn page(key: u8, page: u32) -> HistoryPage<u32> {
			PAGES.with(|p| {
				let raw = p.borrow().get(&(key, page)).cloned().unwrap_or_default();
				assert!(raw.len() as u32 <= HISTORY_PAGE_SIZE, "page over-filled");
				HistoryPage::truncate_from(raw)
			})
		}
		fn append_to_page(key: u8, page: u32, entry: u32) {
			PAGES.with(|p| {
				let mut map = p.borrow_mut();
				let page_vec = map.entry((key, page)).or_default();
				assert!((page_vec.len() as u32) < HISTORY_PAGE_SIZE, "append to full page");
				page_vec.push(entry);
			});
		}
	}

	fn fill(key: u8, n: u32) {
		for i in 0..n {
			history_push::<Mock>(key, i);
		}
	}

	fn read(key: u8, offset: u32, limit: u32) -> (Vec<u32>, u32) {
		history_read::<Mock>(key, offset, limit)
	}

	#[test]
	fn read_within_one_page_is_newest_first() {
		let key = 1;
		fill(key, 5);
		assert_eq!(read(key, 0, 10), (vec![4, 3, 2, 1, 0], 5));
	}

	#[test]
	fn read_honours_offset_and_limit() {
		let key = 2;
		fill(key, 10);
		assert_eq!(read(key, 2, 3), (vec![7, 6, 5], 10));
	}

	#[test]
	fn read_offset_at_last_entry() {
		let key = 3;
		fill(key, 5);
		assert_eq!(read(key, 4, 10), (vec![0], 5));
	}

	#[test]
	fn read_offset_ge_total_is_empty() {
		let key = 4;
		fill(key, 3);
		assert_eq!(read(key, 3, 10), (Vec::new(), 3));
		assert_eq!(read(key, 99, 10), (Vec::new(), 3));
	}

	#[test]
	fn read_empty_list() {
		assert_eq!(read(5, 0, 10), (Vec::new(), 0));
	}

	#[test]
	fn read_zero_limit_is_empty_but_reports_total() {
		let key = 6;
		fill(key, 5);
		assert_eq!(read(key, 0, 0), (Vec::new(), 5));
	}

	#[test]
	fn push_splits_at_page_boundary() {
		let key = 7;
		let n = HISTORY_PAGE_SIZE + 5;
		fill(key, n);
		assert_eq!(Mock::len(key), n);
		assert_eq!(Mock::page(key, 0).len() as u32, HISTORY_PAGE_SIZE);
		assert_eq!(Mock::page(key, 1).len() as u32, 5);
		// newest 3
		assert_eq!(read(key, 0, 3).0, vec![n - 1, n - 2, n - 3]);
	}

	#[test]
	fn read_spans_two_pages() {
		let key = 8;
		let n = HISTORY_PAGE_SIZE + 2; // 130: page 0 = [0,127], page 1 = [128,129]
		fill(key, n);
		// offset 3 (skip 129,128,127), limit 50 → 126 down to 77, crossing the
		// page-0/page-1 boundary at 128.
		let (got, total) = read(key, 3, 50);
		assert_eq!(total, n);
		assert_eq!(got.len(), 50);
		assert_eq!(got.first().copied(), Some(126));
		assert_eq!(got.last().copied(), Some(77));
		// strictly descending, contiguous
		assert!(got.windows(2).all(|w| w[0] == w[1] + 1));
	}

	#[test]
	fn read_window_entirely_in_older_page() {
		let key = 9;
		let n = HISTORY_PAGE_SIZE + 10;
		fill(key, n);
		// skip the whole tail page + a bit → window sits only in page 0
		let (got, total) = read(key, 15, 4);
		assert_eq!(total, n);
		assert_eq!(got, vec![n - 16, n - 17, n - 18, n - 19]);
	}
}
