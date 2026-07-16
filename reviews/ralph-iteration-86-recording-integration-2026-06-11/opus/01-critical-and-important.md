# Critical and Important findings

## Critical

None.

---

## Important

### I-1. TIMER_FIRE has no pairing method on the rail — a scope gap against y78's "every AUX record lands ... during live runs"

`crates/dh-vmm/src/recording.rs` (whole `DeviceRail` impl).

The bead y78 scope is: "every canonical input (PAD_SET ... NET_RX) **and AUX
record** lands in the segment log at the correct boundary icount, with sealing
at segment end." The module docstring (recording.rs:1–7) restates this. The
rail discharges this for the AUX records that flow *through the DevCtx during
dispatch*:

- **FRAME_MARK** — emitted by `PvPad::mmio_write` via `ctx.log_frame_mark`
  (pad.rs:126–129), exercised by the live test (>10 marks). ✓
- **ENTROPY** / **NET_TX** — `ctx.log_entropy` / `ctx.log_net_tx` from the
  device handlers, flowing through the `DevCtx` `service_exit` builds. ✓ (by
  construction; not independently exercised here).

But **TIMER_FIRE is structurally different**: it is NOT produced by a device
MMIO handler. `run_segment` returns it as `SegmentOutcome.timer_fired:
Option<TimerFired>` (runctl.rs:69), and the field's own doc says: *"the caller
logs AUX TIMER_FIRE and disarms the device (one-shot)"* (runctl.rs:68, and
again on `TimerFired` at runctl.rs:139–141). `dh-inputlog` provides
`LogWriter::timer_fire(...)` (dhilog.rs:254) precisely for this caller.

The rail is the natural "caller" — it owns the `LogWriter` and (via the
`set_clock_vns_base` downcast pattern, recording.rs:225–230) can reach the
`PvClock` to disarm. Yet there is **no `log_timer_fire` / `apply_timer_fired`
method**. A rail owner who runs a segment that fires a timer has the
`TimerFired` in hand but no rail API to (a) land the AUX TIMER_FIRE record at
`delivered_icount` and (b) disarm the one-shot. They would have to reach into
`rail.log` directly and downcast the clock themselves — exactly the
re-derivation the rail exists to prevent.

