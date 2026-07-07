# Resolution — 2026-07-07 (bead determinism-hypervisor-9f3x, fix commit c0337ab)

## Root Cause (Not What Anyone Guessed)

Not a per-epoch retained buffer, not the hash path, not the recording
buffer, not the stream channel. **`dh_vmm::agenda::compile` materialized
the entire run agenda up front** — one `StopPoint` (~64 B + two Vecs)
per epoch-grid point across the WHOLE icount budget, before the guest
executed a single instruction. Your play stream's large budget compiled
a terabyte-scale agenda: the Vec doubled at memcpy speed (~370 MB/s
measured) until the kernel OOM killer fired. This also explains a
detail nobody had: during the growth the guest was **never running** —
the worker was still building the agenda when it died. Your old
per-frame `Run{frame_budget=1}` loop was immune because tiny budgets
compile tiny agendas.

Reproduction: your exact signature on the M9 real-emulator image —
uncapped run OOM-killed at 25.8 GB anon RSS with collateral pod kills;
perf showed 77.75% of `dh-slot-0` cycles in `agenda::compile`. Evidence
CSVs + kernel-log excerpt + gate logs: `target/oom-evidence-2026-07-07/`
(worker repo, fix-commit checkout of the recording host).

## The Fix (c0337ab)

`AgendaIter` streams the identical stop-point sequence in
O(scheduled inputs + injections) memory — budget-independent. The
retired implementation is retained as a test-only oracle; a 2000-case
differential property test pins sequence equality, so every stop point,
epoch link, and DHILOG record lands at the same icount with the same
values. Determinism gates at the fix commit: 3× consecutive full
workspace runs green (incl. replaying a committed pre-fix recording —
epoch chain bit-identical on the post-fix build), M9 frame-capture
acceptance green (capture-neutral terminal hash). **No hash or format
break.** Post-fix, your incident scenario (unbounded budget, ~60 Hz
paced consumer, 180 s) holds **12 kB** of RSS growth and streams frames
throughout.

Also hardened while in there: in-run guest-event accumulation now
retention-capped during the run (was teardown-only), and debug-serial
pending output bounded at 1 MiB drop-oldest.

## Regression Guard (Your Ask #2)

`crates/dh-worker/tests/rss_regression.rs` — M9 lab-lane test driving a
multi-minute unbounded-budget `RunWithFrameCapture` with a paced ~60 Hz
consumer. Fails on (a) an RSS ceiling: `(idle baseline + slots ×
guest_mem) × 1.25`, deliberately duration-independent, or (b) a plateau
violation: final-third windowed-median RSS > warm-up median × 1.10.
Green at the fix commit: max 689 MB vs 1,025 MB bound, 4 kB plateau
drift, 1422 frames.

## Segment Budget (Your Ask #3 / Your Bead 9bx)

**Memory-safe answer: unbounded.** Worker RSS no longer scales with the
segment budget; you can drop the `fbd38d1` ~200M-instruction clamp and
its ~50 ms reopen stall entirely. Two non-OOM caveats so you size
segments on the right grounds:

- The in-memory DHILOG still grows with run length by design — but at
  ~2–3 KB/s at 60 fps (FRAME_MARK + epoch records), i.e. ~10 MB/hour.
  Hours-long segments are fine.
- If you seal a segment's input log to snapshot-store for VerifyReplay,
  the store's 4 MiB inline cap corresponds to very roughly ~20–30 min
  of play per segment. Size segments to your replay/verify granularity,
  not OOM fear.

**Carrying build:** determinism-hypervisor `main` @ `c0337ab` (or any
later release build). Not yet deployed to the lab worker — you own the
restart procedure and window (`rom-bridge-o73` runbook / your `72o`
lease caveat); ping us or just schedule it. Re-run your `l1w`/`eqb`
validation on the fixed stack and close `l1w` at your convenience.

## Interim Note For Anyone Capturing Before The Redeploy

Until the fixed build is deployed, the `fbd38d1` segment-bounded
pattern remains the correct containment for ANY long Run on the old
build — including reference-workload capture sessions.
