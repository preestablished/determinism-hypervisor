# Action Items

### Critical

_None._

### Important

_None._

### Suggestions

- [ ] **S1 — Assert `frames_elapsed` in the SDK live test.** In
  `next_sdk_event_stops_at_the_feeding_exit_live` (`crates/dh-vmm/src/runctl.rs`),
  add `assert_eq!(out.frames_elapsed, 0)` (the `pipeline_smoke` guest writes no
  FRAME_COUNTER) to pin that cross-mode frame counting does not leak into a
  non-frame run.
- [ ] **S2 — Collapse the duplicate `sdk_events.ok_or(...)` lookup** in
  `run_segment_with_epochs` (`crates/dh-vmm/src/runctl.rs:277-284`): bind the cell
  once, then use `(cell, cell.get())`. Behavior-neutral readability fix.
- [ ] **S3 — Document the hard-cap-default boundary.** Add one line to the runctl
  module docstring / near the `Until::{NextSdkEvent,FrameBudget}` arms
  (`crates/dh-vmm/src/runctl.rs:264-265`) stating that `hard_cap` is taken
  literally and the proto `0 ⇒ 10e9` default is the worker's `RunRequest → Until`
  job, never runctl's — so no caller wires a raw proto `0` into runctl.
- [ ] **S4 — (Optional) Factor out the `event_until_tests` Segment scaffolding**
  into a small builder to cut the repeated 12-field literal across the five tests.
- [ ] **S5 — (Optional) Unit-pin the frame-mark GPA.** A non-KVM test asserting
  `dh_devices::pad::PV_PAD_BASE + dh_devices::pad::REG_FRAME_COUNTER == 0xD000_101C`
  guards against a silent device-constant move that still lands inside the test's
  MMIO window.
