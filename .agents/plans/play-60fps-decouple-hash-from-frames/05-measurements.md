# Measurements

Timings only — no capture ids, no snapshot refs, no runtime roots
(privacy closeout, 04).

Harness: `cargo test -p dh-worker --release --test play_perf_smoke --
--ignored --nocapture` on the kvm-intel reference host with the DH_M9_*
artifacts staged. Phase A is the historical bridge play loop
(`Run{frame_budget=1}` + `GetFramebuffer`, one full-memory
`hash_final_stop` link per frame); Phase B is one `RunWithFrameCapture`
stream over the same instruction span.

## Host datum (2026-07-06)

- `b3sum --num-threads 1` over 128 MiB: ~50–60 ms — the floor for one
  full-memory chain link.

## M0/M2 harness results

### 2026-07-07, kvm-intel reference host, release build, real-emulator
workload image (dist 2ea42ad)

```
frames sampled (per-frame path):  240
instructions/frame:               27804563
epoch links in span (~50M grid):  133
per-frame Run avg/max:            242.63 ms / 927.91 ms
per-frame GetFramebuffer avg:     0.29 ms
per-frame path fps:               4.1
streaming: 239 frames in 28142.1 ms (first frame at 2.5 ms)
streaming fps:                    8.5
streamed/per-frame fps ratio:     2.06x
```

Attribution at ~27.8M instr/frame:

- guest execution: ~90–115 ms/frame — the dominant cost in BOTH paths;
- per-frame path adds one full-memory `hash_final_stop` link (~50 ms)
  plus RPC overhead per frame → 243 ms/frame;
- streaming path amortizes epoch links (0.55/frame × ~50 ms ≈ 28 ms/frame)
  on top of guest execution → ~118 ms/frame.

**Consequence for the 60fps target:** M2 delivers the expected 2x by
removing per-frame links, but the instructions-per-frame datum is ~28M —
the heavy branch of the M4 decision tree AND ~6x above what 60fps allows
for guest execution alone (60fps needs ≤16.6 ms/frame; 27.8M instr in
16.6 ms ⇒ ~1.7 GIPS sustained, ~6x this host's observed guest rate).
Epoch-hash mitigation (M4) is worth ~28 ms/frame (→ ~11 fps) and removes
multi-frame hiccups, but real-time 60fps requires reducing the in-guest
emulator's instructions-per-frame (reference-workload scope, not this
repo). Documented as the measured gap per plan 02 acceptance.

## M1 rollout

- 2026-07-07: runbook fixed (`docs/ops/rom-bridge-o73-ready-snapshot.md`,
  `docs/ops/m6-grpcurl-metrics-smoke.md`) to build `--release` and run
  the `target/release` artifacts; both live daemons were previously
  confirmed running from `target/debug`. `GetWorkerInfo.build_profile`
  and the serve startup log now report the profile.

## M4 decision

Populated from the instructions/frame datum above:

- ~1M instr/frame → epoch link every ~50 frames → amortized ~1 ms/frame
  → M4 unnecessary.
- ~10M+ instr/frame → epoch link every ≤5 frames → M4 (or `epoch_len`)
  on the critical path.

<!-- Record the decision and its numbers here. -->
