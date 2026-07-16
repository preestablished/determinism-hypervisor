# Tests

Checks run:

- `cargo +nightly fuzz check dhilog_splice`
- `cargo +nightly fuzz check dhilog_parse`

Both completed successfully.

Coverage assessment:

- cargo-fuzz registration is covered by `cargo +nightly fuzz check dhilog_splice`.
- The new target compiles against the real `dh_inputlog::splice::Lineage` API.
- The current branch does not add a deterministic smoke test or committed seed proving that `dhilog_splice` reaches successful multi-segment composition.

Recommended additional verification before landing:

- Run a short `cargo +nightly fuzz run dhilog_splice ... -- -max_total_time=10` after adding a valid multi-segment seed, and confirm the seed is accepted by the harness.
- If a seed is generated in CI instead of committed, add a cheap check that fails when the generated input does not produce a `Lineage` with more than one edge.
