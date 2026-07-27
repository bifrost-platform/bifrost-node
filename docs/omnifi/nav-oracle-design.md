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

#### Fundamental limitation: a single source can forge `output` and no relayer vote catches it

Concretely: if there's exactly one custodian/source, and that custodian publishes a **genuine,
unmodified encrypted `input.content`** alongside a **fabricated `output`** (e.g. an inflated
`net_asset_value`), relayer verification as designed above cannot catch it — no matter how many
relayers vote:

- Checksum verification only proves the document matches what was originally published, not that
  what was published is true — the custodian forged it *before* publishing, so the forged version
  and the checksum agree perfectly.
- Signature verification only proves the registered custodian really authored the document —
  which is true even when they're lying, since it's their own forgery.
- The `output.oracle` vs. `output.dashboard` consistency check only proves the compact on-chain
  payload faithfully encodes the plaintext dashboard — it says nothing about whether the dashboard
  faithfully reflects the encrypted `input.content`, because relayers hold no decryption key for
  `input.content` (that's the whole point of encrypting it for custodian privacy) and structurally
  cannot compare output against input.

So every check available to a relayer only establishes **internal self-consistency of what the
single source itself produced** — none of them establish truthfulness against reality. Voting adds
nothing here: every relayer is confirming the same self-consistent forgery.

**This limitation applies equally to a lying/buggy Agent, not just a lying custodian — and it's
sharper than it first looks**, because the custodian's signature only ever covers `input.content`.
Nothing in the document is signed by the custodian *for* `output` — `output` is entirely the
Agent's own computation. So even a fully honest custodian, providing genuine raw data, doesn't
protect against an Agent (buggy or compromised) that produces a self-consistent but false
`output` — the checksum, the custodian's signature, and the `oracle`/`dashboard` consistency check
all pass, because none of them were ever checking the Agent's output against the real input in
the first place. Structurally, the Agent is single-sourced for the *input → output* transformation
the same way a lone custodian is single-sourced for the underlying facts.

Real mitigations, none of them a protocol trick — all require changing who has access to what:

1. **Split roles within the single source, via a narrow dedicated auditor role — NOT the general
   relayer set**: give decrypt access for `input.content` to a small, separately-vetted auditor
   role, so *someone* other than the Agent can compare output against input. This doesn't require
   a second external source, just separating who *provides* the data from who *computes/verifies*
   it, and holds as long as the custodian and that auditor aren't colluding.
   **Correction, reconsidered**: the baseline relayer verification described below (checksum,
   signature, `output.oracle`/`output.dashboard` consistency, nav extraction) needs **zero**
   decrypt access — every one of those checks operates on already-plaintext or already-ciphertext
   fields as-is. Only this deeper re-computation check needs plaintext, and it should **not** be
   extended to the whole relayer set: relayers are deliberately a broad, less centrally-vetted
   group (that's the point of using them for censorship-resistant relay), and handing that group
   decrypt access to sensitive institutional financial data would substantially erode the privacy
   `input.content` encryption exists to provide — likely well past what a regulated custodian
   actually expects when they're told their data is private. If this mitigation is used at all,
   it belongs on a small, separately accountable role (e.g. auditors under their own
   agreement/KYC with the custodian), opt-in per product, not a default extension of relayer
   permissions. It may also be reasonable to simply not pursue this mitigation for most products
   and accept arithmetic/consistency-only verification as the practical baseline.
2. **Cross-check individual fields against public data where it exists**: some fields have
   independent public reference data even with a single custodian — e.g. a T-Bill's market price/
   yield is public bond-market data, checkable regardless of what the custodian claims. This
   doesn't help with the field that's fundamentally impossible to verify this way: *how many units
   the custodian actually holds* — that fact exists only where the custodian says it does.
3. **Otherwise, this reduces to legal/audit/reputational trust, not cryptography** — the same limit
   Chronicle's own "Proof of *Reputation*" naming implicitly acknowledges. This isn't a gap
   specific to this design; it's the general boundary of connecting a single real-world fact to an
   on-chain value. No amount of on-chain verification machinery removes the need to trust the
   custodian isn't lying, unless an actual second independent channel (option 1 or a true second
   source) exists.
4. **Have the Agent sign `output` too, separately from the custodian's signature over `input`**:
   this doesn't newly enable anyone to verify `output` is *true* (same limit as option 3 — it's
   evidence, not cryptographic proof of correctness), but it closes the accountability gap
   identified above, where `output` currently carries no signature from anyone at all. With an
   Agent signature over `output`, a later-discovered bad `output` can be attributed unambiguously
   to the Agent specifically (as opposed to the custodian, who only ever attested to `input`) —
   turning "nobody signed this" into "the Agent is on the hook for this," which matters for the
   legal/reputational backstop in option 3.

