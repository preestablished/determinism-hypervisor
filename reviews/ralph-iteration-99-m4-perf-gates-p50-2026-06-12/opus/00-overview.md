# M4 Perf-Gate Instrument Review — Overview

- **Branch:** `ralph/iteration-99-m4-perf-gates-p50` vs `main`
- **Date:** 2026-06-12
- **Reviewer:** Claude Opus
- **Bead:** 9sb (M4 ACCEPT instrument), escalation 8ot, nightly consumer 1pa
- **Commit:** `afe401c ralph: iteration 99 checkpoint - M4 perf-gate instruments + measurement (9sb)`

## Scope

Four hand-written files plus a `Cargo.lock` refresh:

| File | Change |
| --- | --- |
| `Cargo.toml` | `+criterion = "0.5"` workspace dep |
| `crates/dh-worker/Cargo.toml` | `criterion` x86_64-gated dev-dep, `[[bench]] perf_gates harness=false` |
| `crates/dh-worker/tests/perf_gates.rs` | NEW — p50 acceptance gates (`#[ignore]`d) |
| `crates/dh-worker/benches/perf_gates.rs` | NEW — criterion trend instrument |
| `Cargo.lock` | criterion transitive closure |

## Summary

This is a well-built instrument. The timed windows are correctly scoped against the
three engine signatures: `restore_snapshot` takes an already-created slot (slot creation
is excluded in both test and bench, per §8.3), `fork_slot` owns child-slot creation and
the codec apply (both correctly inside the timer, with `bus_c`/`test_bus()` construction
and `drop(outcome)` teardown outside it), and `take_snapshot`'s incremental path drains
the ring + ships the dirty set inside the window. The thresholds (fork < 10 ms,
incremental ≤ 8k pages < 15 ms, warm restore < 150 ms, all p50 at 128 MiB) match the
IMPLEMENTATION-PLAN §M4 wording exactly. The `#[ignore]` + debug-build-refusal pattern
correctly keeps perf assertions out of the parallel `cargo test` sweep (the iteration-68/69
flake lesson). The comments describe the failures as storage-bound without editorializing
the thresholds away.

The one substantive measurement-fidelity note is the **8k dirty-page methodology**: the
test/bench populate `DirtyPageSet` via direct host-side `dirty.insert()` with an **empty
dirty ring**, so `harvest_at_boundary` (which the engine *always* calls inside the timed
incremental window) drains a zero-entry ring and skips the KVM `reset_dirty_rings` ioctl.
A guest-dirtied 8k-page run would harvest 8192 ring entries (across multiple
`DirtyRingFull` boundaries) plus the reset ioctl — that cost is part of the gate per the
plan's "incremental snapshot ≤ 8k dirty pages" wording but is **not** measured here. Given
the gate is currently storage-bound (111.6 ms vs 15 ms, the harvest delta is sub-millisecond
noise next to 32 MiB of fsync), this does not change the M4 verdict or the 8ot escalation —
but it should be documented so the gate is not mistaken for a complete-path measurement once
the storage path is fixed.

No correctness bugs. No blocking issues. The escalation framing is accurate.

## Verdict

**APPROVE**

The instrument is sound and the measurement it produced (fork passes; snapshot and restore
fail, both storage-bound) is trustworthy for the 8ot human decision. The harvest-cost gap is
a documentation/scope note, not a defect in this change — recorded as Important so it travels
with the bead.

## Stats

- Files reviewed: 4 new/changed source files + 3 mirrored test files + 4 engine sources
- Critical: 0
- Important: 1 (measurement-scope documentation, non-blocking given storage-bound context)
- Suggestions: 5
- Positive notes: 6
