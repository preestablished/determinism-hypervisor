# M4 Perf-Gate Instruments — Second Review (Opus)

- **Branch:** `ralph/iteration-99-m4-perf-gates-p50` vs `main`
- **Date:** 2026-06-12
- **Reviewer:** Claude Opus (2nd reviewer)
- **Scope:** `crates/dh-worker/tests/perf_gates.rs` (new), `crates/dh-worker/benches/perf_gates.rs` (new), criterion wiring in `Cargo.toml` + `crates/dh-worker/Cargo.toml`. Engine sources read for behavioral verification: `src/snapshot_engine.rs`, `src/restore_engine.rs`, `src/fork_engine.rs`, `tests/common/mod.rs`; store path `../snapshot-store/crates/snapstore-client/src/client.rs` + `snapstore-pagestore/src/ingest.rs`.

## Summary

The change adds an `#[ignore]`d single-threaded acceptance test asserting the three IMPLEMENTATION-PLAN M4 p50 gates (fork < 10 ms, 8k-page incremental snapshot < 15 ms, tier-B warm restore < 150 ms) on a 128 MiB guest, plus a criterion trend bench over the same three surfaces with a custom `main` (`harness = false`). The engine-driving code is correct and faithfully exercises the real store (R12). The methodology is mostly sound and well-documented in the module header.

Two real problems surfaced:

1. **CRITICAL — the bench breaks the aarch64 CI lane.** The bench crate root is `#![cfg(target_arch = "x86_64")]` and the target is `harness = false`, so on the `ubuntu-24.04-arm` lane the crate compiles to an empty body with **no `main`** → `error[E0601]: main function not found`. CI runs `cargo clippy --workspace --all-targets -D warnings` and `cargo build --workspace` on that lane, both of which compile bench targets. Reproduced empirically (see 01). The sibling `tests/*.rs` use the same crate-root gate safely *only* because the default libtest harness synthesizes `main`; a `harness = false` bench has no harness to do so.

2. **IMPORTANT — the incremental-snapshot p50 measures a server-deduped path.** All 30 samples (and the root full snapshot before them) ship byte-identical page content (`[page as u8 ^ 0x5A]`, deterministic per page). The pagestore dedups globally by BLAKE3 content hash (`ingest.rs:265-267`, test `dedup_across_batches`), so samples 2..30 are all dedup hits — **no page bytes are written to disk** for the median sample. `pages_shipped` stays 8192 (it counts pre-upload `pages.len()`, snapshot_engine.rs:149), so the `assert_eq!(out.pages_shipped, DIRTY_PAGES)` passes without proving any I/O happened. The p50 therefore measures manifest-assembly + container-put + fsync over an empty page-write set — NOT the cold 8k-page write the gate intends. This does not change the PASS/FAIL verdict on this box (snapshot already FAILs at 111.6 ms), but it under-measures the true cost and the methodology note should be corrected.

## Verdict

**REQUEST CHANGES** — the aarch64 breakage is a hard CI failure and must be fixed before merge. The dedup methodology issue is important to document/correct but does not block the human-decision escalation (bead 8ot) already in flight, since the measured snapshot number already fails its gate.

The measured FAILs (snapshot 111.6 ms vs 15 ms; restore 317 ms vs 150 ms) tracking the box's raw dd-fsync ext4 floor is a platform/threshold question correctly escalated to bead 8ot; nothing in the test code is producing a spurious failure. The acceptance bead 9sb staying open blocked on 8ot is the right call.

## Stats

- Files reviewed: 4 changed + 5 engine/store sources read
- Critical: 1
- Important: 1
- Suggestions: 4
- Positive notes: 5
