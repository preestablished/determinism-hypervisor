# Iteration 39 — PMI Skid Histogram + margin/2 Gate — Review Overview

- **Reviewer:** Claude Opus
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-39-skid-histogram` vs `main`
- **Bead:** determinism-hypervisor-19l
- **Environment:** lab box, `/dev/kvm` rw, `perf_event_paranoid=1` — everything run live (CLI 5×, suite 2×, clippy, fmt)

## Verdict: APPROVE

A clean, well-scoped, normatively-faithful iteration. The skid histogram lands exactly where ARCH §9 asks for it ("PMI skid histogram" Prometheus metric), the gate enforces the R1 alert threshold (max < skid_margin/2) with the correct strictness and empty-fails semantics, and the embedded-guest CLI driver is self-contained and live-stable. The pid-as-tid fix is correct, complete, and well-justified; no remnants remain. No Critical or Important issues found. A handful of low-value Suggestions only.

## What it does

- `crates/dh-verify/src/skid.rs` — `SkidHistogram` over `BTreeMap<u64,u64>` buckets, deterministic plain-text artifact + Prometheus exposition (cumulative `le` buckets, `+Inf`, `_sum`, `_count`), strict `assert_margin` (`max < margin/2`; empty histogram FAILS).
- `tools/dh-cli/src/skid.rs` + `dh-cli skid [--samples N]` — boots `landing_loop` (cmdline `4e9`), per sample arms the PMI exactly `period` ahead (cycling 100k/50k/25k/10k, throttle-safe), runs to the kick EINTR, records `after − (before+period)`, parks the period between samples, and errors loudly on a kick *before* the armed point.
- `dh-vmm::run::current_tid()` — safe `gettid()` wrapper replacing the pid-as-tid hack that silently misrouted PMI kicks from worker threads.

## Live results (this box)

| Check | Result |
|---|---|
| `dh-cli skid --samples 100` × 5 | All GATE OK, max skid **31** < 4096; **zero** stale-signal errors |
| Run-to-run stability | `sum=2931` **identical** across all 5 runs; band **27..31** stable |
| Distinct buckets | only `{27, 30, 31}` populated (discrete delivery latency) |
| `skid_gate` integration test (50 samples) | pass (run twice — no flake) |
| `dh-verify` skid unit tests (3) | pass |
| `dh-vmm` run.rs PMI kick live tests (2) | pass |
| `cargo clippy` (dh-cli, dh-verify, dh-vmm, all-targets) | clean, zero warnings |
| `cargo fmt --all --check` | clean |

## Stats

- Files changed: 10 (+313 / −9)
- New code: `crates/dh-verify/src/skid.rs` (150), `tools/dh-cli/src/skid.rs` (84), `tools/dh-cli/tests/skid_gate.rs` (29)
- Critical: 0 · Important: 0 · Suggestions: 4 · Positive notes: 7
