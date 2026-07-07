# Follow-up: linux_m5 FRAME_HARD_CAP is fixture-calibrated

From rom-operator-bridge, 2026-07-06, after landing the reference-workload fix
(`refwork-4qj`, commit `40eaf4f`: `refwork-harness` now uses `SdkPlatform`, so
the real emulator publishes ring-W `FrameMark` + pv-pad `FRAME_COUNTER`).

## Observation

With the fixed image, `linux_m5_frame_budget_records_post_ready_frame_marks`
@ `ecd60ae` now **produces frames** but still reports `HARD_CAP` at its default
cap:

```
ready_frame_counter=0  post_run_frame_counter=Some(2)
post_run_icount - ready_icount = 50,000,000  (== FRAME_HARD_CAP)
```

The real emulator costs **~25M instructions per frame** (a full game frame),
whereas the `m9-refwork-contract` fixture is **~150K/frame**. `FRAME_HARD_CAP =
50_000_000` (`m5_frame_scheduling.rs:47`) was calibrated for the fixture, so it
covers only ~2 of `FIRST_FRAMES = 3` real frames → `HARD_CAP`, not
`BUDGET_REACHED`.

## Confirmation

Temporarily raising `FRAME_HARD_CAP` to `500_000_000` (nothing else changed),
the test **passes fully** against the real emulator:

```
first_frames=[(8724677,1),(33929802,2),(57954297,3)]     (fresh boot)
restored_frames=[(24024950,4),(48049735,5)]              (after restore)
test result: ok. 1 passed
```

`BUDGET_REACHED`, the strict deterministic frame table, and restore continuity
all hold — so the worker-side drain fix (`4b19c52`) + the reference-workload
platform fix are together sufficient. The only remaining gate is the test's own
cap.

## Ask

Raise `FRAME_HARD_CAP` (and, if it gates the same real-emulator path,
`DETCHANNEL_FRAME_HARD_CAP`) so `FIRST_FRAMES` + `AFTER_RESTORE_FRAMES` real
frames fit with margin — e.g. `~25M * frames * 2`. A per-frame budget of
`~100M` (i.e. `FRAME_HARD_CAP ≈ 150_000_000` for `FIRST_FRAMES = 3`) is
comfortable at the observed ~25M/frame. This is a determinism-hypervisor
test-tuning change; it is not a workload or worker defect. Once retuned,
`linux_m5` against the real-emulator initramfs is a durable green gate for this
whole path.

Tracking: rom-operator-bridge-9xo. reference-workload fix: `40eaf4f`.
