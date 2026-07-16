# Correctness

## Finding: accepted multi-segment splice paths are effectively unseeded

Severity: Medium

`dhilog_splice.rs` splits raw fuzz input into length-prefixed segments and calls `Lineage::new` over prefixes, then tries `extend` from the first accepted segment (`crates/dh-inputlog/fuzz/fuzz_targets/dhilog_splice.rs:17`, `crates/dh-inputlog/fuzz/fuzz_targets/dhilog_splice.rs:41`, `crates/dh-inputlog/fuzz/fuzz_targets/dhilog_splice.rs:57`). The nightly command passes `tests/fixtures` as an extra corpus directory (`.github/workflows/nightly-drift.yaml:150`), but those fixture files are complete DHILOG files, not length-prefixed multi-segment splice inputs.

That means the deterministic accepted path is mostly:
- raw single fixture -> `Lineage::new(&[fixture])` succeeds
- `edges()` is inspected for a one-segment lineage

The important multi-segment success cases are much harder to reach by mutation. `Lineage::new` and `extend` require every segment to parse as a sealed v1 log and require stitch continuity (`crates/dh-inputlog/src/splice.rs:85`, `crates/dh-inputlog/src/splice.rs:113`). Synthesizing two valid sealed logs with matching `end_snapshot_id`/`base_snapshot_id` and valid body hashes from arbitrary bytes is not a realistic fuzz discovery path.

This misses the core hostile-input surface described in the bead: splice checks and `index - 1` / `len - 1` arithmetic under multi-segment composition. The target will exercise many rejection paths, but it is unlikely to exercise successful `extend`, successful `Lineage::new` with `len() > 1`, or `edges()` over multiple accepted segments.

Recommended fix: seed `dhilog_splice` with at least one valid length-prefixed two- or three-segment lineage, or have the fuzz target construct valid sealed segments from fuzzed anchors using the existing `LogWriter`/`SealParams` path and then splice those generated segments. Keep the arbitrary raw-byte path too; the issue is the missing accepted multi-segment corpus.

## No panic bug found in the harness structure

The harness handles empty segment vectors, zero-length split segments, rejected parses, and rejected child extensions without panicking. `split_segments` also makes progress by consuming the length prefix even when the requested payload length is zero.
