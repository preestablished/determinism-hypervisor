## Action Items

### Critical
- [x] [crates/dh-worker/benches/perf_gates.rs:82] Fix the stale fork_slot benchmark call and run `cargo check -p dh-worker --benches`.

### Important
- [x] [crates/dh-worker/src/fork_engine.rs:135] Normalize explicit zero seeds defensively in fork_slot and add an engine regression.

### Suggestions
- [x] [crates/dh-worker/src/proto_map.rs:104] Clarify count validation ownership.
- [x] [crates/dh-worker/src/proto_map.rs:532] Add 33-byte invalid seed coverage.

