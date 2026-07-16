# Suggestions (non-blocking)

## S1 — Document that "warm restore" is deliberately the warm-page-cache regime, and that sample 1 is a cold outlier

- **File:** `crates/dh-worker/tests/perf_gates.rs:181-204`

The restore loop creates a fresh slot each sample but reuses the same root `snapshot_ref`, the same tempdir store, and the same OS page cache for the store's pack files. After sample 1, the pack bytes are warm in the page cache — which is *correct*: IMPLEMENTATION-PLAN.md:84 names the gate "tier-B **warm** restore < 150 ms", and the engine module doc calls out the `mmap(MAP_PRIVATE)` materialized-file fast path as the perf optimization the 9sb bead measures. So the warm regime is the right one to gate.

The wrinkle is only statistical hygiene: sample 1 pays the cold-cache cost and is a left-tail outlier. With p50 = `samples[15]` after sort, one cold sample cannot move the median, so this is benign for the p50 gate — but a one-line note ("sample 1 is cold-cache; p50 is robust to it; the nightly trend bench warms up explicitly") would preempt the exact question a future reader asks. Unlike the snapshot dedup case (I1), warm-cache restore is genuinely the intended target, so no behavioral change is needed here.

## S2 — Factor out the ~120 lines duplicated between test and bench

- **Files:** `crates/dh-worker/tests/perf_gates.rs:38-68` and `crates/dh-worker/benches/perf_gates.rs:31-53`

`MEM`, `DIRTY_PAGES`, `config_128()`, and `boundary()` are duplicated verbatim, and the three fixture-setup blocks (frozen parent, root snapshot + dirty fill, fresh-slot restore) are near-identical. The bench already shares `tests/common/mod.rs` via `#[path = "../tests/common/mod.rs"]`. The cleanest extraction is to move `config_128`/`boundary`/the two consts into `tests/common/mod.rs` (behind the existing `#[allow(dead_code)]` convention there) so both targets pull them from one place. The per-surface setup blocks are harder to share cleanly because the bench wraps them in criterion closures while the test inlines them; leaving those duplicated is acceptable, but the consts + two helper fns are a low-risk dedup that prevents the two copies of `boundary()` from silently drifting (a drift there would make the two instruments measure different machine state). Worth doing; not blocking.

## S3 — Make the snapshot guard actually prove I/O happened, not just intent

- **File:** `crates/dh-worker/tests/perf_gates.rs:176`

`assert_eq!(out.pages_shipped, DIRTY_PAGES)` only proves the *intent* to ship 8k pages (it is `pages.len()` pre-dedup). If I1 is fixed by varying content, consider also asserting against the store's `(pages_new, pages_deduped)` return (surfaced by `put_pages`, though `take_snapshot` currently discards it) so the test fails loudly if a future change accidentally re-introduces cross-sample dedup. This would require threading the new/deduped counts through `TakeSnapshotOutcome`; only worth it if the engine already needs that telemetry for the nightly trend job — otherwise a comment is enough.

## S4 — Consider a tiny outlier-trim or report spread alongside the bare p50

- **File:** `crates/dh-worker/tests/perf_gates.rs:70-73, 122/179/204`

The `eprintln!` lines report only the p50. For a quiesced-box acceptance run it would cost almost nothing to also print min/max (or p10/p90) from the already-sorted `samples` slice, so the operator can see at a glance whether the distribution is tight (a trustworthy median) or bimodal (e.g. the cold-vs-warm split, or fsync hiccups). `p50()` already sorts in place, so `samples[0]` and `samples[len-1]` are free. Pure observability; no gate-logic change.
