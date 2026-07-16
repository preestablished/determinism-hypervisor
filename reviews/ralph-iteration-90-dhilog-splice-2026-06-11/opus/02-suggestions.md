# Suggestions

### S1. `is_empty()` hardcoded `false` — acceptable, but a debug assert documents the invariant better

**File:** `crates/dh-inputlog/src/splice.rs:126-128`

```rust
pub fn is_empty(&self) -> bool {
    false // Lineage::new rejects empty — kept for clippy's len/is_empty pairing
}
```

This is the clippy `len_without_is_empty` pairing hack. It is **acceptable** — the invariant
(a `Lineage` is never empty, both constructors reject zero segments) is real and the comment
states it. Two lighter alternatives, in order of preference:

- Make it derive from state so it can never drift if a future constructor path is added:
  `self.segments.is_empty()` (always `false` today, but self-checking). Costs nothing.
- Or `debug_assert!(!self.segments.is_empty()); false` to assert the invariant in tests.

Not worth blocking on; `self.segments.is_empty()` is the cleanest.

---

### S2. `edges()` re-parses each segment — fine at cw2 scale, but worth a one-line note on cost

**File:** `crates/dh-inputlog/src/splice.rs:147-156`

```rust
log: LogReader::parse(bytes).expect("validated at construction"),
```

Each `edges()` call re-runs the full `LogReader::parse` (header + BLAKE3 body_hash over the
whole segment body + record framing walk) per edge. It is validated at construction, so the
`expect` cannot fire; but the cost is paid again every time `edges()` is iterated.

For cw2 (per-child lineage = root + 1 child, iterated once to drive VerifyReplay) this is
negligible — at most 2 re-parses per child. **Judgment: fine as-is.** Only flag it if a
future caller iterates `edges()` repeatedly on a long lineage; then cache the `LogReader`
(or store `LogReader<'a>` in `segments` instead of `Header`, since `LogReader` derives
`Clone` and already owns the `Header`). Storing the `LogReader` would also let `edges()`
become a pure `.clone()` with no `expect`, removing the only `expect` in the public path.

---

### S3. Module is silent on `entropy_seed` across a lineage — confirm this is intended (it is, per the spec)

**File:** `crates/dh-inputlog/src/splice.rs` (doc-comment, lines 11-26)

The bead title mentions "seq/icount/watermark continuity rules"; the review brief asks
whether a mid-lineage segment with a **nonzero** `entropy_seed` (a PRNG re-seed) is a lineage
smell the module should reject.

**Finding: correctly left unconstrained.** API.md §3.1 (line 514) defines `entropy_seed` as
"ChaCha20 seed for the segment (zeros ⇒ continue base snapshot's PRNG stream)", and §3.4(1)
makes seeding a per-segment replay input. A nonzero seed mid-lineage is a **legitimate
recording choice** (re-seed at a fork point), not a continuity violation — and cw2 explicitly
runs "DISTINCT seeded" children, so per-child re-seeding is the *expected* shape. The PRNG
stream is captured in the base snapshot's state, so the content-addressed stitch
(`end_snapshot_id == base_snapshot_id`) already guarantees state continuity regardless of the
seed field. No rule needed. **Suggestion:** add one sentence to the doc-comment's continuity
list making this explicit ("`entropy_seed` is a per-segment replay input, not a lineage
invariant — a re-seed is a legal fork choice"), so a future reader doesn't mistake the
silence for an omission.

---

### S4. `encoder_fingerprint` continuity — also correctly unconstrained; worth the same one-liner

**File:** `crates/dh-inputlog/src/splice.rs` (doc-comment)

Per `reader.rs:100-103` and API.md §3.1 (line 523), `encoder_fingerprint` is compared
**per segment** by verifiers ("Verifiers compare fingerprints before SDK_EVENT digests"),
and zero means "no SDK digests in this segment". Different segments from different encoder
versions are therefore fine — each segment's VerifyReplay checks its own fingerprint against
its own digests. The module is right to not enforce cross-segment fingerprint equality.
Same as S3: a half-sentence in the doc-comment would pre-empt the "should this be checked?"
question. (Lower priority than S3 since the fingerprint is more obviously per-segment.)
