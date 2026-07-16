# Iteration 38 — guest-armed timer chain — Review Overview

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-38-timer-arming` vs `main`
- **Bead:** determinism-hypervisor-5v5 (pv-clock timer arming -> agenda point -> vector injection)
- **Commit reviewed:** `a3defb8` (iteration 38 checkpoint)
- **Files in scope:** `crates/dh-vmm/src/runctl.rs` (primary), `crates/dh-devices/src/clock.rs`, `crates/dh-vmm/src/vt.rs`, `crates/dh-vmm/src/agenda.rs`, `crates/dh-vmm/src/inject.rs`, `crates/dh-vmm/src/hash.rs`; ARCH §4 / §6.2 / §3.4.

## Verdict

**APPROVE.** The conversion math, agenda merge, one-shot reporting, and the live chain are correct and the live test is deterministically stable. Two doc/bead-trail gaps are worth fixing before the device run loop lands (both Important, neither blocks merge): the absolute-vs-relative `vns` contract is asserted in code comments but never normatively in ARCH §6.2, and the mid-segment re-arm / stale-agenda hazard has no bead note on the future-wiring beads (40q / 583). Everything host-runnable and live-runnable in scope was exercised.

## What I verified (independent of first reviewer)

1. **5x live-stability run** of `armed_timer_fires_and_reports_live`: 5/5 PASS, zero skips (`/dev/kvm` rw confirmed), `delivered_icount == DEADLINE` every run — the no-deferral outcome is deterministic, not flaky. Walk-through of *why* is in 03-positive-notes.md.
2. **Overflow path** (`deadline_vns = u64::MAX`, halving clock) → `RunError::ClockOverflow`, no panic — proven with a scratch test (added, run, reverted; tree clean). `deadline_vns = 1` @ 1:1 → icount 1 via `max(1, start+1)`.
3. **Disarm semantics**: `TIMER_DEADLINE = 0` → `armed()` returns `None` (clock.rs:97-99); `TimerArm` is only constructed from `Some(..)`, so the converter never sees deadline 0. Confirmed in source.
4. **Pending-vector-crosses-boundary** (budget == deadline): the queued vector is captured by the state hash via `events.interrupt.injected`/`nr` (hash.rs:272-273) — both replays end equal. M4 restore of that state exists structurally (ARCH §8.1 + §8.3 VCPU_EVENTS). Detail in 03.
5. **Doc-trail adjudication** of the absolute-vs-relative `vns` seam — clock.rs and runctl.rs agree; ARCH §6.2 is silent (gap). Detail in 01.

## Stats

- Files reviewed: 6 source + 1 ARCH doc
- Tests run: `runctl::timer_tests` (2), `vt::tests` (8), `runctl::timer_tests::armed_timer_fires_and_reports_live` x5, 1 scratch (reverted)
- Test outcome: all green, 0 flakes across 5 live repeats
- Findings: 0 Critical, 2 Important, 3 Suggestions, 4 Positive notes
- Build: `cargo build -p dh-vmm -p dh-devices` clean
