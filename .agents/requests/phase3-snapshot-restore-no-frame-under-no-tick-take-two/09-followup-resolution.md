# Followup Resolution: Frame Hard Caps Retuned From Fresh Measurement

Closes `08-followup-frame-hard-cap.md` (2026-07-07, bead
`determinism-hypervisor-ego1`; retune commit `92bb674`, IO-frame cap
correction in the same-day review-fix commit).

## Fresh measurement (not the trail's ~25M or 27.8M — re-derived)

Against reference-workload dist `workload-image-0.1.0` (built_from
`7b0c7b2`, includes the `40eaf4f` fix), gate
`linux_m5_frame_budget_records_post_ready_frame_marks` with the cap
temporarily at 500M, two consecutive runs bit-identical:

- fresh frames: 7.77M (f1, post-READY warmup), 27.68M (f2), 27.68M (f3)
- post-restore: 27.68M (f4), 27.68M (f5)
- **max = 27,675,634 instr/frame**; NOP-game diagnostic frame 9.2M

## Retuned values (derivation comments in the code)

- `FRAME_HARD_CAP` = **150,000,000**
  (`crates/dh-worker/tests/m5_frame_scheduling.rs`): 27.7M × 3 frames
  (FIRST_FRAMES) × ~1.8 margin; ratio to measured cost 1.81× (≤4× ✓).
- `LINUX_FRAME_HARD_CAP` = **30,000,000**
  (`crates/dh-worker/tests/m5_net_loopback.rs`): the IO frame itself
  measured **7,768,576** instructions (io_frame_cost, identical across
  two runs); 7.77M × 1 frame (LINUX_IO_FRAMES) × ~3.9 margin; ratio
  3.86× (≤4× ✓), and the cap still covers a steady-state-cost frame
  (~27.7M) if the IO ever moves past the warmup frame. (First landed as
  60M off the frame-scheduling 27.7M proxy in `92bb674`; the review pass
  measured the IO frame directly — 60M would have been 7.7×, over the
  ≤4× bound — and corrected it.)
- `DETCHANNEL_FRAME_HARD_CAP` = 1,000,000 **unchanged**: the detchannel
  test builds a synthetic VM (nanokernel `detchannel_frames_elf`,
  tempdir cache) and never loads the M9 Linux image — it does not gate
  the real-emulator path. Verified by reading its VM setup
  (`m5_frame_scheduling.rs` detchannel test).

Design decision: one conservative constant per file (safety net), not
image-profile-aware caps — no fixture test relies on the cap firing, and
the fixture path is unaffected by a larger net.

## Green evidence

`target/frame-cap-retune-20260707T200907Z/` (`00-evidence.md` therein):
3 consecutive green runs of the linux_m5 frame-scheduling gate with the
final caps, identical frame tables, stop reason BUDGET_REACHED (not
HARD_CAP); both `verify_replay_done` replay legs green each run; fixture
suites pass unchanged without staged artifacts.

## Known limitation (filed, not blocking this closure)

`m5_net_loopback`'s Linux test passes its Run leg (BudgetReached under
the new 60M cap) but then fails the fixture-era `PVBLKIO1` meta-proof
assertion — the real refwork-harness never writes that proof (its meta
layout has cart-hash bytes at meta[32..56]). The same fixture-era
staleness affects the m5_record_replay Linux corpus and m7_fork_verify
Linux paths. Filed as bead `determinism-hypervisor-jyo7` with full
diagnosis; needs a cross-repo proof contract with reference-workload.
