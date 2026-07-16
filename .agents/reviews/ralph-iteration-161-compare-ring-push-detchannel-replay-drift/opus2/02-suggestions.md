# Suggestions

S1. Consider splitting or documenting `detchannel_exit_generated_event`.

The predicate is now used for three related but distinct decisions: record-position normalization in `comparable_replay_records_with_options` at `crates/dh-worker/src/replay_engine.rs:730`, classifier equality in `normalized_generated_record_equal` at `crates/dh-worker/src/replay_engine.rs:824`, and replay application skipping at `crates/dh-worker/src/replay_engine.rs:1899`. RING_PUSH belongs in the current position-normalization behavior, but future detchannel event additions may not belong in all three places. A short comment or separate predicates would reduce the chance of accidentally changing replay application semantics while only intending to change comparison diagnostics.

S2. When RING_PUSH channel-memory effect replay or comparison is implemented, add an effect-level test before changing the label to `channel_mutation_drift`.

`DeviceRail::apply_dev_event` only records device events today; it does not apply detchannel memory effects (`crates/dh-vmm/src/recording.rs:273`). That makes the current choice to leave RING_PUSH payload drift as `skipped_input` appropriate. A future label upgrade should be paired with a test that proves the RING_PUSH bytes were applied to, or compared against, channel memory.
