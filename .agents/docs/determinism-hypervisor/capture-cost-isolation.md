# Capture Cost Isolation — feature-only vs framebuffer-lz4 (bead `uyhu`)

Date: 2026-07-16. Host: `infra-control` (Intel i5-8400, 6 cores, determinism
class verified). Method of record: the three-variant TakeSnapshot delta block
in `crates/dh-worker/tests/capture_engine_real_image.rs` (client-side RPC
time, `seal_input_log=false`, 100 iterations per variant, interleaved
full → features-only → no-capture per iteration, p50/p95 percentiles, signed
p50 deltas), run `--release` under `taskset -c 2-5` against the real workload
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

**Feature-only capture cost is ≈ 50–250 µs p50 — comfortably inside scorer
M4's 1.5 ms p50 budget** (≥ 6× headroom even on the worst loaded-host
replicate, ~30× on the quiet-host primary run). The full-capture delta is
consistently an order of magnitude larger than the feature-only delta and is
framebuffer-lz4-attributable, matching the payload asymmetry: 591 packed
feature bytes vs a 229,376-byte compress. No follow-up optimization request
is needed for the feature path.

## Caveats

- Delta method, not a microbenchmark: TakeSnapshot machinery (p50 ~37 ms
  quiet, ~60–70 ms loaded) dominates the absolute numbers. The deltas, not
  the absolutes, are the finding.
- Deltas are differences of medians of separately-timed populations, so they
  can go negative under load (run 2's feature delta) — that reads as "below
  the noise floor", not as negative cost. p95s are contamination-prone and
  must not be quoted as capture cost.
- fb-lz4 absolute cost is load-sensitive (462 µs quiet vs ~3 ms loaded);
  scorer M4 sizing should use the quiet-host figures.
- Runner-reservation caveat: the `kvm-intel` actions runner service could
  not be paused (no passwordless sudo in this session); verified no queued
  or in-progress GitHub runs during the measurement windows.
