# Review — iteration 72: snapstore-server for tests (build-and-spawn in-process)

- **Branch:** `ralph/iteration-72-snapstore-server-decision` vs `main`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus (2nd reviewer)
- **Bead:** determinism-hypervisor-wbq
- **Commit:** `82c2c21` (iteration 72 checkpoint)

## Scope

Independent, experiment-driven verification of the iteration-72 decision: R12
joint tests spawn the REAL `snapstore-server` in-process via `serve_for_tests`
over a UDS on a per-test `TempDir`. Diff meat:

- `docs/decisions/snapstore-server-for-tests.md` (new, 72 lines)
- `tests/determinism/tests/store_joint.rs` (new, 146 lines, 3 tests)
- `Cargo.toml` (workspace dep defs) + `tests/determinism/Cargo.toml` (x86-gated dev-deps)
- `Cargo.lock` (server closure), `docs/ops/test-partitioning.md` (1 line)

## Summary of findings

The **decision itself is sound and well-supported**: build-and-spawn-in-process
is the right call, the cited seams (`serve_for_tests`,
`serve_for_tests_with_metrics`, the sibling's own `page_channel_fallback.rs`)
all exist and match, hermeticity holds under stress, and the Cargo target-gating
is verifiably correct (aarch64 never compiles the server closure).

The **one substantive problem is the headline empirical claim** embedded in the
code comment and the test design. The comment in `store_joint.rs` asserts the
`put_pages` (pages_new, pages_deduped) split "flips to all-deduped under the
client's transparent retry, observed as (0,3) on a FRESH store under parallel
test load," and the test was weakened to assert only `new + deduped == 3`.

I could not reproduce this. Across **246 fresh-store first-puts** (sequential +
up to 8-way parallel, content-salted), the split was `(3, 0)` **every single
time** — zero anomalies. Reading the client retry path confirms why: `with_retry`
only fires on `Unavailable | DeadlineExceeded | Transport(_)`; on a healthy
in-process UDS server to an in-process server **none of these occur**, so the
retry never triggers and the split is deterministic. The sibling's own
`put_pages_retries_on_unavailable` test (test_cases.rs:558) even asserts the
*opposite* shape on retry — `(new=1, deduped=0)` — because `FlakyServer` injects
its failure *before* committing pages. The committed comment's stated mechanism
and its observed value are therefore unsubstantiated as written. This is an
Important (not Critical) finding: the weakened assertion is *safe* (it can't
produce false failures) but the *rationale* attached to it is misleading and
will mislead the qmp/6hg authors who build on this helper.

## Verdict

**APPROVE WITH CHANGES.** The decision and mechanism are correct and ship-ready;
the tests pass and are hermetic. Before this is treated as ground truth, the
`store_joint.rs` retry-flip comment must be corrected (or the original (0,3)
observation re-captured with a repro), because it documents a client semantic I
could not reproduce and which the client's own retry policy contradicts on a
healthy server.

## Stats

| Item | Result |
|---|---|
| `cargo test -p determinism-tests --test store_joint` (2x) | PASS (3/3 each) |
| Stress: `--test-threads=8`, 5 iterations | PASS, 0 flakes |
| Fresh-store dedup-flip repro (246 first-puts, ≤8-way parallel) | 0/246 anomalies — claim NOT reproduced |
| `cargo clippy -p determinism-tests` (x86_64, --all-targets) | clean |
| `cargo clippy -p determinism-tests` (aarch64 cross, --all-targets) | clean, server NOT compiled (2.82s) |
| aarch64 graph contains snapstore-server? | NO (cargo metadata --filter-platform) |
| x86_64 graph: crates depending on snapstore-server | only `determinism-tests` |
| Tree clean after experiments | YES |
| Critical findings | 0 |
| Important findings | 1 |
| Suggestions | 5 |
