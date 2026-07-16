# Review: recording integration (DeviceRail, bead y78)

- **Branch:** `ralph/iteration-86-recording-integration`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus
- **Commit:** `d5625d8` ralph: iteration 86 checkpoint - recording integration (DeviceRail, y78)
- **Scope:** 5 files, +582 / 632 diff lines, 1 commit

## Summary

This change productizes the `m1_acceptance` `on_exit` harness into a reusable
`DeviceRail<M: GuestMem>` struct in `crates/dh-vmm/src/recording.rs`. The rail
owns the per-segment device surface (bus, serial, entropy, the segment
`LogWriter`, the guest-mem adapter, the pending-IRQ queue) and exposes:

- `service_exit(icount, exit)` — the productized `on_exit` body (serial PIO +
  bus MMIO + loud log-fault check);
- `apply_pad_set` / `apply_net_rx` — canonical-input entry points that PAIR the
  device mutation with the record landing and queue any returned edge vector;
- `drain_net_tx` — the §6.7 loopback frame-recovery seam (read TX frame from
  guest RAM at the doorbell exit);
- `set_clock_vns_base` — a restore-time clock reseed passthrough;
- `seal(outcome, end_snapshot_id)` — builds `SealParams` from the
  `SegmentOutcome`, refuses an undrained IRQ queue.

Supporting changes: `PvPad` and `PvNet` gain `as_any_mut` overrides (the
`PvClock` downcast pattern), a new free function `stop_reason_u8` mirrors the
proto `StopReason` numbering with a cross-pin in `dh-worker/proto_map.rs`, and
two tests land — a host-level NET_RX pairing test and a live `pad_echo` proof
across three budget-bounded segments.

The work is clean, well-documented, and the live test is a genuine end-to-end
proof (PAD_SETs reach the guest AND land in the log at the exact landed
icounts; >10 FRAME_MARK AUX records flow from the device; END carries the
outcome). The productization is faithful to the m1 template for the parts it
covers. My concerns are about completeness against the bead's "every ... AUX
record lands ... during live runs" scope (TIMER_FIRE has no pairing method),
the documented-but-unenforced pairing atomicity claim, and one misleading
error variant.

## Verdict

**NEEDS_DISCUSSION**

The code is correct and merge-quality for what it implements. The discussion
items are about whether y78's AUX scope (specifically TIMER_FIRE) is meant to
be discharged here or is deferred to run-control wiring in a later bead, and
whether the "atomically from the caller's perspective" pairing doc should be
tightened to state the slot-fatal invariant it actually relies on. None of
these are correctness bugs in the shipped path; they are scope/contract
clarifications that the author can resolve quickly, after which this is an
APPROVE.

## Stats

| Metric | Count |
|---|---|
| Critical | 0 |
| Important | 2 |
| Suggestions | 5 |
| Positive notes | 6 |

## Files reviewed

- `crates/dh-vmm/src/recording.rs` (new, +548)
- `crates/dh-vmm/src/lib.rs` (module decl)
- `crates/dh-devices/src/pad.rs` (`as_any_mut` hunk)
- `crates/dh-devices/src/net.rs` (`as_any_mut` hunk)
- `crates/dh-worker/src/proto_map.rs` (cross-pin test)

## Context consulted

- `tests/determinism/tests/m1_acceptance.rs` (the productization template)
- `crates/dh-vmm/src/runctl.rs` (`SegmentOutcome`, `StopReason`, `TimerFired`)
- `crates/dh-inputlog/src/dhilog.rs` (record/seal signatures, AUX/canonical split, failure semantics)
- `crates/dh-devices/src/pad.rs`, `net.rs`, `ctx.rs` (device + DevCtx methods)
- `bd show determinism-hypervisor-y78` (scope)
