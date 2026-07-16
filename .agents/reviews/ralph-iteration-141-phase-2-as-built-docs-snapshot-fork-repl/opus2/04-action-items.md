# Action Items

## Critical

None.

## Important

- Correct `docs/phase-2-exit-gate.md:67` so DHILOG golden bytes are not credited with NET_RX lower-bound validation or lineage/splice guarantees; cite reader validation and `crates/dh-inputlog/src/splice.rs` for those parts.
- Reword `docs/phase-2-exit-gate.md:103` from "M7 fork/VerifyReplay harness remains runnable" to "buildable/discoverable" or equivalent, unless the row includes an exact ignored skip-mode command and explicitly says it is not acceptance coverage.

## Suggestions

- Add pass/result snippets, commit SHA, or another durable evidence anchor to the `cargo test --workspace` and `cargo build --workspace` rows in `docs/phase-2-exit-gate.md`.
- Clarify that the perf p50 values are the 2026-06-12 accepted baselines from divergence ledger #20, not fresh measurements from the 2026-06-16 docs sign-off.
- Rename the `docs/ops/test-partitioning.md` Phase-2 row to make clear it is a reference record, not a runnable gate command.
