# Critical & Important — iteration 87 (opus2)

## Critical

None.

The reviewer's nominated critical — a hostile under-length EPOCH_HASH payload panicking the reader's `chain_value: p[8..40].try_into().unwrap()` (reader.rs:184–185) — **is not reachable**. `LogReader::parse` (reader.rs:274) → `validate_records` (282) → `validate_kind` (454) enforces `KIND_EPOCH_HASH | KIND_END => payload.len() == 40` (reader.rs:539) *before* any `Record::body()` can be constructed (records are only handed out by the infallible iterator over the already-validated body). A 39-byte hostile EPOCH_HASH is rejected with a `ReadError`, never panicked. The decode-path slice index traces back to a dominating bounds check, exactly as the no_std codec research requires. This matches the per-kind length battery used for ENTROPY/SDK_EVENT/NET_TX (==16), TIMER_FIRE (==20), PAD_SET (==12), FRAME_MARK (==8). The reader side is sound; the existing `single_byte_corruptions_never_panic` test passes. **No action.**

---

## Important

### I1 — Pause roll-forward emits an EPOCH_HASH in `FinalOnly` runs; record/replay flag/record asymmetry

**File:** `crates/dh-vmm/src/runctl.rs:374–397` (the async-pause branch), interacting with `:240–245` (agenda `epoch_len` selection) and the new sink contract.

The agenda's *epoch-grid* points are correctly disabled under `HashEpochs::FinalOnly`: `epoch_len` is fed as `None` (runctl.rs:244), so `compile` sets no `epoch_hash` stop points and the sink push at `runctl.rs:337` (guarded by `if point.epoch_hash`) never fires. So far, correct.

But the **pause roll-forward push is unconditional** on `hash_epochs`:

```rust
// runctl.rs ~374
if seg.pause.load(Ordering::Relaxed) {
    let epoch = seg.config.epoch_len.max(1);
    let next_epoch = point.icount.div_ceil(epoch).max(1) * epoch;
    ... land_at(next_epoch) ...
    seg.chain.push_final_link(...);
    epoch_sink.push((b.icount / epoch, b.icount, seg.chain.value())); // <-- always
    return Ok(SegmentOutcome { reason: Paused, ... });
}
```

The inline comment at runctl.rs:240–241 acknowledges this by design ("the pause roll-forward grid below is independent config arithmetic either way") — and for the *chain* (push_final_link) that independence is correct: pausing always rolls to an `epoch_len`-aligned boundary so a pause/resume is reproducible regardless of hash mode. The problem is the **sink push**, not the chain link.

Once y62 is wired into the live recording path (the next step toward a5e/39w), `DeviceRail::log_epoch_hashes(&links, ...)` will emit a `KIND_EPOCH_HASH` record for that pause link **and** the seal will set `FLAG_EPOCH_HASHES` — on a log the operator configured `FinalOnly` precisely so it would carry *no* epoch hashes. Consequences:

1. **Spec surprise:** a `FinalOnly` log contains an EPOCH_HASH record and the `has_epoch_hashes()` header flag is set. Any consumer that branches on "FinalOnly ⇒ no epoch hashes" (39w's verify side is the obvious one) sees a record it did not expect at the pause boundary.
2. **Record/replay symmetry risk:** replay (39w) re-drives by *recorded* quantum boundaries, so it will *not* organically hit a pause at the same icount — pause is a live, wall-clock-driven event. If replay does not re-emit/expect that same EPOCH_HASH at the recorded pause boundary, the verify-side EPOCH_HASH set differs between record and replay. a5e's invariant is "every EPOCH_HASH equal"; an extra pause-induced EPOCH_HASH present in the record but not the replay (or vice versa) is exactly the kind of set-mismatch that invariant is meant to catch — but here it would be a *false* divergence caused by the producer, not a real one.

This is currently latent: the `log_epoch_hashes` caller is not yet wired into the live run loop, and the live test uses default `EpochsOn`. But y62 exists precisely to be wired next, so resolve before that lands.

**Recommended fix (pick one, smallest first):**

- **(a) Gate the sink push, keep the chain link.** Push the pause link into `epoch_sink` only when `hash_epochs == EpochsOn` (equivalently, only when the agenda's `epoch_len` was `Some`). Thread the already-computed `Option<NonZeroU64>` (or a `bool epochs_on`) into the run so the pause branch can consult it. The `push_final_link` stays unconditional. One-paragraph rationale in the comment: "the chain rolls forward in every mode for reproducible pause, but only EpochsOn surfaces it as an EPOCH_HASH record."
- **(b) If FinalOnly is intended to surface pause epoch hashes** (i.e. "FinalOnly means no *grid* hashes, but a pause boundary is always a hash"), then the asymmetry is a deliberate spec point — but it MUST be written into API.md §3.3 and into 39w's verify contract, and the live test must add a FinalOnly+pause case proving record and replay agree on that one record. Without that, the verify side has no way to know the rule.

Given a5e/czq are P0 and depend on this producer being trustworthy, (a) is the safer default unless the spec author explicitly wants (b).
