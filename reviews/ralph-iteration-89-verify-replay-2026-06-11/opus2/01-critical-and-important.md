# Critical & Important Findings

## Critical

None.

---

## Important

### I-1 — Reconstructed EpochOk honesty: VERIFIED HONEST, but the count guarantee is implicit and worth a one-line assert-comment

**File:** `crates/dh-worker/src/verify_replay.rs:48-63`

The success path re-parses `log_bytes` and emits one `EpochOk` for every
`EpochHash` record in `log.aux()`. This is honest **only if** the replay
engine actually verified each of those records. I traced this end-to-end and
it holds:

1. `replay_engine` builds `expected_epochs` from the *same* `log.aux()`
   filter on `RecordBody::EpochHash` (replay_engine.rs:143-152).
2. The epoch sink increments `verified` once per matched link
   (replay_engine.rs:224) and **fails fast with `Divergence`** on the first
   mismatch or unexpected epoch (replay_engine.rs:205-222).
3. Before returning `Ok`, the engine pins `verified == expected_epochs.len()`
   (replay_engine.rs:335-343), returning `Divergence` otherwise.

Therefore on the `Ok` path: `outcome.epoch_hashes_verified ==
expected_epochs.len() == (count of EpochHash records in aux)`, which is
exactly what the wrapper re-counts. **The `debug_assert_eq!(emitted,
outcome.epoch_hashes_verified)` can never fire on the Ok path** — it is a
pin, not a check. That is fine, but it means the *real* guarantee lives in
the engine, not here.

**Hostile-log sub-question (records AFTER end_icount):** Can a parse-valid log
carry an `EpochHash` with `icount > end_icount` that the engine never reached
but the wrapper still emits as `EpochOk`? **No.** `validate_records`
(reader.rs:443) rejects any record whose icount regresses, and the END record
(always last, reader.rs) has `icount == header.end_icount`. An EpochHash with
`icount > end_icount` must precede END (END-not-last is rejected), so it would
make END's icount *regress* relative to it → `IcountRegressed`. The log never
parses. So the wrapper's re-parse cannot see an epoch the engine skipped.
**The reconstruction is sound.**

