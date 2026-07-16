# Suggestions (non-blocking)

These are optional polish items. None block merge.

---

## S1 — The two `let epoch = seg.config.epoch_len.max(1);` bindings could share one hoisted let

**File:** `crates/dh-vmm/src/runctl.rs:337` and `:375`.

The agenda-walk arm and the pause branch each recompute `let epoch = seg.config.epoch_len.max(1);`.
They are correct as-is (both local, both identical), but hoisting a single `let epoch =
seg.config.epoch_len.max(1);` once near the top of the loop body would remove the duplication and
make it visually obvious the two sink sites use the same divisor. Minor; current form is fine and
arguably clearer about locality. Leave or hoist at author's taste.

---

## S2 — Consider a debug assertion that sinked icounts are grid-aligned

**File:** `crates/dh-vmm/src/runctl.rs:338,397`.

Both sink sites assume `point.icount` / `b.icount` is an exact multiple of `epoch`. This holds by
construction today (the agenda emits epoch points on the grid; the roll-forward computes
`next_epoch` as a grid multiple). A cheap `debug_assert_eq!(icount % epoch, 0)` immediately before
each push would turn a future invariant-breaking refactor (e.g. someone changing how epoch points
are scheduled) into a loud test failure rather than a silently-wrong `epoch_index`. Purely
defensive; the live test already pins the happy path.

---

## S3 — `log_epoch_hashes` doc could note it is a no-op on an empty slice

**File:** `crates/dh-vmm/src/recording.rs:73-84`.

`log_epoch_hashes(&[], rip)` correctly writes nothing and leaves `wrote_epoch_hash` false (so a
quantum that crossed no epoch boundary does not spuriously set `FLAG_EPOCH_HASHES`). This is the
right behavior and the loop handles it naturally, but a one-line doc note ("an empty `links` slice
is a no-op — does not set the header flag") would make the empty-quantum contract explicit for the
39w replay author who will pair with this. Documentation-only.