#### So what is relayer verification actually for?

Given the above, relayer voting does not, and structurally cannot, verify that a single source's
claims are *true*. What it does provide:

1. **Transport integrity** — what reaches the chain is exactly what the custodian published,
   unaltered in transit.
2. **Authenticity / anti-impersonation** — the document really came from *this product's*
   registered custodian, not an attacker publishing arbitrary data and trying to pass it off as a
   legitimate NAV update. This is a real attack the relayer layer does block.
3. **Format/internal-consistency quality control** — catches accidental bugs and malformed
   submissions (not necessarily malicious fraud by the custodian itself).
4. **Availability** — multiple independent successful fetches of the same CID confirm the document
   is genuinely retrievable, not a dead reference.
5. **Decentralized, censorship-resistant relay** — no single relayer's downtime or misbehavior can
   block a legitimate update, since any sufficient subset can complete the relay.
6. **An unforgeable evidence trail** — a durable, cryptographically-anchored record of exactly what
   the custodian signed and when, so that if fraud is later discovered off-chain (audit, legal
   action), there's no ambiguity about what was claimed — this is what makes the "legal/reputational
   trust" backstop in point 3 above actually enforceable.

In short: relayer verification protects the **integrity of the pipeline** (transport, authenticity,
availability, censorship-resistance, evidence), not the **truth of the content**. Content
truthfulness rests on the custodian's signature, reputation, and legal exposure; the relayer layer's
job is making sure that signed claim reaches the chain intact and stays durably provable — not
independently confirming the claim is factually correct.

### 4. Custodian-facing Agent portal + CCCP relayers as Validators

Concrete operational proposal:

- Bifrost operates an admin portal for institutional custodians. Custodians are whitelisted and
  register an EVM wallet address up front.
- Whitelisting maps directly onto the already-designed `OracleFeeder` role in
  `pallet-tranche-permissions` (`grant_permission(product_id, OracleFeeder, custodian_wallet)`) —
  no new permission concept needed.
- **The custodian is not the Agent.** The custodian's job through the portal is to provide and
  sign raw data — not to compute NAV/holdings themselves. **This is Bifrost's own design
  judgment, not a confirmed Chronicle precedent** — an earlier draft of this document justified it
  by claiming Chronicle itself operates the Agent, inferred from phrasing like *"credibly-neutral
  attestation by fetching data directly from the custodian"*. That inference doesn't hold up:
  checking Chronicle's `adapters.md`/`routers.md` docs found no explicit statement of who runs the
  Agent, and the `ChronicleVAO_<Issuer>_<AssetTicker>_...` naming convention suggests Chronicle's
  actual counterparty might be the *issuing protocol* (e.g. Centrifuge, Superstate) rather than
  the custodian directly — a three-party structure (issuer ↔ Chronicle ↔ custodian) this document
  hadn't considered. The real justification for Bifrost operating the Agent stands on its own:
  the pallet that ultimately trusts `nav` is Bifrost's, whitelisting (`OracleFeeder`) is already
  Bifrost's responsibility, so keeping the computation layer auditable and under the same party's
  control keeps the trust boundary in one place — not because Chronicle is known to do the same.
- The actual **Agent role — parsing raw data into the computed `output` (NAV, price per share,
  holdings breakdown) — is Bifrost's own portal backend**, running auditable, re-runnable
  computation logic, not the custodian.
- This reading is also supported structurally by the real Chronicle example: `proofs` sits nested
  under `input`, not under `output` — consistent with the signature attesting to the *raw input's*
  authenticity (this really came from the custodian) while `output`'s trustworthiness comes from
  being a deterministic, re-computable function of that signed input, not from a signature of its
  own.
