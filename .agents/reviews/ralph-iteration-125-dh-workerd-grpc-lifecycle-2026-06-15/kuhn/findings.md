# Kuhn Review

Scope: working-tree diff for `ralph/iteration-125-dh-workerd-grpc-service-on-7400-uds-life`.

Findings:

1. P0 `crates/dh-worker/src/service.rs:1302`: `take_snapshot` can durably store a snapshot and clear the dirty set, then fail while sealing/storing the DHILOG or updating the manager. On that path the runtime can keep the old parent while dirty state has been consumed. Fixed by faulting the slot on any post-store failure and by moving fallible preconditions before snapshot where possible.

2. P1 `crates/dh-worker/src/service.rs:1324`: DHILOG sealing used cumulative `boundary.icount`; segments are segment-relative. Fixed by sealing with `runtime.position.segment_icount` and computing segment-relative vns.

3. P1 `crates/dh-worker/src/service.rs:1344`: after starting a new segment, `position.segment_icount` was not reset to 0. Fixed when rolling `base_snapshot`/`log`.

4. P1 `crates/dh-worker/src/service.rs:1202`: `Fork` children inherited an older parent `base_snapshot` even if the parent had advanced within the current segment. Fixed by rejecting fork when `parent_runtime.position.segment_icount != 0`.

5. P1 `crates/dh-worker/src/service.rs:1101`: `RestoreSnapshot` accepted an explicit all-zero 32-byte entropy seed, but DHILOG interprets zero as “continue snapshot PRNG”. Fixed by rejecting explicit zero and requiring omission for continue.

6. P2 `crates/dh-worker/src/service.rs:1273`: `TakeSnapshotRequest.capture` was ignored. Fixed by rejecting capture with `UNIMPLEMENTED` until the capture bead lands.

7. P2 `crates/dh-worker/src/service.rs:1334`: SILG `inner_version` used DHILOG wire header version `0x0100` instead of crate-level `DHILOG_FORMAT_VERSION`. Fixed.

