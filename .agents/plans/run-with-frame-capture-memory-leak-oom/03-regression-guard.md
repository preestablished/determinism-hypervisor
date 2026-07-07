# 03 — Regression Guard: RSS Ceiling AND Plateau Over A Multi-Minute Streaming Run

Phase4 item 3, verbatim requirement: a CI-runnable (or documented
lab-lane) test driving a multi-minute streaming Run that fails if
(a) worker RSS exceeds a derived ceiling, **or** (b) RSS fails to
plateau. The ceiling catches the loud 300–500 MB/s leak; the plateau
catches a 1–5 MB/s slow leak that a generous ceiling would let survive
long enough to kill a 4-hour Phase-5 soak.

## Test Shape

- **Where:** `crates/dh-worker/tests/` alongside the other
  long-running `--ignored` lab-lane tests (exemplars:
  `m9_ready_handoff.rs`, `frame_capture_stream.rs`,
  `play_perf_smoke.rs` — follow their pattern: `#[ignore]`, release
  profile, gated on artifact availability via the `DH_M9_*` env vars
  per `docs/phase-2-exit-gate.md`, with a documented invocation line).
  Note: there is no `determinism-tests` crate despite older session
  notes referencing one — `dh-worker` is the integration-test home. If a
  synthetic workload (tight guest loop dirtying pages + FRAME_COUNTER
  increments) reproduces the growth in 01, prefer it — it removes the
  M9-artifact dependency and can run un-ignored in CI at a shorter
  duration. Decide based on what 01's repro actually needed; write the
  decision into the test's module doc.
- **Drive:** one `RunWithFrameCapture` stream (in-process service or
  gRPC against a spawned worker — match how 01's repro drove it) with a
  large icount budget, consuming frames paced at ~60 Hz, for ≥3 minutes
  wall time (lab lane) / a duration CI can afford if the synthetic repro
  is fast (state both numbers in the test).
- **Measure:** sample the worker process's RSS (`/proc/self/status`
  `VmRSS` in-process, or `/proc/<pid>/status` if driving a spawned
  worker) every ~1 s into a `Vec<(elapsed, rss_kb)>`. On failure, dump
  the full series into the test log — the series is the diagnostic, not
  just the verdict.

## The Two Assertions

1. **Ceiling.** `max(rss) <= BOUND` where `BOUND` is derived
   mechanically, not guessed:

   ```
   BOUND = slots × mem_bytes            (guest RAM per configured slot)
         + slots × shadow_cost          (0 today; +mem_bytes per slot if
                                         02 lands the M4 shadow copy)
         + baseline_rss                 (measured at "serving, idle" —
                                         record the measurement and build)
         + margin                       (allocator slack + stream buffers;
                                         propose 25% of the above, state it)
   ```

   Write the derivation and each input's source (config field, measured
   value + where measured) in a comment block above the constant. The
   bound must NOT scale with run duration or epoch count — that is the
   entire point.

2. **Plateau.** Parameterize by run duration `T`: warm-up window ends
   at `max(60 s, T/3)`; let `rss_warm` = windowed median of RSS over
   the last 10 s of the warm-up window, and `rss_final` = windowed
   median (or p90 — pick one, state it) over the final third of the
   run. Assert `rss_final <= rss_warm × (1 + P)` with `P` a stated
   small percentage (propose 10%; tune against the post-fix profile
   from 01 so the test is not flaky, and record the observed post-fix
   drift that justified the choice). Medians-of-windows, not single
   samples or a raw max — one transient allocator spike must not fail
   the run. If a short-CI variant runs under ~2 minutes, apply only
   the ceiling assertion there and leave the plateau to the lab lane;
   the warm-up is deliberate — page-cache warm-up, lazy allocations,
   and lz4/stream buffers should all be paid before it ends.

## Determinism Cross-Check (Do Not Skip)

This guard runs alongside — not instead of — the hash-chain bit-identity
check from `02-fix-design.md`. Concretely, at the fix commit:

1. Existing record/replay determinism gates green (3+ consecutive full
   workspace runs — repo standing rule for hash-sensitive changes).
2. One pre-fix recording (produce it BEFORE landing the fix; stash the
   sealed DHILOG + its chain values in the evidence dir) replayed on the
   post-fix build yields bit-identical epoch chain values.

## Flakiness Budget

RSS is host-noisy. Mitigations, in order of preference: assert on
`VmRSS` deltas from the test's own baseline rather than absolutes where
possible (the plateau assertion above already uses windowed medians);
and if the lab host shares load, document a rerun policy on the bead
rather than loosening `P` past 15%.
Do not paper over a failure by widening bounds without a profile showing
the growth is benign and bounded.
