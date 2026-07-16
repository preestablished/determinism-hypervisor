# Tests

Build checks passed:
- `cargo +nightly fuzz check dhilog_splice`
- `cargo +nightly fuzz check dhilog_parse`

Test coverage gap:
- There is no deterministic smoke or seed that proves `dhilog_splice` reaches an accepted lineage with more than one segment.

Suggested check after adding a multi-segment seed:
- Run `cargo +nightly fuzz run dhilog_splice fuzz/corpus/dhilog_splice -- -runs=1`.
- Keep the existing `cargo +nightly fuzz check dhilog_splice` gate.

The existing unit tests in `splice.rs` cover accepted multi-segment composition, but the new cargo-fuzz target does not currently get that same accepted shape through its corpus.