This is the one place the productization is *narrower* than the m1 template's
intent (m1 didn't exercise timers, so the template doesn't cover it either —
but y78's text explicitly enumerates the AUX set, and TIMER_FIRE is in it per
§3.3 / dhilog's KIND_TIMER_FIRE).

**Judgment:** This is an Important, not a Critical, because (1) nothing in the
shipped/tested path fires a timer, so there is no live divergence today, and
(2) it is plausibly intended for the later run-control wiring bead (39w/4qo,
which y78 blocks). But the rail is the layer that *should* own this pairing,
and shipping it without even a stub leaves the "AUX records land during live
runs" claim only partially discharged at the layer that claims it.

**Recommendation (pick one, then document):**

- **Option A (preferred):** add to `DeviceRail`:
  ```rust
  /// Land the AUX TIMER_FIRE for a segment's fired timer and disarm the
  /// one-shot clock (§3.3 / §4). Pairs the record with the device-state
  /// change exactly as apply_pad_set pairs PAD_SET — the disarm and the
  /// record must both land or neither.
  pub fn log_timer_fired(&mut self, t: &TimerFired) -> Result<(), RecordError> {
      self.log
          .timer_fire(t.delivered_icount, 0, t.vector, t.armed_deadline_vns, t.delivered_icount)
          .map_err(RecordError::Log)?;
      let clk: &mut PvClock = self.device_mut(DEVICE_ID_PV_CLOCK, "pv-clock")?;
      clk.disarm(); // or whatever the one-shot disarm entry point is
      Ok(())
  }
  ```
  (Verify the `LogWriter::timer_fire` arg order — the first `icount` is the
  record's landing icount; `delivered_icount` is the payload field. They are
  equal for an at-deadline delivery but the API takes both.)

- **Option B:** if this is deliberately deferred to run-control wiring, add a
  `NOT HERE:` line to the module docstring next to the detcall note, stating
  that TIMER_FIRE pairing (record + disarm) is owned by the segment loop in a
  later bead, and update the y78 bead description to scope it out. Right now the
  docstring's "every ... AUX record lands" reads as a completeness claim the
  rail does not meet.

---

### I-2. The "atomically from the caller's perspective" pairing claim is documented but not enforced — drift on a post-mutation log failure depends on an unstated invariant

`crates/dh-vmm/src/recording.rs:150–171` (`apply_pad_set`),
`176–201` (`apply_net_rx`).

Both apply-methods mutate the device FIRST, then write the record:

```rust
let vector = pad.apply_pad_set(port, buttons).map_err(RecordError::Pad)?;   // device state changed
self.log.pad_set(icount, boundary_rip, port, buttons, frame_hint)
    .map_err(RecordError::Log)?;                                            // record may fail HERE
```

The doc says "apply to the pad latch AND write the record, **atomically from
the caller's perspective**" (recording.rs:150–152). It is not atomic: if
`log.pad_set` returns `WriteError`, the latch has already moved but no record
landed — an applied-but-unrecorded input, which is *exactly* the replay
divergence the pairing exists to prevent (the inverse of the module
docstring's "applied-but-unrecorded input is a replay divergence by
construction").

Two mitigating facts, neither of which the code makes load-bearing:

1. **The log writer leaves its buffer untouched on a failed append**
   (dhilog.rs:357 "a failed append leaves the buffer untouched"). So the *log*
   is consistent; only the device-vs-log relationship drifts.

2. `RecordError::Log` is documented "DATA_LOSS class, **slot-fatal**"
   (recording.rs:56) and `RecordError::Pad`/`Net` "slot-fatal"
   (recording.rs:52–55). **IF every `RecordError` unconditionally destroys the
   slot**, the drifted device state is never observed by a replay and the
   divergence is moot.

The problem: that "slot is always destroyed on `RecordError`" invariant is the
only thing making the non-atomic order safe, and it is asserted **only in a
doc comment on the enum**, not enforced by the type or even stated at the
apply-method call sites. The method returns `Result<Option<u8>, RecordError>`
to a caller that the compiler does not force to treat the error as fatal. A
future caller that logs-and-continues (e.g. "retry the segment", "skip this
input") would silently ship a divergent slot. The m1 template sidesteps this
because its log faults flow through `ctx.log_fault()` and are *always* mapped
to a `BoundaryError` that unwinds the whole segment (m1_acceptance.rs:242–244,
mirrored in `service_exit` recording.rs:127–130) — there, the loud-unwind is
structural. Here it is left to the caller's discipline.

Note the order question raised in the brief: **neither order is atomic.**
Logging first then applying would instead leave a logged-but-unapplied input
(a record for a latch change the guest never saw) — equally divergent.
Apply-first is the right choice *given* the slot-fatal invariant, because the
log stays internally consistent (untouched buffer) and the only casualty is a
device state that the dead slot will never replay.

**Recommendation:**

- Tighten the doc on `apply_pad_set`/`apply_net_rx` from "atomically from the
  caller's perspective" to something honest, e.g.: *"The device mutation
  happens before the record write. These are not transactional: a `Log`
  failure here leaves the device mutated with no record. This is sound ONLY
  because every `RecordError` is slot-fatal — the caller MUST destroy the slot
  on any `Err`, never retry or continue. See `RecordError`."*
- Strengthen the `RecordError` enum doc to state the invariant as a *caller
  contract*, not just a classification ("Any `RecordError` obligates the caller
  to fail the slot; the rail's pairing soundness depends on it").
- Consider (Suggestion S-4) a debug-only guard or a test that asserts a caller
  cannot observe a sealed-and-healthy log after a mid-apply `Log` failure.
