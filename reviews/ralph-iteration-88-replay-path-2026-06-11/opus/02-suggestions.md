# Suggestions

### S1 — The boundary-rip cross-check: a cheap belt, but the chain already wears suspenders

**File:** `crates/dh-worker/src/replay_engine.rs:413-426` (PadSet) / 428-433 (NetRx)

The prompt asks: replay re-records PAD_SET with `rec.boundary_rip()` (the rip **from the input
log**, not the replayed machine's actual boundary rip). So a replay that lands at the correct
icount but a *different* rip would still re-record the same rip → produce a byte-identical
reseal → mask the rip divergence. Should the engine cross-check `out.boundary.rip ==
rec.boundary_rip()`?

**Judgment: cheap strengthening, but largely subsumed — implement only as defense-in-depth.**

I traced the hash chain (`crates/dh-vmm/src/hash.rs:181-208`): `push_final_link` calls
`get_regs()` and folds **rip plus all 16 GPRs and rflags** into the chain at every epoch-grid
point AND every quantum boundary (`runctl.rs:346`, `405`, `445`). In replay, each `run_to`
quantum lands exactly at a PAD_SET icount and pushes a final link there, so the boundary rip
*is* chained. A rip divergence at a boundary therefore propagates into the next EPOCH_HASH (or
`end_state_hash`) and is caught by the existing checks.

The one residual gap the cross-check would close: a rip that diverges at a *non-epoch* quantum
boundary AND reconverges (rip and all regs) before the next epoch-grid point AND leaves
`end_state_hash` identical. That is an extraordinarily narrow window (the machine would have to
take two different control-flow paths to the same icount that reconverge bit-for-bit by the
next epoch). It is not impossible in principle, and the cross-check is two lines:

```rust
if out.reason == StopReason::BudgetReached && out.boundary.rip != rec.boundary_rip() { ... }
```

But note `run_to` currently discards `out` except for `out.reason`/`out.boundary.icount`, and
the rip is only meaningful when `target` is a real record boundary (the tail run_to lands at
`end_icount` where the log carries no canonical record). I'd file this as a low-priority
follow-up rather than block: the chain already carries rip, so this is pure defense-in-depth,
not a correctness fix.

---

### S2 — `service_exit` and the apply_* methods stamp `boundary_rip = 0` for exits but the log's recorded rip for inputs — document the asymmetry where replay relies on it

**File:** `crates/dh-worker/src/replay_engine.rs:415, 424-425`

`run_to`'s `service_exit` closure passes rip implicitly as 0 (recording.rs:103-111 documents
the debug-loop convention: rip is "not retrievable while the segment holds the vCPU"), while
`apply_pad_set`/`apply_net_rx` use `rec.boundary_rip()`. This is correct (it mirrors the
recording exactly, which is what makes the reseal byte-identical), but the replay engine never
states *why* it trusts the log's rip for inputs but ignores the machine's. A one-line comment at
the `let rip = rec.boundary_rip();` site ("rip comes from the log to reproduce the recording's
record bytes; the machine's rip at this icount is verified via the chain, not here") would save
the next reader the trace I just did for S1.

---

### S3 — `ReplayOutcome` lacks `Debug`; tests work around it

**File:** `crates/dh-worker/src/replay_engine.rs:242-250`

`ReplayOutcome` derives nothing. The tests `.expect("replay")` on the `Result` (which works
because `ReplayError: Debug`) and then read fields individually, so they don't hit this — but
the moment a test wants `assert_eq!` or a `dbg!` on the whole outcome, or a caller wants to log
it, the missing `Debug` bites. `#[derive(Debug)]` is free here (all fields are `Debug`).
`#[derive(Clone)]` may also be wanted by 1py if it threads the outcome around. Trivial.

---

### S4 — Reseal copies `header.end_snapshot_id` without producing/verifying an end snapshot

**File:** `crates/dh-worker/src/replay_engine.rs:488-491`

`seal(&outcome_like, header.end_snapshot_id)` reproduces the input's `end_snapshot_id` field
byte-for-byte, which is what makes the reseal byte-identical. But it means: if the original
recording took an end snapshot (nonzero id), replay does **not** take a new one and does **not**
verify that re-snapshotting the replayed machine yields the same id. For the phase-1 verify path
(1py needs only hash comparison) this is fine and correct. Worth a one-line note in the module
doc that end-snapshot *reproduction* (not just id passthrough) is out of scope here, so a later
reader doesn't mistake the byte-identical reseal for proof that the end snapshot was
re-validated. The reader does not validate `end_snapshot_id` against anything either, so the
field is purely passthrough on both sides — consistent, but undocumented.
