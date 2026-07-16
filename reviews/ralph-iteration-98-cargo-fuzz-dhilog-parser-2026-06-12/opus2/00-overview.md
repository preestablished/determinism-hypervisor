# Review Overview — cargo-fuzz DHILOG parser

- **Branch:** `ralph/iteration-98-cargo-fuzz-dhilog-parser` vs `main`
- **Date:** 2026-06-12
- **Reviewer:** Claude Opus (2nd reviewer)
- **Lens:** subtle logic errors, missing edge cases, implicit assumptions, long-term maintainability, CI ergonomics
- **Stats:** 5 files, +121/-10, 1 commit (`77c9df6`)

## What this change does

Adds a `cargo-fuzz` crate for the DHILOG v1 read path:

- `crates/dh-inputlog/fuzz/Cargo.toml` — standalone (own `[workspace]`) libFuzzer crate, kept out of the main build graph.
- `crates/dh-inputlog/fuzz/fuzz_targets/dhilog_parse.rs` — drives arbitrary bytes through `LogReader::parse`, then exercises every record accessor (`kind`/`rflags`/`seq`/`icount`/`boundary_rip`/`is_aux`/`body`), the `canonical`/`aux` iterators, and `end()`. Invariant: no panics, no unbounded allocation on hostile input.
- `.github/workflows/nightly-drift.yaml` — new `dhilog-fuzz` job (1h nightly on a hosted runner; operator `workflow_dispatch` can set `fuzz_seconds=86400` + `fuzz_runner=kvm-intel` for the 24h M5-accept run). Failure alerting and the `alert-on-failure` `needs` chain are updated to include the new job.
- `docs/ops/github-runner.md` — corrects the "pre-staged, not yet exercised" note now that cargo-fuzz is genuinely exercised.

## Verification performed (not assumed)

Run locally with `cargo-fuzz 0.13.2` + installed `nightly` toolchain:

- **Build + 716k-run smoke** of the verbatim CI command (`cargo +nightly fuzz run dhilog_parse fuzz/corpus/dhilog_parse tests/fixtures -- -max_total_time=... -rss_limit_mb=4096`): builds clean, **no crashes**, cov plateau ~351. The CI invocation is correct as written; cargo-fuzz auto-creates the corpus dir if absent, so a missing `fuzz/corpus/dhilog_parse` is not a failure.
- **Overflow-checks question resolved empirically.** `cargo fuzz build -v` shows cargo-fuzz injects `-Cdebug-assertions` into the instrumented target build (the `debug=1`-only `Cargo.toml` is the correct, complete template). A standalone `rustc -O -Cdebug-assertions` repro confirms integer-overflow checks follow `debug-assertions` and **panic on overflow** in this configuration. So integer-overflow bugs *are* caught by this fuzz build — no `[profile.release] overflow-checks = true` needed.
- Read `reader.rs` (549 lines) and `splice.rs` (298 lines) in full to confirm every accessor the fuzz target touches is total over already-validated bytes.

## Verdict

**APPROVE.** This is a correct, well-scoped, genuinely-exercised fuzz lane. No Critical or Important findings. The fuzz target's accessor coverage matches the reader's invariants exactly, and the CI plumbing is sound. A small set of non-blocking suggestions (corpus persistence across nightlies, dispatch/schedule interference wording, a `splice.rs` follow-up target) are worth filing but none block merge.
