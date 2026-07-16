# Anscombe Review

Scope: working-tree diff for `ralph/iteration-125-dh-workerd-grpc-service-on-7400-uds-life`.

Findings:

1. `proto/hypervisor.proto:258` says `seal_input_log` defaults true, but `service.rs` treated the proto3 scalar default as false. Fixed by making the proto field `optional bool` and using `unwrap_or(true)` in the worker.

2. `service.rs:1405` returned empty capture fields when `TakeSnapshotRequest.capture` was present. Fixed by rejecting capture requests with `UNIMPLEMENTED` before snapshot/store mutation.

3. `service.rs:1202` gave fork children the parent’s existing `base_snapshot`, which would be wrong if the parent had run since that snapshot. Fixed by refusing fork when the parent segment has advanced.

4. `service.rs:534` does not build a detchannel device. Left as residual blocked scope for the execution/SDK-event/capture beads; current lifecycle increment deliberately keeps `Run`, `InjectInputs`, `NextSdkEvent`, and capture paths unimplemented or rejected.

Verification reported by reviewer:

- `git diff --check`
- `cargo fmt --check --package dh-worker`
- native and aarch64 `cargo check -p dh-worker --bin dh-workerd`
- `cargo test -p dh-worker service::tests:: -- --nocapture`
- `cargo test -p dh-worker --bin dh-workerd`

