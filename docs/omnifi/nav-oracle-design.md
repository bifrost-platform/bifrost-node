# NAV Oracle Design Notes

Working notes from the `pallet-rwa-nav-oracle` design discussion on the `omnifi-revamp` branch
(2026-07-24). Nothing here is implemented yet — this captures the reasoning and open questions so
the eventual pallet/precompile design has the context behind it, not just the conclusions.

## Starting point

Under the OmniFi tranche-system pivot, `pallet-rwa-nav-oracle` no longer computes NAV on-chain.
The old `pallet-pools` design derived `oracle_nav = total_borrowed ± pnl - repaid_earnings` in its
`on_initialize` hook; that computation is gone. The new model:

> The pallet trusts whatever value a registered `OracleFeeder` submits, as-is. All data
> retrieval and verification happens in an oracle layer outside the node.

The open design question was: what does that off-chain oracle layer actually look like, and what
(if anything) should the on-chain pallet do beyond blindly storing a number?

## Reference: Chronicle Protocol's Proof of Asset

Chronicle Protocol runs a production oracle product for exactly this problem (NAV/reserve/holdings
attestation for tokenized RWAs — M0, Centrifuge's Anemoy fund, Superstate's USTB, BlackRock's
BUIDL, among others). Docs:
[chronicleprotocol/documentation, docs/Products/VerifiedAssetOracle](https://github.com/chronicleprotocol/documentation/tree/main/docs/Products/VerifiedAssetOracle).

### Pipeline

Three off-chain roles, only the last of which ever touches a chain:

1. **Agent** — pulls data from the source (API preferred, but also email/SFTP/etc.), computes/
   transforms it, packages "original + computed data," signs it, and publishes it to IPFS.
2. **Validator** — retrieves the payload from IPFS, verifies the input signature, independently
   re-verifies the computed output, and participates in a Schnorr-aggregated multi-signature.
   Chronicle calls this "Proof of Reputation" — each validator (Gnosis, Infura, Nethermind, etc.)
   stakes its own name on the attestation.
3. **Relay** — submits the verified, signed payload on-chain to `uScribe`, a gas-optimized
   contract built to publish *arbitrary structured data*, not just prices.

### Tamper-evidence mechanism

- **IPFS CID** is a content hash: the address is derived from the content itself, so any edit to
  the document produces a completely different address. Same input → same CID, always; different
  input → different CID, always (this is IPFS's own default SHA-256-based addressing, not
  something Chronicle adds).
- **On-chain checksum**: a `keccak256` hash of the document (chosen specifically because EVM has a
  native, cheap opcode for keccak256 — unlike SHA-256, which exists only as a precompile).
  Published as a URL param (`?checksum=0x...`) and checked on-chain via a verifier contract.
- Anyone — not just Chronicle — can download the IPFS document, recompute the hash themselves, and
  compare it to the published checksum. No one has to trust Chronicle's word for the data's
  integrity, only for its *initial sourcing*.

**Worked example** (real values, computed during this discussion — not Chronicle's own, just
illustrating the mechanism with SHA-256 for both steps rather than switching to keccak256):

| | Original | One digit changed (`10.42` → `10.52`) |
|---|---|---|
| content | `{"fund":"USTB","nav_per_share":10.42,"date":"2026-07-24"}` | `{"fund":"USTB","nav_per_share":10.52,"date":"2026-07-24"}` |
| SHA-256 | `65ceb6ed0e062d5874ea43eb701f323b6e6ee13fa63db8e8b0655060d90f8bb6` | `921f1d9d1dab4c333bd71a53b644dcaf2971d46b98934ae7d57f476ef79e78c1` |
| IPFS CIDv1 | `bafybeiaqnmezvtaj2vbytinjgmvlb2kni5tnazigrykynvmwgci4jnztwe` | `bafybeifjmxmk5duq27rpakjmm32bfwyi26cd4mmafnz2qnvrufx3p5maby` |

Both CIDs share the `bafybei` prefix — that's just CID format metadata (version + hash algorithm
tag), not derived from content. The actual fingerprint (everything after it) diverges immediately.

A rendered version of this walkthrough, plus the pipeline diagram, is published as a Claude
artifact: <https://claude.ai/code/artifact/63cb017a-de89-4e02-8e05-270b3b759690>.

### IPFS notes (for anyone unfamiliar)

- IPFS has no central operator. It's a peer-to-peer protocol (Protocol Labs builds/maintains the
  reference implementation and drives the spec, but doesn't run "the network"). Anyone can run a
  node.
