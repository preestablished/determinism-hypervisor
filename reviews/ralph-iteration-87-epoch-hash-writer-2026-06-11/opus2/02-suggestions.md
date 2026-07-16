# Suggestions — iteration 87 (opus2)

## S1 — Pin the absolute-grid anchoring with a doc note that names a5e/39w

The single most load-bearing determinism property of this change is that `epoch_index = point.icount / epoch_len` is **identical across record and replay even when the two runs are quantized differently**. That holds *only* because the agenda anchors the epoch grid at **absolute segment-start-0 multiples of `epoch_len`**, not start-relative offsets:

- `agenda.rs:157` — "Epoch grid: multiples of epoch_len from SEGMENT start (icount 0) within (start, final]"
- `agenda.rs:59` — "Segment alignment means a pause/resume never shifts the hash grid."
- `agenda.rs:465` (property test) — `p.epoch_hash == (p.icount % len == 0 && p.icount > start)`

So `point.icount` is always an exact multiple of `epoch_len`, the division is exact, and a quantum starting at 100k under a 30k grid links at 120k/150k/180k — the *global* grid, not a local 100k+30k grid. This is precisely what 39w (replay re-grids per quantum from recorded boundaries) and a5e ("every EPOCH_HASH equal, x100") require.

The new code at `runctl.rs:336–338` depends on this invariant implicitly. Add a one-line note at the sink push pointing at the agenda guarantee, e.g.:

```rust
// epoch_index is the ABSOLUTE grid index (agenda anchors multiples of
// epoch_len at segment-start 0, not start-relative) — so this index is
// identical under record and replay regardless of quantum boundaries.
// This is the a5e "every EPOCH_HASH equal" invariant; do not make it
// start-relative.
let epoch = seg.config.epoch_len.max(1);
epoch_sink.push((point.icount / epoch, point.icount, seg.chain.value()));
```

A future refactor that "optimizes" the grid to start-relative would silently break a5e; the comment is cheap insurance.

## S2 — Add a non-hardware unit test for the sink↔index correspondence

The only coverage of the sink population (`epoch_index` arithmetic, the epoch-grid push, and the pause roll-forward push) is the kvm-gated live test `epoch_hashes_flow_from_quantum_to_sealed_log`, which is skipped wherever `/dev/kvm` is unavailable (CI sandboxes, this review env). The agenda grid itself has solid property tests, but the *new* `point.icount / epoch` mapping and the pause `div_ceil` index do not. Consider a small `#[cfg(test)]` harness that drives `run_segment_with_epochs` over a stub/no-exit path (or factor the `(icount → (index, icount))` mapping into a tiny pure fn and table-test it: `30000/30000==1`, `60000/30000==2`, boundary at `u64::MAX`, pause `div_ceil` landing). Keeps the index logic green where hardware is absent.

## S3 — `run_segment` delegation allocates a throwaway `Vec` per call

`run_segment` now calls `run_segment_with_epochs(..., &mut Vec::new())`. Every existing `run_segment` caller (the non-recording path) now allocates a zero-capacity `Vec` that is immediately discarded. `Vec::new()` does not allocate until first push and these callers never push (their agenda may still have epoch points though — if `hash_epochs == EpochsOn`, the sink *will* grow and then be dropped). For a hot per-quantum loop that is a small avoidable allocation+free on the epoch path. Minor; if it ever shows up, a `&mut dyn FnMut(u64,u64,[u8;32])` sink callback (or `Option<&mut Vec<...>>`) would let `run_segment` pass a no-op. Not worth doing speculatively — noted for awareness.

## S4 — 39w readiness note (the EPOCH_HASH VERIFY side)

**With y62 landed, 39w's first iteration should implement the verify-as-you-go epoch check, and it can rely on the producer contract this iteration establishes.** Concretely, the replay executor walks the DHILOG, and at each `KIND_EPOCH_HASH` record it must: (1) compute the live §8.5 chain value at that record's `icount` using `push_final_link` on the *replay* chain exactly as `run_segment_with_epochs` does on the record side (same empty-payload final link, same vns-from-icount), (2) compare both the `epoch_index` (must equal `icount / epoch_len` — the absolute grid index, per S1) and the 32-byte `chain_value` against the record, and (3) on mismatch report a `Divergence` at that icount (feeds 1py's VerifyReplay path), else continue. Two contract points 39w must honour, both surfaced by this review: **(a)** the index is the *absolute* grid index, so replay must use the same global `epoch_len` anchoring and must NOT re-grid start-relative per quantum; **(b)** resolve I1 first — if the pause roll-forward keeps emitting EPOCH_HASH under FinalOnly, 39w's verify side needs to know that a pause boundary can carry an "extra" epoch hash that the organic replay grid won't reproduce, or it will report a false divergence. Until I1 is decided, 39w should restrict its first acceptance to `EpochsOn` runs without async pause (a5e's pad-echo sequence is exactly that), deferring FinalOnly+pause to a follow-up once the spec point is nailed down. The `RecordBody::EpochHash { epoch_index, chain_value }` accessor and the `has_epoch_hashes()` header flag are both ready for the consumer; no reader changes are needed for 39w to start.
