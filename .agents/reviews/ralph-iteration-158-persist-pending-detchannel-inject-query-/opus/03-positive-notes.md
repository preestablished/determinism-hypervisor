# Positive Notes

- `crates/dh-devices/src/detchannel.rs:312` serializes pending injects from a `BTreeMap`, which gives the v2 variable-length table a canonical sorted order without extra sorting logic.
- `crates/dh-devices/src/detchannel.rs:370` preserves v1 restore compatibility with an exact legacy-length check and an empty pending table, avoiding ambiguity between legacy and v2 payloads.
- `crates/dh-devices/src/detchannel.rs:380` through `crates/dh-devices/src/detchannel.rs:397` validates v2 count, total length, detached-state emptiness, and strictly increasing `iseq`; that is the right shape for a snapshot format parser.
- `crates/dh-devices/src/detchannel.rs:665` consumes restored pending entries through `FaultPlan::decide` and logs exactly one `PIO_ANSWER`, so replay-backed plans still consume the expected answer cursor.
- `crates/dh-devices/src/detchannel.rs:705` mirrors newly drained `InjectQuery` events and drops stale restored entries for the same `iseq`, keeping the live mirror synchronized after restore.
- `crates/dh-devices/src/detchannel.rs:1405` covers the load-bearing OUT/restore/IN gap and asserts the pending tables are cleared after the answer.
- `crates/dh-worker/tests/linux_worker_api.rs:826` updates the Linux worker acceptance helper to accept both legacy EVTC v1 and variable-length EVTC v2 sections.
- `docs/phase-2-exit-gate.md:72` and `docs/upstream-divergences.md:280` document the format bump and legacy restore compatibility in the right long-lived references.