**Recommendation:** Keep the code; upgrade the `debug_assert_eq!` to a hard
`assert_eq!` OR add a one-line comment stating *why* it can never fire (the
engine's count-pin at replay_engine.rs:336 is the real guarantee). A
`debug_assert` that is structurally impossible to trip in release is a
documentation comment wearing a macro costume — make the intent explicit so a
future refactor of the engine's count-pin doesn't silently weaken this.

---

### I-2 — `Divergence` field mapping is nonsense for three of the four divergence shapes; the proto-mirroring claim overstates

**Files:** `crates/dh-worker/src/verify_replay.rs:70-83`,
`crates/dh-worker/src/replay_engine.rs:326-385`

The wrapper maps **every** `ReplayError::Divergence` uniformly:

```rust
first_bad_epoch: at_icount / machine_config.epoch_len.max(1),
at_icount, what, expected, got,
```

But the engine emits `Divergence` with **four structurally different
`(what, at_icount, expected, got)` shapes**, and the uniform mapping is only
correct for one of them:

| `what` (engine source)                          | `at_icount` actually is | `expected`/`got` actually are | `first_bad_epoch = at_icount/epoch_len` |
|-------------------------------------------------|-------------------------|-------------------------------|-----------------------------------------|
| `"EPOCH_HASH chain value"` (re:205)             | the epoch's icount      | 32-byte chain hashes          | **correct** ✓                           |
| `"end_vns"` (re:327)                            | `end_icount`            | **u64 packed into 32 bytes**  | misleading (= last epoch index)         |
| `"end_state_hash"` (re:346)                     | `end_icount`            | 32-byte hashes                | misleading (= last epoch index)         |
| `"EPOCH_HASH count ..."` (re:337)               | `end_icount`            | **all-zero** `[0;32]`         | misleading                              |
| `"resealed log bytes (at_icount = first differing byte offset)"` (re:380) | **a BYTE OFFSET, not an icount** | `expected=body_hash`, `got=[0;32]` | **NONSENSE** — byte_offset/epoch_len ✗  |

The two worst cases:

- **`resealed log bytes`**: `at_icount` is a *byte offset into the log*, so
  `first_bad_epoch = byte_offset / epoch_len` is a meaningless number (a
  small-byte-offset divergence yields `first_bad_epoch = 0` regardless of
  where in execution it actually diverged), and `got = [0;32]` is a
  placeholder, not the real bytes.
- **`end_vns`**: `expected`/`got` are u64s LE-packed into the first 8 bytes of
  a 32-byte slot (`u64_hash`, re:397). A consumer reading these as hashes sees
  garbage; `first_bad_epoch` points at the last epoch, not "the vns axis."

The verify.rs doc-comment claims `Divergence` "reports the FIRST bad epoch and
the hash pair" and "mirrors proto `Divergence`." For three of five shapes,
**`first_bad_epoch` is not a bad epoch and `expected`/`got` are not a hash
pair.** The proto-mirroring claim overstates what the model faithfully
represents.

**Recommendation (pick one):**

1. **Make the mapping `what`-aware.** Only compute `first_bad_epoch =
   at_icount/epoch_len` for the epoch-chain case; for end_vns / end_state_hash
   set `first_bad_epoch = expected_epochs.len()` (the END boundary) or a
   sentinel; for `resealed log bytes`, do NOT divide a byte offset by epoch_len
   — set `first_bad_epoch` to a sentinel (e.g. `u64::MAX`) and keep `at_icount`
   labeled as an offset only through `what`. **OR**
2. **Document the field shapes honestly in verify.rs.** Add to the
   `Divergence` doc: "`first_bad_epoch` and `expected`/`got` are meaningful
   only when `what` is an epoch-chain or end_state_hash divergence; for
   `end_vns` the hash slots carry LE-packed u64s, and for `resealed log bytes`
   `at_icount` is a byte offset and `first_bad_epoch` is not meaningful." The
   M8 bisection fields will need this discipline anyway.

This matters for cw2: 1000 children, each must surface a clear verdict on
failure. A `first_bad_epoch = 0` from a reseal-byte mismatch would point an
operator at the wrong place.

---

### I-3 — `verified()` last-event contract is fragile; prefer "contains Done && no Divergence"

**File:** `crates/dh-verify/src/verify.rs:43-47`

```rust
pub fn verified(&self) -> bool {
    matches!(self.events.last(), Some(VerifyProgress::Done { .. }))
        && self.divergence().is_none()
}
```

For *this* wrapper's emission order (EpochOks…, then Done) the result is
correct. But the model is a public, reusable collector (`pub events`, `pub
fn push`), and the M6 RPC (rfv) will stream these. The "last event must be
Done" semantics silently flips `verified()` to `false` if any caller pushes a
trailing event after `Done` — e.g. a future streamer that appends a
terminal `Stats`/heartbeat record, or a batched harness that concatenates two
runs' events into one report.

The *intended* contract (per the doc-comment "ends with `Done` and carries no
divergence") is really **"reached Done and never diverged."** That is better
expressed as:

```rust
self.done().is_some() && self.divergence().is_none()
```

`done()` already exists and scans for the first `Done`. This is strictly more
robust (order-independent) and still rejects the no-Done case the unit test
checks. The only behavior it changes is the pathological "Done then more
events" case — which the current implementation gets *wrong*, not right.

**Recommendation:** Change `verified()` to `self.done().is_some() &&
self.divergence().is_none()`, and update the doc to "reached `Done` and
carries no divergence." If "Done must be terminal" is genuinely a desired
invariant, enforce it in `push` (reject post-Done events) rather than encoding
it implicitly in `verified()`.
