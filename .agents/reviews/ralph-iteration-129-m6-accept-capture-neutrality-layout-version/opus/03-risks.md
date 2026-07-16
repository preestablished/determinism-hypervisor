# Risks And Coverage Notes

## Child Refs

The test does cover the child snapshot ref identity for the restore/run/snapshot service flow:

- `crates/dh-worker/src/service.rs:4035` starts the no-capture child leg from the base snapshot.
- `crates/dh-worker/src/service.rs:4036` through `crates/dh-worker/src/service.rs:4043` starts the capture child leg from the same base snapshot.
- `crates/dh-worker/src/service.rs:4048` through `crates/dh-worker/src/service.rs:4052` compares the resulting `SnapshotRef.hash`.

`proto::SnapshotRef` only has the hash field (`proto/hypervisor.proto:51`), so this is a real child snapshot ref comparison. It does not cover fork/live-child refs; it covers child snapshots produced by two restored descendants.

## Epoch Hashes

The standalone direct-run helper does exercise epoch callbacks and verifies capture-after-boundary does not perturb those callback outputs. That is useful, but it is not the same path as `WorkerService::run`.

The service-level epoch assertion is too weak because an empty vector on both sides passes. This is the main blocking risk.

## Inputs

The capture-neutrality service legs use the same base snapshot and no scheduled inputs. An empty input set is deterministic, but it is weaker than exercising a non-empty "same inputs" flow. A future input/capture interaction in queue consumption, canonical input logging, or frame/input scheduling would not be covered by this acceptance test.

## Layout Version

The new test checks `FAILED_PRECONDITION` and a `layout_version` message substring for both:

- `Run`: `crates/dh-worker/src/service.rs:4074` through `crates/dh-worker/src/service.rs:4084`
- `TakeSnapshot`: `crates/dh-worker/src/service.rs:4106` through `crates/dh-worker/src/service.rs:4115`

Existing nearby tests add more detailed behavior checks, including run-boundary commit after run capture failure and no snapshot publication after failed snapshot capture.

## Hardware Gating

The test is `#[cfg(target_arch = "x86_64")]` and calls `runtime_tests_available()` before opening KVM-backed legs. `runtime_tests_available()` checks KVM dirty-ring availability and honors `DH_REQUIRE_KVM_TESTS`.

