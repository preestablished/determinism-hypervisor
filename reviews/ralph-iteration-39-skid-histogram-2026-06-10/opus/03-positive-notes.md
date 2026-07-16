# Positive Notes

## P1 — The pid-as-tid fix is exactly right, and the bug story is real

The original `std::process::id() as i32` worked only on the main thread (pid == tid there) and would silently misroute PMI kicks from any worker thread. `current_tid()` (`crates/dh-vmm/src/run.rs:129`) puts the one unsafe `gettid()` syscall behind a safe wrapper in the crate that already owns `#[allow(unsafe_code)]`, so dh-cli's `#![forbid(unsafe_code)]` stays intact. The doc comment captures the precise failure mode. Both call sites fixed, helper deleted, no remnants — a clean, complete fix.

## P2 — Strict, empty-fails gate matches the normative R1 rule precisely

`assert_margin` enforces `max < margin/2` strictly and treats an empty histogram as a failure with a sentinel `u64::MAX`. This is exactly the bead's "no data is not a pass" contract and ARCH §3.2 / risk R1's "alert at margin/2, then re-baseline." The `MarginViolation` Display message even spells out the re-baseline action.

## P3 — Determinism is enforced by construction

`BTreeMap` ordering means every artifact and Prometheus export is byte-stable regardless of insertion order. The unit test pins exact output strings. Live confirmation: `sum=2931` was bit-identical across 5 independent CLI runs.

## P4 — Correct cumulative Prometheus histogram

Cumulative `le` buckets, `+Inf == count`, `# TYPE … histogram`, `_sum`, `_count` — a textbook-correct exposition that drops straight into the ARCH §9 metrics surface. The implementation is tiny and obviously correct.

## P5 — Throttle-safe period spread

Cycling 100k/50k/25k/10k (all ≥ 10k) deliberately stays clear of `perf_event_max_sample_rate` throttling — the iteration-16 hazard, called out in the module doc. The spread also samples skid across multiple period magnitudes, which strengthens the "skid is period-independent" claim.

## P6 — Loud, falsifiable stale-signal guard

`after < armed_point` is a real assertion, not a comment: it converts the stale-kick hazard into a hard error. Across 5 live runs it never fired, which both validates the hazard analysis and demonstrates the harness is not flaky.

## P7 — Self-contained, live-verified deliverable

Promoting nanokernel from dev-dependency to a regular dependency is the right call: `skid.rs` embeds `landing_loop_elf()` at runtime, so the guest must ship with the binary. The result is a `dh-cli skid` that boots, measures, gates, and prints Prometheus-ready output with no external files — and it ran green five times in a row on the box. dh-worker remains correctly absent from the dependency set.
