# Suggestions (non-blocking)

### S1 — Add a guest-observable device assertion to earn back the "device-state leak" claim

**File:** `crates/dh-worker/tests/m4_transparency.rs:259-265`

`restore_snapshot` hands back `outcome.entropy` (the restored `DetEntropy`) and sets the restored
`PvClock`'s `vns_base`, but the test consumes neither. Two cheap, fully in-process checks would
turn the round-trip's device path from "exercised" into "verified":

```rust
// The restored PRNG continues the snapshot's stream, not a fresh seed.
let mut restored = outcome.entropy;
let mut control = DetEntropy::from_seed([9; 32]); // same seed take_snapshot used
// (advance `control` by however many draws the captured state had consumed — 0 here,
//  since `from_seed` is never drawn from before the snapshot, so they start equal)
assert_eq!(restored.next_u64(), control.next_u64(), "ENTR round-trip");
```

and, if `PvClock` exposes its base, assert the restored clock's `vns_base == r1.vns`. This is the
ENTR-golden-test idea folded into the transparency test; it directly addresses I1 and costs
microseconds. (If the entropy/clock accessors aren't public on the dev-facing API, leave this to
the dedicated ENTR test and instead apply the I1 doc trim.)

### S2 — Assert the full `r2 == c2`, not just three of its fields

**File:** `crates/dh-worker/tests/m4_transparency.rs:269-274`

The pre-snapshot check uses the strong form `assert_eq!(r1, c1, ...)` (`:215`) — full
`SegmentOutcome` equality. The post-restore check instead compares `r2.boundary`, `r2.vns`, and
`r2.state_hash` field by field. That silently omits `reason`, `injections_delivered`, and
`timer_fired` from the second-leg comparison. They're all trivially equal here, but asserting the
whole struct is both shorter and symmetric with the `r1 == c1` line:

```rust
assert_eq!(
    r2, c2,
    "H1 != H2 (or the landing position / virtual time diverged): the \
     snapshot/restore detour is VISIBLE — an instruction-count, RAM, or vCPU leak"
);
```

Keep the three targeted asserts above it as failure-localizers if you like the granular messages,
but the wholesale equality should be the actual gate. (Coordinate the message with I1 on the
"device-state" wording.)

### S3 — Cheap structural assertions on the snapshot/restore outcomes

**File:** `crates/dh-worker/tests/m4_transparency.rs:236, 260`

`take_snapshot` returns `pages_shipped` and `restore_snapshot` returns `pages_loaded` and
`epoch_index`; none are asserted. For a `PageSource::Full` snapshot of a 16 MiB slot these are
known constants and confirm the round-trip moved the bytes it claims to:

```rust
assert_eq!(snap.pages_shipped, MEM / 4096, "full snapshot ships every page");
// ...
assert_eq!(outcome.pages_loaded, MEM / 4096, "restore materialized every page");
assert_eq!(outcome.epoch_index, HALF / cfg.epoch_len, "restored epoch index");
```

The `epoch_index` check in particular guards against a TIME-section off-by-one that
`cumulative_icount` alone wouldn't catch.

### S4 — Tie the snapshot's recorded chain to the pre-snapshot leg

**File:** `crates/dh-worker/tests/m4_transparency.rs:236`

`BoundaryState.hash_chain` is fed `chain.value()` (`:230`), which after `run_more` returns equals
`r1.state_hash`. `take_snapshot` echoes it back as `snap.hash_chain`. One line confirms the
snapshot stored the chain the pre-snapshot leg actually produced (and, transitively, the same
value the control leg reached at 1e8 via `r1 == c1`):

```rust
assert_eq!(snap.hash_chain, r1.state_hash, "snapshot recorded the pre-snapshot chain");
```

This makes the "the restored chain is the parent's chain" invariant explicit rather than implicit
in `from_value(time.hash_chain)`.

### S5 — Deduplicate the copy-pasted `gettid` / `kvm_usable` helpers

**File:** `crates/dh-worker/tests/m4_transparency.rs:63-81`

`kvm_usable()` and `gettid()` are byte-for-byte identical to the copies in
`tests/determinism/tests/regression.rs:28-50` and the three `runctl.rs` test modules. The Rust
integration-test convention (and the research note, "shared fixture code deduplicated rather than
copy-pasted") is `tests/common/mod.rs`. This is low priority — the duplication is small and the
two files live in different packages so a shared module would need to be a tiny published helper —
but worth a note so it doesn't metastasize as M5/M6 add more live tests in this package.

### S6 — The `let _ = chain;` shadow is a no-op; a comment-only fence reads clearer

**File:** `crates/dh-worker/tests/m4_transparency.rs:243`

`let _ = chain;` after `drop(slot)` is intended (per the comment) to "shadow it so nothing below
can touch pre-snapshot state by accident," but `let _ = chain;` does **not** move or shadow
`chain` — it's a binding to a `Copy`/by-value discard of a `&mut`… actually `chain` here is a
`StateHashChain` value, so `let _ = chain;` *does* move it, making subsequent use a compile error.
That's the intended fence and it works. The follow-up `let mut chain = outcome.chain;` (`:264`)
then rebinds a fresh name. This is fine; only flagging that the comment ("shadow it") slightly
mis-names the mechanism (it's a move-out, not a shadow). A one-word comment tweak ("move it out so
nothing below can touch pre-snapshot state") removes the ambiguity. Pure nit.