- Content-addressing (tamper-evidence) and content-*availability* are separate concerns. A CID
  stays meaningful forever, but nothing guarantees the bytes remain fetchable unless some node
  actively **pins** them. In practice this means either self-hosting a pinning node or using a
  pinning service (Pinata, web3.storage, Infura's IPFS product, etc.) — both are structurally
  equivalent for integrity (the CID scheme doesn't care who hosts it), they differ only in
  operational/availability trust.
- Retrieval works via: a public IPFS HTTP gateway (`https://ipfs.io/ipfs/<CID>`, simplest, no P2P
  stack needed), a self-run IPFS node (stronger independence from any one gateway operator, more
  infra), or IPFS's own libp2p pub/sub (real-time push, but requires running a node continuously).
- Computing a CID is a pure, local, offline operation (hashing) — distinct from actually
  publishing/pinning content so others can fetch it. `ipfs-only-hash`-style tools compute "what CID
  would this content get" without touching the network at all; that's what produced the table
  above.

### Chronicle's real production payload (decoded)

The user supplied an actual Chronicle-submitted IPFS document (a Superstate-style USTB-type fund
report) for inspection. Structure:

```jsonc
{
  "version": "1.0",
  "payload": {
    "timestamp": 1784842263,
    "input": {
      "content": "<age-encrypted blob, base64, ~3MB>",
      "proofs": [
        { "type": "ECDSA", "payload": { "signature": "0x..." } }
      ]
    },
    "output": {
      "oracle": "0x...",           // abi.encode(uint256 timestamp, uint256 price_per_share_wad)
      "dashboard": {
        "document_date": "2026-07-22",
        "price_per_share": "1.110401",
        "net_asset_value": "880184820.71",
        "outstanding_token_supply": "792672449.798016",
        "outstanding_shares": "792056393.269780",
        "outstanding_token_supply_breakdown": [
          { "chain": "ARB1", "supply": "2516.000000" },
          { "chain": "ETH", "supply": "783691015.923403" }
          // ... 9 chains total
        ],
        "portfolio": {
          "positions": [
            {
              "isin": "US912797TP29",
              "description": "TREASURY BILL 0",
              "units": 72300000,
              "maturity_date": "2026-07-23",
              "market_value": "72300000",
              "current_price": "100.000000",
              "yield_to_maturity": "0.000000"
            }
            // ... 15 laddered T-Bill positions total
          ]
        }
      }
    }
  }
}
```

Key findings from decoding it:

- **`input.content` is encrypted**, not plaintext. Decoding the base64 gives an `age` (the file
  encryption tool, age-encryption.org) header: `age-encryption.org/v1` / `-> mlkem768x25519 ...` —
  a post-quantum-hybrid (ML-KEM-768 + X25519) recipient. This is the mechanism behind Chronicle's
  documented "privacy considerations of regulated financial entities" line — the raw
  custodian-sourced document is genuinely encrypted before publication, while the *computed*
  output (`dashboard`) is published in the clear. This corrects an earlier assumption in this
  discussion that raw data was fully public.
- **`input.proofs` is an array**, currently holding one ECDSA signature — structurally already
  extensible to multiple signers/attestation types, not hard-coded to exactly one.
- **`output.oracle` decodes to exactly `(timestamp, price_per_share × 1e18)`** — verified by
  hand: `word1 = 1784842263` (matches `payload.timestamp` exactly), `word2 / 1e18 = 1.110401`
  (matches `dashboard.price_per_share` exactly). This is the *entire* on-chain payload — 64 bytes,
  two `uint256`s. Everything else (holdings, per-chain supply breakdown) exists only in the
  off-chain document, anchored by the checksum, never pushed on-chain directly.
- Internal consistency check: `net_asset_value / outstanding_shares` ≈ `880184820.71 /
  792056393.27` ≈ `1.1112`, matching `price_per_share` (`1.110401`) closely — the fields are
  mutually consistent, as expected of a real computed report.
- Portfolio holdings are laddered T-Bills (maturities from 2026-07-23 through 2026-11-17,
  yield-to-maturity rising with maturity length — consistent with a normal short-end yield curve).

## Design decisions for Bifrost

### 1. On-chain payload mirrors Chronicle's `output.oracle`

`pallet-rwa-nav-oracle`'s stored value should be the same shape as Chronicle's on-chain payload: a
compact `(timestamp, nav_wad)` pair, not the full report. The full report (whatever a Bifrost
equivalent of `dashboard` looks like) stays off-chain, anchored by a hash.

### 2. `attestation_hash` alongside `nav`

`update_nav`/the pallet's confirm-extrinsic should carry a hash of the full off-chain document (the
Chronicle-equivalent checksum), not just the bare NAV number. The chain never interprets it — it's
a commitment that lets anyone later fetch the referenced document and independently confirm the
on-chain number traces back to a real report, without the chain needing to understand IPFS, `age`
encryption, or anything else about the document's internals.

```solidity
function update_nav(uint256 product_id, uint256 nav, bytes32 attestation_hash) external;
```

### 3. The "independent source" principle (key finding, not yet fully resolved)

Trust strength comes from the number of **independent source channels** behind an attestation, not
from the number of parties re-checking a single channel. This splits cleanly by adapter type:

- **OnchainSource adapters** (Compound/Morpho/Aave-style): independent verification is trivial and
  already free — `totalAssets()` is a public view call anyone (including a relayer) can call
  directly and get the exact same answer. No oracle/attestation machinery is needed here at all;
  Chronicle-style attestation is irrelevant to this adapter type.
- **OffchainSource adapters** (real custodian, RWA loan book): genuine independent verification
  ("did this custodian really hold what's claimed") is only possible if the custodian exposes the
  same underlying facts through **multiple independent channels** to multiple independent
  verifying parties. If there is exactly one channel (one Agent, one API relationship), no amount
  of additional relayers/validators downstream of that single channel adds real independence —
  they're all re-checking the same upstream claim. What they *can* still do without a second
  channel is **arithmetic/consistency verification** (recompute `NAV = Assets - Liabilities` from
  the numbers already in the payload, confirm signatures are valid) — genuinely useful (catches
  computation errors, tampering in transit, malformed submissions) but weaker than Chronicle's real
  security property (catching the *custodian or Agent lying about the underlying facts*).

Whether a given product's OffchainSource data actually has multiple independent channels is a
business/legal question between Bifrost and the custodian, not something the protocol can create
by adding more on-chain machinery. **Open idea, not decided**: expose how many independent sources
backed a given attestation (e.g. via `input.proofs`' array length, mirroring Chronicle's own
extensible-array design) so consumers can judge trust level per-product themselves, rather than the
chain implying uniform security across all products.

### 4. Custodian-facing Agent portal + CCCP relayers as Validators

Concrete operational proposal:

- Bifrost operates an admin portal for institutional custodians (the "Agent" role). Custodians are
  whitelisted and register an EVM wallet address up front.
- Whitelisting maps directly onto the already-designed `OracleFeeder` role in
  `pallet-tranche-permissions` (`grant_permission(product_id, OracleFeeder, custodian_wallet)`) —
  no new permission concept needed.
- The portal handles the IPFS submission (and, per the Chronicle example, should encrypt the raw
  document — e.g. `age` with a post-quantum-hybrid recipient — before publishing, to actually
  address custodian data privacy, matching what Chronicle's real payload does).
- **CCCP relayers take the Validator role**, reusing the network's existing threshold-signature
  infrastructure (watch → ⅔+ sign → `Poll_Submit`, the same pattern already used for the
  deposit/approve/borrow flows) rather than recruiting/managing a separate validator set.

### 5. Two-phase flow: announce → confirm

Mirrors the `RequestedInvestment` → `ApprovedInvestment` pattern already used in
`pallet-tranche-investments`:

1. Custodian (`OracleFeeder`, whitelisted wallet) submits `submit_nav_report(product_id, cid,
   checksum)` from their registered wallet — **no claimed NAV number in this call**, deliberately.
   Including a "claimed" number here would create a second place a value could diverge from the
   actual document; instead the real NAV only ever exists as whatever relayers extract *from* the
   verified document.
2. This emits an on-chain announce event/writes to a pending-storage — relayers already watch
   on-chain events for other flows (Socket-event pattern), so this becomes one more event type they
   watch, not new infrastructure.
3. Relayers fetch the IPFS document (via gateway or self-run node), verify it (signature validity +
   whatever consistency/independent-source checking applies for that product), and if ⅔+ reach
   consensus, one of them submits `confirm_nav(product_id, nav, attestation_hash)`.
4. That confirm call should be gated by a precompile-checked custom Origin (mirroring
   `pallet-tranche-investments`'s `Origin::Valuation` — the precompile verifies the relayer
   threshold was actually met before dispatching, the pallet just trusts the origin once
   constructed), not a bare `ensure_signed`.
5. `pallet-rwa-nav-oracle` stores the confirmed `(nav, attestation_hash)` — this is the value other
   pallets/consumers read as "the" trusted NAV for that product.

## Open questions

- Exactly what CCCP relayers verify for OffchainSource NAV reports (signature/consistency only, vs.
  attempting genuine independent-source cross-checks where a custodian offers multiple channels) —
  not decided; likely varies per product based on what the custodian relationship actually offers.
- Whether to surface an explicit "independent source count" field on-chain, and if so, in what
  shape.
- Exact encryption scheme for the custodian portal's IPFS submissions (reuse `age` +
  ML-KEM-768/X25519 as observed in Chronicle's payload, or something else).
- Full storage/interface design for `pallet-rwa-nav-oracle` and its precompile — this document is
  the design reasoning that should inform that pass, not the pass itself.

## References

- Chronicle Protocol docs: <https://github.com/chronicleprotocol/documentation/tree/main/docs/Products/VerifiedAssetOracle>
  (`verifiedAssetOracle.md`, `proofOfAssets.md`, `data.md`, `glossary.md`, `vaoDashboard.md`) and
  <https://github.com/chronicleprotocol/documentation/blob/main/docs/Resources/FAQ/Vao.md>.
- Pipeline diagram + worked hashing example (Claude artifact):
  <https://claude.ai/code/artifact/63cb017a-de89-4e02-8e05-270b3b759690>.
- See also `docs/omnifi/` sibling notes (if present) and project memory for the broader
  OmniFi tranche-system pivot this fits into (`pallet-tranche-system`, `pallet-tranche-investments`,
  `pallet-tranche-permissions`).
