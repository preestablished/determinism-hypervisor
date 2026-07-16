# Action Items

## Critical
- [ ] None.

## Important
- [ ] None.

## Suggestions
- [ ] [crates/dh-worker/src/restore_engine.rs:212-255] (S1) Optionally hoist the reverse shape-count check (`total_sections == 5 + expected_non_entropy_device_count`) to *before* the device-restore mutation loop, computing the expected count from `bus.devices()` up front. Behavior unchanged; raises the over-shaped-container error before any device state is written.
- [ ] [crates/dh-worker/src/restore_engine.rs:257-268] (S2) Optionally fold the PvClock `set_vns_base` into the device-restore loop so the `as_any_mut`/`downcast_mut::<PvClock>` seam appears exactly once and the bus is walked once. Optional — the current two-pass form is also defensibly clearer.
- [ ] [crates/dh-worker/src/restore_engine.rs:220-221] (S3) Optionally enrich the entropy-arm restore error (currently a fixed string via `map_err(|_| ...)`) for symmetry with the non-entropy arm's version/length detail. Minor diagnosability.
- [ ] [crates/dh-worker/src/restore_engine.rs:286-287] (S4) Clarify `pages_loaded`: either document it as "total guest RAM pages materialized" or return `resolved.len()` if a caller would benefit from the store-side entry count. Cosmetic.
- [ ] [crates/dh-worker/tests/restore_engine.rs:310-313, :469] (S5) Optionally add a resolved-page-set byte-equality assertion alongside the ref equality to localize a future regression to container-vs-pages. The ref equality already implies full container byte-identity; optional.
- [ ] [crates/dh-worker/src/restore_engine.rs:133] (S6) Optionally add a debug-assert or doc note that `slot.mem_bytes % PAGE_SIZE == 0`, making the truncating-division invariant explicit (pre-existing, mirrors snapshot_engine.rs:121).
