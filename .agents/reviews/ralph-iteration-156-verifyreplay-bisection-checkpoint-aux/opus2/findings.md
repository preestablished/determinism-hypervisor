# Findings

## Critical / Important

None found.

## Reviewed Areas

- Normalized reseal comparison: `crates/dh-worker/src/replay_engine.rs:363`-`409` and the reseal gate at `crates/dh-worker/src/replay_engine.rs:1556`-`1561`.
  - `LogReader::parse` validates each side before comparison, including `body_hash`, record framing, contiguous `seq`, monotone `icount`, `record_count`, `HAS_AUX`, `EPOCH_HASHES`, and END/header consistency in `crates/dh-inputlog/src/reader.rs:327`-`336` and `crates/dh-inputlog/src/reader.rs:506`-`569`.
  - The normalized comparison still compares replay-relevant header fields and every non-`BISECTION_CHECKPOINT` record by kind, flags, icount, boundary RIP, and payload.
  - The intentionally ignored differences are the checkpoint AUX records themselves plus fields that necessarily change when those records are omitted from the reseal (`seq`, `record_count`, `body_hash`, and `FLAG_HAS_AUX`).

- Bisection checkpoint evidence validation: `crates/dh-worker/src/service.rs:1464`-`1475`, `crates/dh-worker/src/service.rs:1485`-`1498`, and `crates/dh-worker/src/service.rs:1567`-`1586`.
  - When `bisect_on_divergence` is enabled, service code still builds a `BisectionCheckpointIndex` before replay and validates checkpoint snapshot refs before starting KVM replay.
  - Cross-record evidence validation remains in `crates/dh-worker/src/bisection_index.rs:217`-`279` and `crates/dh-worker/src/bisection_index.rs:380`-`483`, including checkpoint ordering, required preceding `EPOCH_HASH`, canonical-record separation, and `max_covered_gap`.

- Test coverage: `crates/dh-worker/src/replay_engine.rs:1645`-`1663`, `crates/dh-worker/src/service.rs:7909`-`8034`, `crates/dh-worker/src/service.rs:8038`-`8188`, `crates/dh-worker/src/service.rs:8192`-`8376`, and `crates/dh-worker/src/service.rs:8380`-`8443`.
  - The branch adds no-divergence coverage for a bisection-checkpoint log.
  - Existing divergence coverage still exercises the coarse semantic divergence path, replay-vs-recorded checkpoint evidence, widened evidence windows, and invalid checkpoint metadata rejection.
