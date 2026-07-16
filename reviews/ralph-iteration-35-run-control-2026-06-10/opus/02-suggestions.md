# Suggestions

### S1 — Pause roll-forward can overshoot the requested budget

**File:** `crates/dh-vmm/src/runctl.rs` lines 239-263.

The math itself is correct (verified, see below), but `next_epoch` is **not clamped to
the segment's final icount**. A pause arriving at a near-final non-epoch agenda point
rolls forward to `next_epoch`, which can be **past** the requested `IcountBudget`/`VnsBudget`
final boundary. The engine then runs the guest beyond the budget the caller asked for,
and reports `Paused` at a boundary greater than the budget.

§3.3 sanctions rolling forward to the next epoch ("latency ≤ epoch_len") for an
*external* pause, so this is arguably within spec — but with the default `epoch_len =
50_000_000`, a pause near the end of, say, a 60M-instruction budget would run up to ~50M
instructions *past* the budget. That is a large, surprising overrun. Consider clamping:
`next_epoch.min(final_icount)` — landing at the budget boundary is still a deterministic
point, and "pause" semantically subsumes "stop". At minimum, document the overrun.

Also note the interaction with I1: if `hash_epochs == FinalOnly`, the pause branch still
computes `next_epoch` from `epoch_len` and hashes there, even though the rest of the run
emitted no epoch links. The pause grid and the hash grid should be the same grid.

**Math verification (no defect in the arithmetic):**
- `point.icount == k*epoch`: `div_ceil → k`, `*epoch → k*epoch == point.icount` →
  `land_at(point.icount)` returns immediately (`c == target`, no Overshoot). ✓
- `point.icount` between multiples: `div_ceil` rounds up → strictly the next multiple. ✓
- `point.icount == 0`: `div_ceil(0)=0`, `.max(1)=1` → `next_epoch = epoch`. ✓ (guards the
  zero case; cannot land before the current point.)

### S2 — `gettid()` correctness depends on "called from the main thread"; make it robust or assert

**File:** `tools/dh-cli/src/run.rs` lines 13-17.

```rust
fn gettid() -> i32 { std::process::id() as i32 }   // "main thread's tid IS the pid"
```

This is true **only** on the process's main thread, and only because the run command is
invoked directly from `main` on the main thread. It is correct today. But the counter's
overflow signal is routed to this tid (`route_overflow_to_thread`), so if a future
refactor ever calls `dh_cli::run::run` from a spawned thread (a test, a server worker),
the PMI kick is delivered to the wrong thread and the boundary engine stops working —
a subtle, hard-to-debug failure, not a loud one.

dh-cli forbids `unsafe`, so the clean `libc::syscall(SYS_gettid)` used in the dh-vmm tests
isn't available here. Options: (a) read `/proc/thread-self/task` / parse `/proc/self/stat`
field; (b) gate `run::run` with a debug assertion that it is on the main thread (compare
against a captured main-thread id); (c) at minimum, expand the comment into a hard
**precondition** ("MUST be called on the process main thread") and add it to the `run`
doc, since the function is `pub` in a `lib.rs`-exposed module and could be called from
anywhere. The dh-vmm test module's `gettid` uses the real syscall and is unaffected.

### S3 — Guest HLT/Shutdown is reported as a fatal `RunError`, not `GUEST_HALTED`

**File:** `tools/dh-cli/src/run.rs` lines 661-667 (`on_exit`).

API.md §2.4 defines `StopReason::GUEST_HALTED` ("guest executed terminal HLT / triple
fault") and `FAULTED`. The Phase-1 `StopReason` enum omits both, and the dh-cli `on_exit`
turns any non-serial exit (including `Hlt`/`Shutdown`) into `BoundaryError::Exit(...)` →
`RunError` → process exit 1. For Phase-1 demo guests (landing loop never halts) this is
fine, but it means a halting guest is an *error* rather than a clean `GUEST_HALTED`
outcome. Fine to defer to the device-loop bead, but worth a tracking note so the
StopReason subset is completed deliberately rather than by omission. The module doc
already scopes NextSdkEvent/FrameBudget as NotYetWired; GUEST_HALTED/FAULTED deserve the
same explicit "Phase-1 out of scope" mention.

### S4 — `injections_delivered` counts queue attempts, not deliveries; will over-count once C1 is "fixed by overwrite"

**File:** `crates/dh-vmm/src/runctl.rs` lines 206, 256, 285.

`delivered` increments once per `inject_at_boundary` return. Given C1, when two vectors
share a boundary today, `delivered` becomes 2 while only 1 vector actually reaches the
guest. The field doc on `SegmentOutcome::injections_delivered` says "actually delivered".
Once C1 is fixed so both vectors genuinely deliver, the count becomes truthful — so this
is mostly a corollary of C1, but flag it: the counter should reflect deliveries, and the
fix for C1 should make it so by construction. If you take the "reject len>1" interim fix,
the count is trivially correct.

### S5 — `at.rcx` carried unchanged across chained injections is a stale diagnostic

**File:** `crates/dh-vmm/src/runctl.rs` lines 207-211.

After an injection, the new `at` keeps the **pre-injection** `rcx`:

```rust
at = Boundary { icount: inj.delivered_icount, rip: inj.delivered_rip, rcx: at.rcx };
```

`Boundary::rcx` is documented as diagnostics-only (REP progress), so this is harmless to
correctness. But `Injection` does not carry `rcx`, so the chained boundary's `rcx` no
longer corresponds to its `(icount, rip)`. Minor: either drop `rcx` from the chained
synthetic boundary (it isn't re-read) or have `inject_at_boundary` return it. Lowest
priority; noted for completeness because it is the kind of stale-diagnostic that confuses
a future debugger.
