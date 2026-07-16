# Critical & Important findings — pv-blk

## Critical

None. The execution paths (`do_read`, `do_write`, `request_range`, `execute`) are pure functions of (base content, overlay state, guest RAM, request registers). I specifically hunted for record-vs-replay divergence and found none:

- **HashMap iteration order never reaches guest-visible state.** `do_read`/`do_write` use `overlay.get(&cluster)` / `contains_key`/`get_mut` — point lookups by computed key, never iteration. The only iteration over the map is in `snapshot`, which collects keys into a `Vec`, `sort_unstable()`s them, and serializes in sorted order (blk.rs:255-263). `snapshot_is_sorted_deterministic_and_roundtrips` dirties the same clusters in opposite order on two devices and asserts byte-identical overlay serialization — the divergence hazard is closed and tested.
- **Chunk sizes are deterministic.** `take = remaining.min(CLUSTER_SIZE - within)` depends only on `off`/`remaining`, both functions of the request. No host-dependent branch.
- **No allocation-failure-dependent behavior** on the hot path beyond `vec![0u8; take]` / `Box::new([0; CLUSTER_SIZE])`, whose failure aborts (consistent on replay over the same state).

So nothing here is replay-Critical.

---

## Important

### I-1. `do_read` MEM_FAULT leaves a partial guest-RAM write; doc comment is silent

`do_read` (blk.rs:152-176) writes each cluster-chunk into guest RAM as it goes:

```rust
if ctx.mem.write(gpa, &chunk).is_err() {
    return STATUS_MEM_FAULT;
}
off += take as u64; gpa += take as u64; remaining -= take;
```

If a multi-chunk read faults on chunk *k*, chunks `0..k` are already committed to guest RAM and STATUS is `MEM_FAULT`. `do_write` has an explicit comment acknowledging its partial side effect ("The cluster stays populated (RMW already happened)…", blk.rs:202-204), but `do_read` has **no** comment about the partial guest-RAM write it leaves behind.

This is **deterministic** (same request over same state → same partial result), so it is *not* a replay hazard — which is exactly why it is easy for a future maintainer to "fix" it into something that *is* one (e.g. by buffering the whole transfer and writing atomically, or by zeroing on fault). The CoW contract and the §6.5 STATUS semantics should state plainly that a `MEM_FAULT`/`HOST_IO` request may leave guest RAM and/or the overlay **partially** mutated, and that this partial state is itself part of the deterministic device state.

**Why Important, not Suggestion:** a guest driver that retries a `MEM_FAULT` read assuming the buffer is untouched will read stale-prefix + new-suffix data. The behavior is fine and deterministic, but it is an undocumented guest-ABI fact on a device that other beads will build drivers against.

**Fix:** add a one-line note to the §6.5 STATUS doc block (and a comment at blk.rs:168) that non-OK completions may leave the destination partially written; do not change the code.

### I-2. Snapshot omits `host_io_errors`; restored `STATUS_HOST_IO` pairs with a zero counter

`snapshot` serializes `sector / buf_gpa / count / status` + overlay (blk.rs:249-264). It does **not** serialize `host_io_errors`. `restore` (blk.rs:266-297) sets `status` from the bytes but leaves `host_io_errors` at whatever the fresh device had (0 from `new`).

Consequence: snapshot a device the instant after a `CMD_READ` that returned `STATUS_HOST_IO` (so `status == 0xFE`, `host_io_errors == 1`), then restore into a fresh `PvBlk`. The restored device reports `status == 0xFE` with `host_io_errors == 0`.

The `STATUS_HOST_IO` doc block (blk.rs:55-61) tells run control to treat "a nonzero check of `PvBlk::host_io_errors` after the exit as slot-fatal." That contract is written for *live* dispatch (counter bumped in `execute` right before the exit returns). After a restore, the (status, counter) pair `(0xFE, 0)` is a state the live device can never produce, and any run-control logic that trusts the counter as the sole host-IO signal would mis-classify a restored-mid-fault slot as healthy.

In practice the host-IO STATUS is meant to be transient and slot-fatal — a slot that hit `STATUS_HOST_IO` should already be faulting and never reach a clean snapshot. But nothing in this code *enforces* that, and `restore` silently accepts `status == 0xFE`.

**Why Important:** it is a latent correctness gap the moment run control starts consuming `host_io_errors`. It is not Critical because no caller consumes the counter yet (confirmed: `grep host_io_errors crates/dh-vmm` finds only `blkfile.rs`/`blk.rs` — no run-control wiring exists).

**Fix options (pick one, document the choice):**
1. Serialize `host_io_errors` in the section (bump `SECTION_VERSION` to 2, add a u64; restore it). Cleanest — makes the counter a true function of device state.
2. On `restore`, reject `status == STATUS_HOST_IO` (return `RestoreError`) on the grounds that a host-IO state is never snapshottable — and document that invariant.
3. On `restore`, normalize a restored `STATUS_HOST_IO` to `STATUS_BAD_REQUEST`/`STATUS_OK` and document that the counter, not STATUS, is the canonical host-IO signal — but then STATUS itself becomes the lie.

Option 1 is preferred: it keeps `snapshot`/`restore` a total round-trip of all observable state and matches the trait doc ("Must be a pure function of device state"). `host_io_errors` *is* device state.
