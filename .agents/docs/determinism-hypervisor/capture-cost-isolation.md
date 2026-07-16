# Capture Cost Isolation — feature-only vs framebuffer-lz4 (bead `uyhu`)

Date: 2026-07-16. Host: `infra-control` (Intel i5-8400, 6 cores, determinism
class verified). Method of record: the three-variant TakeSnapshot delta block
in `crates/dh-worker/tests/capture_engine_real_image.rs` (client-side RPC
time, `seal_input_log=false`, 100 iterations per variant, interleaved per
iteration, p50/p95 percentiles, signed p50 deltas), run `--release` under
`taskset -c 2-5` against the real workload
image (`reference-workload/dist/workload-image-0.1.0`, initramfs blake3
`36f50484…`, 12-range spec, 591-byte packed feature payload, 229,376-byte
framebuffer).

## Why this exists

The 2026-07-08 capture proof measured a with-vs-without-capture TakeSnapshot
delta of ~1.9 ms p50 — a noisy upper bound that included lz4-compressing the
full framebuffer every iteration, sitting just above state-scorer M4's
**1.5 ms p50 budget**. Bead `uyhu` asked to separate feature-byte extraction
cost from framebuffer-lz4 cost. This is measurement + documentation only; no
production code changed (`capture_at_boundary` in
`crates/dh-worker/src/service.rs` is untouched).

## Measurements

Three accepted runs, all pinned `taskset -c 2-5` (slot cores) at normal
priority. Runs 1–2 ran against heavy niced background load (sibling
`replay-renderer` ffmpeg encode tests; load avg ~14–22 on 6 cores); run 3 ran
after that load drained (load avg ~4–7) and is the primary evidence — its
baseline p50 (~36.9 ms) is closest to the 2026-07-08 proof's host state
(~44 ms). A preliminary unpinned run under full load was rejected as swamped
(baseline p50 measured slower than full capture; every delta clamped to 0 —
that observation motivated the switch to signed deltas in the test).

| run | conditions | variant | p50 (µs) | p95 (µs) |
|-----|-----------|---------|----------|----------|
| 1 | loaded | full capture | 70,640 | 176,999 |
| 1 | loaded | features-only | 67,654 | 155,188 |
| 1 | loaded | no capture | 67,412 | 157,733 |
| 2 | loaded | full capture | 68,099 | 139,751 |
| 2 | loaded | features-only | 60,968 | 136,038 |
| 2 | loaded | no capture | 64,963 | 177,824 |
| **3** | **quiet (primary)** | full capture | 37,429 | 89,505 |
| **3** | **quiet (primary)** | features-only | 36,967 | 87,328 |
| **3** | **quiet (primary)** | no capture | 36,917 | 106,044 |

Derived p50 deltas (signed):

| delta | run 1 | run 2 | **run 3 (primary)** |
|-------|-------|-------|---------------------|
| features-only − baseline (**feature-only cost**) | +242 µs | −3,995 µs (noise inversion) | **+50 µs** |
| full − baseline (full capture cost) | +3,228 µs | +3,136 µs | **+512 µs** |
| full − features-only (fb-lz4-attributable) | +2,986 µs | +7,131 µs | **+462 µs** |

## Verdict

**Feature-only capture cost is +50 µs p50 on the quiet primary run — ~30×
inside scorer M4's 1.5 ms p50 budget.** The verdict rests on run 3 alone: the
loaded replicates are directionally consistent (+242 µs / noise-inverted) but
non-probative against a 1.5 ms budget, because run 2's −3,995 µs inversion
shows their median-of-medians noise is on the order of ±4 ms — larger than
the budget itself. Run 3 is resolving real signal at the ~100 µs scale: its
fb-lz4 delta of +462 µs is physically plausible for a 229 KB lz4 compress,
and its full-capture delta is an order of magnitude above the feature-only
delta, matching the payload asymmetry (591 packed feature bytes vs a
229,376-byte compress). No follow-up optimization request is needed for the
feature path.

## Caveats

- Delta method, not a microbenchmark: TakeSnapshot machinery (p50 ~37 ms
  quiet, ~60–70 ms loaded) dominates the absolute numbers. The deltas, not
  the absolutes, are the finding.
- Deltas are differences of medians of separately-timed populations, so they
  can go negative under load (run 2's feature delta) — that reads as "below
  the noise floor", not as negative cost. p95s are contamination-prone and
  must not be quoted as capture cost.
- Recorded runs 1–3 used a fixed intra-iteration order
  (full → features-only → no-capture), which biases deltas *downward*: the
  predecessor warms caches for the next variant, and the always-last baseline
  sits latest in any delta-chain drift. The committed instrument now rotates
  the order per iteration (review finding, landed after these runs). The bias
  cannot threaten the verdict — even an order-of-magnitude understatement of
  +50 µs clears the 1.5 ms budget.
- A post-rotation validation run came back green (instrument works) but its
  numbers were noise-dominated by concurrent host load (features-only median
  above full capture, negative fb-lz4 delta — unphysical) and are recorded as
  validation only, not evidence. It is a live demonstration of the median-
  noise caveat above.
- fb-lz4 absolute cost is load-sensitive: 462 µs quiet vs ~3–7 ms loaded
  (run 2's 7.1 ms is inflated by the same noise that inverted its feature
  delta — the two deltas share `feat_p50`, so noise moves them in opposite
  directions). Scorer M4 sizing should use the quiet-host figures.
- Runner-reservation caveat: the `kvm-intel` actions runner service could
  not be paused (no passwordless sudo in this session); verified no queued
  or in-progress GitHub runs during the measurement windows.
