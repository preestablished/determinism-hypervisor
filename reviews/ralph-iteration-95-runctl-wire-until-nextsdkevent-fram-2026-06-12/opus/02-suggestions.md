# Suggestions (non-blocking)

## S1 — Pin `frames_elapsed` in the NextSdkEvent live test

`next_sdk_event_stops_at_the_feeding_exit_live` (`runctl.rs:~1255`) asserts the
reason, the run-twice identity, and `icount < cap`, but never inspects
`out.frames_elapsed`. Since `frames_seen` is now counted in EVERY mode, an
assertion (even `assert_eq!(out.frames_elapsed, 0)` for the `pipeline_smoke`
guest, which writes no FRAME_COUNTER) would pin that the cross-mode counting
does not leak frame marks into a non-frame run. Cheap insurance for the field's
"in EVERY until-mode" contract.

## S2 — Drop the duplicate `seg.sdk_events.ok_or(...)` lookup

`runctl.rs:277-284` calls `seg.sdk_events.ok_or(RunError::MissingSdkEventFeed)?`
twice to build the `(cell, baseline)` tuple. It is correct (the cell is `Copy`-ish
`&Cell`) but reads as if it might double-fetch. A single bind is clearer:

```rust
let sdk_feed = match until {
    Until::NextSdkEvent { .. } => {
        let cell = seg.sdk_events.ok_or(RunError::MissingSdkEventFeed)?;
        Some((cell, cell.get())) // baseline at segment start
    }
    _ => None,
};
```

Behaviorally identical; just removes the second `ok_or`.

## S3 — State the hard-cap-default boundary in the runctl docstring

API.md and the proto say `hard_icount_cap 0 ⇒ worker default (10e9)`, and the
prompt confirms that mapping is the WORKER's job, not runctl's. runctl treats
`hard_cap` as a literal `FinalStop::HardCap(hard_cap)` (`:264-265`), so a literal
`0` would stop immediately. One line in the module docstring or near those arms —
"`hard_cap` is taken literally; the `0 ⇒ 10e9` default is applied by the worker's
`RunRequest → Until` mapping, never here" — would prevent a future caller from
wiring a raw proto `0` straight into runctl.

## S4 — `event_until_tests` boilerplate is heavily repeated

Each of the five tests rebuilds the same `Segment { … }` literal (12 fields) and
`cfg()`/`chain`/`pause` scaffolding. A small `fn seg_for<'a>(...)` or a closure
builder would shrink the module and make the next stop-mode test a few lines.
Optional — the existing tests in this file already follow the verbose pattern, so
matching it is defensible for local consistency.

## S5 — Consider asserting the frame-mark GPA decode in a unit test

The frame decode hinges on `PV_PAD_BASE + REG_FRAME_COUNTER` (`:272`). The live
tests exercise it end-to-end, but a tiny non-KVM unit test asserting
`frame_mark_gpa == 0xD000_101C` (the current constant sum) would catch a silent
device-constant move that happens to still land inside the `pad_serial_exits`
0x1000 window and thus not fail the live test. Low value, but the determinism
product makes "the stop GPA is exactly this" worth a cheap pin.
