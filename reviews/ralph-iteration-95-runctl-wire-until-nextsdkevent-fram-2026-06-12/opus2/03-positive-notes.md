# Positive Notes

Specific things this change did well.

## P1. Reuses the proven HLT sentinel-unwind instead of inventing a new stop mechanism
The event stops piggy-back on the exact mechanism terminal HLT already used: set a flag
in the `exits!` wrapper, return a sentinel `Err`, catch it at every boundary-engine call
site. Generalizing `finish_halted` → `finish_at_counter(reason, …)` rather than adding a
parallel path means the event stops inherit HLT's already-tested boundary-stability
property "for free" (the not-yet-retired-exit invariant). This is the right design choice
and the module doc (runctl.rs:324–329) states the invariant explicitly.

## P2. The "stop ON the triggering exit, after servicing it" ordering is correct and documented
`on_exit(exit)?` runs BEFORE the frame/sdk flags are checked (runctl.rs:342–355), so the
rail logs FRAME_MARK / drains the doorbell at the exit's icount and the device state is
current at the stop boundary. ARCH §6.6's FRAME_MARK consistency rule and the "the record
is written, then we stop" expectation are honored. The comment at runctl.rs:326–329
captures exactly why.

## P3. `MissingSdkEventFeed` fails loud instead of silently spinning to the cap
The author recognized that a NextSdkEvent run without a feed would otherwise walk all the
way to `hard_cap` and report `HardCap` — a silently-wrong reason. Erroring early
(runctl.rs:279) is the correct call, the doc comment (runctl.rs:97–101) explains the
reasoning, and there is a dedicated test (`next_sdk_event_without_a_feed_fails_loudly`).

## P4. Baseline read once; strict-rise stop is robust to multi-event drains
Reading the feed baseline at segment start and stopping on `cell.get() > baseline`
(runctl.rs:350–353) correctly handles a cumulative cross-segment counter AND multiple
matching events drained in a single doorbell exit — both collapse to one stop. The inline
comment (runctl.rs:280–281) names the cross-segment-cumulative concern directly.

## P5. Exhaustiveness used as a compile-time forcing function — and the doc was updated honestly
`proto_map.rs` deliberately has no `_ => Unspecified` arm, so adding `StopReason::NextSdkEvent`
broke compilation and forced the mapping decision at the same commit. The module doc was
updated to record that this is exactly how the variant landed (proto_map.rs:9–11, 32–34:
"`NextSdkEvent` landed exactly that way with bead 4qo"). Both the proto-name and the
byte-pin (`(R::NextSdkEvent, 3)`) tests were extended.

## P6. Live tests cover the real risk surface, not just the happy path
The `event_until_tests` module asserts: replay-identical boundary for both modes
(`run()` twice, compare `(icount, rip, state_hash)`), the `FrameBudget(0)` start-boundary
no-entry case (`boundary.icount == 0`), and — importantly — both `hard_cap` safety-net
paths (`frame_budget_without_frames_hits_the_hard_cap_live`,
`next_sdk_event_without_events_hits_the_hard_cap_live`), which pin that a never-triggering
run stops at the cap rather than spinning. The hard-cap tests assert the exact cap icount.

## P7. The `replay_engine.rs` hardcoded `frames_elapsed: 0` is correct AND annotated
Rather than leaving the reader to wonder why replay reseals with `frames_elapsed: 0`, the
comment (replay_engine.rs:365–368) states that `seal()` does not read the field (frame
marks travel as AUX FRAME_MARK records, not END fields) so any value reseals identical
bytes. This pre-empts exactly the question a reviewer would raise.

## P8. Frame decode uses the device crate's own constants, not magic numbers
`frame_mark_gpa = dh_devices::pad::PV_PAD_BASE + dh_devices::pad::REG_FRAME_COUNTER`
(runctl.rs:272) sources the GPA from the device definition, so a future pad-layout change
propagates automatically instead of drifting against a hardcoded `0xD000_101C`.
