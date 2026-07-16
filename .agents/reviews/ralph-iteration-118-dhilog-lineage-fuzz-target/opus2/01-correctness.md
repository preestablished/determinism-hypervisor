# Correctness

## Finding: multi-segment composition is not effectively seeded

Severity: Medium

References:

- `crates/dh-inputlog/fuzz/fuzz_targets/dhilog_splice.rs:13`
- `crates/dh-inputlog/fuzz/fuzz_targets/dhilog_splice.rs:17`
- `crates/dh-inputlog/fuzz/fuzz_targets/dhilog_splice.rs:21`
- `.github/workflows/nightly-drift.yaml:150`

The harness does create a multi-segment grammar: `split_segments` consumes length-prefixed segment bytes, and `exercise` tries every prefix plus incremental `extend`. However, the workflow seeds the fuzzer with `tests/fixtures`, whose current checked-in files are plain single DHILOG files, not length-prefixed splice inputs.

That means the reliable seeded path is `raw_single -> Lineage::new([data]) -> edges()`. The multi-segment path exists, but CI depends on libFuzzer mutating or combining corpus entries into valid length-prefixed sealed logs with matching lineage anchors. Because the reader validates sealed logs and header hashes, this is an unlikely path to reach from the existing fixtures.

This matters for bead `6zm` because the requested risk area is specifically `Lineage::new/extend/edges` under multi-segment composition and the index arithmetic around `index - 1` / `len - 1`. Without at least one reachable valid multi-segment seed, the run mostly exercises single-segment acceptance and parse rejection rather than stitched lineage checks.

Suggested fix:

Add a deterministic seed path for `dhilog_splice` that reaches at least a two-edge valid lineage. Reasonable options:

- commit or generate a length-framed fuzz seed containing two or three sealed segments with matching `end_snapshot_id -> base_snapshot_id`, same config hash, and same clock ratio;
- add a target-local deterministic construction path that uses fuzz bytes to choose among prebuilt valid stitched segments, then still runs the hostile arbitrary segment list path;
- add a dedicated splice seed fixture outside the ignored `fuzz/corpus/` directory and pass it to `cargo fuzz run dhilog_splice`.

The important property is that CI should deterministically hit `Lineage::new(&[a, b, ...])`, `extend` success, and `edges()` over `len > 1` before relying on mutations.

## Non-finding: API surface exercised

The target does call the intended APIs. `exercise` tries all prefixes via `Lineage::new`, then starts from the first segment and applies `extend` for each later segment, calling `inspect` after every successful composition. `inspect` walks `edges()` and record accessors, including END access.