- **Only the Agent's own access is unavoidable — not relayers'.** Computing `output` at all
  requires plaintext access to the raw data, so *the Agent* necessarily has it (that's its job).
  This does **not** extend to the general relayer set: the baseline relayer verification (§5
  below) needs zero decrypt access — it only ever touches already-plaintext `output` fields and
  the ciphertext-as-opaque-bytes (for the checksum).
- **Decided flow**: the custodian encrypts client-side (to the Agent's `age` recipient key) before
  ever submitting to the portal — plaintext never touches Bifrost's systems except transiently
  inside the Agent's own decrypt-and-compute step. This is stronger than having the portal receive
  plaintext and encrypt it afterward (which would mean Bifrost's upload/storage path handles raw
  plaintext, a larger exposure window).
- **Signing order matters: encrypt-then-sign, not sign-then-encrypt.** For relayers to verify the
  custodian's signature without holding a decryption key (see §5 below), the signature must cover
  the *ciphertext* bytes actually published, not the plaintext underneath it — otherwise relayers
  would need to decrypt just to reconstruct what was signed, defeating the point of keeping
  baseline verification decrypt-free. The real Chronicle payload is consistent with this:
  `proofs` sits as a sibling field next to `content`, not embedded inside the encrypted blob,
  matching "sign the ciphertext" rather than "encrypt the (plaintext + signature) bundle."
  Signing the ciphertext still functions as attesting to the underlying content — only the
  custodian, who holds the plaintext, could have produced that specific ciphertext.
- **The custodian's raw document needs a defined, versioned schema** the Agent can parse
  automatically (assuming the Agent is software, not a human transcribing PDFs by hand) — e.g. a
  structured format for OffchainSource loan-book holdings (borrower, collateral value, outstanding
  principal, maturity, etc.), analogous to Chronicle's `portfolio.positions` shape for T-Bills but
  for whatever an RWA loan book's holdings actually are. Onboarding a new custodian means either
  they can natively export in this schema, or a mapping/transformation step exists before
  signing. A `version` field (Chronicle's real payload has one: `"version": "1.0"`) lets the Agent
  apply the right parsing/computation logic as the schema evolves, and the Agent should validate
  against the expected schema before computing — reject malformed submissions rather than
  silently producing bad output.
- Whoever can decrypt the *published* `input.content` later (i.e. holds a private key matching
  whatever `age` recipient(s) it was encrypted to) is a deliberate, separate choice from the
  Agent's own access — `age` supports multiple recipients on one ciphertext, so this *could*
  mechanically extend decrypt access to additional parties without a separate key-distribution
  system. But see the fundamental-limitation section's corrected mitigation-1 discussion above:
  that capability should go to a small, separately-vetted auditor role if used at all, opt-in per
  product — not be handed to the general relayer set, which would undercut the privacy the
  encryption exists to provide.
- **CCCP relayers take the Validator role**, reusing the network's existing threshold-signature
  infrastructure (watch → ⅔+ sign → `Poll_Submit`, the same pattern already used for the
  deposit/approve/borrow flows) rather than recruiting/managing a separate validator set.

### 5. Two-phase flow: announce → confirm

Mirrors the `RequestedInvestment` → `ApprovedInvestment` pattern already used in
`pallet-tranche-investments`:

1. The custodian submits encrypted, signed raw data through the portal; the Agent (Bifrost's
   backend) decrypts it, computes `output`, publishes the resulting document to IPFS, and submits
   `submit_nav_report(product_id, cid, checksum)` on-chain to announce it — **no claimed NAV
   number in this call**, deliberately. Including a "claimed" number here would create a second
   place a value could diverge from the actual document; instead the real NAV only ever exists as
   whatever relayers extract *from* the verified document. **Open question**: since the Agent (not
   the custodian directly) now performs the on-chain announce, should this call still come from the
   custodian's registered `OracleFeeder` wallet (the Agent submitting on the custodian's behalf,
   e.g. as a relayed/sponsored transaction), or from a separate Agent-operated wallet? The
   custodian's authenticity is already provable via the signature embedded in the document itself
   (verified by relayers in step 3.2), so the announce transaction's sender may not need to be the
   custodian's own wallet at all — not decided.
2. This emits an on-chain announce event/writes to a pending-storage — relayers already watch
   on-chain events for other flows (Socket-event pattern), so this becomes one more event type they
   watch, not new infrastructure.
3. Relayers fetch the IPFS document (via gateway or self-run node) and verify it, in order:
   1. **Checksum integrity** — rehash the fetched bytes locally, compare to the `checksum` from
      the announce event. Mismatch → reject (tampered document, wrong CID, or bad gateway
      response).
   2. **Signer validity** — recover the signer from `input.proofs[].signature` via `ecrecover`
      and confirm it matches the product's registered `OracleFeeder` wallet. The custodian signs
      `input.content` — the *ciphertext*, not the plaintext underneath it (encrypt-then-sign, not
      sign-then-encrypt; see §4 above for why this ordering is required for relayers to verify
      without a decryption key). Still open: whether the signing key is the same wallet as the
      on-chain `OracleFeeder` registration, or a separate registered signing key. **Note this
      signature only covers `input`, not `output`** — see the fundamental-limitation section's
      note on Agent accountability above; if the Agent-signs-`output` mitigation is adopted, this
      step should also verify that second signature against the Agent's known key.
   3. **Consistency check** — decode `output.oracle` and confirm it faithfully encodes what's in
      `output.dashboard` (this is exactly the `(timestamp, price_per_share × 1e18)` cross-check
      done by hand against the real Chronicle example above), plus whatever internal-consistency
      or independent-source checks apply for that product (see the independent-source principle
      above).
   4. **Extract, don't compute, `nav`** — the relayer does not calculate NAV itself. It reads the
      already-verified `output.oracle` (or `dashboard.price_per_share`) value directly out of the
      document. Because every relayer who fetches the same CID and passes the same checks is
      reading the same deterministic document, they all extract the identical number — there is
      no independent judgment call to reconcile between relayers here, only a pass/fail on
      verification.
4. Because step 3.4 is deterministic, the "⅔+" threshold isn't reconciling differing values — it's
   proving enough independent relayers actually fetched and verified this specific CID. Mirrors
   the existing CCCP-v2 `Poll_Submit{msg, sigs}` pattern used elsewhere in the protocol: relayers
   each sign `msg = {product_id, nav, attestation_hash}` off-chain, signatures are collected until
   the threshold is met, and only then does one relayer submit
   `confirm_nav(product_id, nav, attestation_hash)`.
5. That confirm call should be gated by a precompile-checked custom Origin (mirroring
   `pallet-tranche-investments`'s `Origin::Valuation` — the precompile verifies the relayer
   threshold was actually met before dispatching, the pallet just trusts the origin once
   constructed), not a bare `ensure_signed`.
6. `pallet-rwa-nav-oracle` stores the confirmed `(nav, attestation_hash)` — this is the value other
   pallets/consumers read as "the" trusted NAV for that product.

## Open questions

- Exactly what CCCP relayers verify for OffchainSource NAV reports (signature/consistency only, vs.
  attempting genuine independent-source cross-checks where a custodian offers multiple channels) —
  not decided; likely varies per product based on what the custodian relationship actually offers.
- Whether to surface an explicit "independent source count" field on-chain, and if so, in what
  shape.
- Whether the custodian's signing key is the same wallet as the on-chain `OracleFeeder`
  registration, or a separately registered signing key. (The signed-message construction itself is
  now resolved: the custodian signs the ciphertext, i.e. `input.content` as published —
  encrypt-then-sign — so relayers can verify without a decryption key. See §4.)
- Whether the on-chain `submit_nav_report` announce call should be sent from the custodian's
  registered `OracleFeeder` wallet (Agent submitting on the custodian's behalf) or from a separate
  Agent-operated wallet, now that the Agent (not the custodian directly) performs the IPFS
  submission. See §5 step 1.
- Whether relayer confirmation should reuse the existing off-chain signature-collection +
  single-submission `Poll_Submit` pattern (as currently written in §5, consistent with the rest of
  the protocol), or an on-chain per-relayer voting mechanism — worth confirming explicitly since
  the latter would be a new pattern not used elsewhere in this protocol and costs proportionally
  more gas.
- Exact encryption scheme for the custodian portal's IPFS submissions (reuse `age` +
  ML-KEM-768/X25519 as observed in Chronicle's payload, or something else).
- The raw-data schema(s) the Agent parses (per source/asset type — e.g. OffchainSource loan-book
  holdings), including versioning and validation — not designed yet, see §4.
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
