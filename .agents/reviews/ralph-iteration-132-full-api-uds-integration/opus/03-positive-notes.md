# Positive Notes

- `crates/dh-worker/tests/m6_full_api_uds.rs:203`: The test starts a real `WorkerService` behind `HypervisorWorkerServer` and reaches it through a generated tonic client over UDS, so it exercises the public API surface rather than calling service methods directly.

- `crates/dh-worker/tests/m6_full_api_uds.rs:536`: The single-slot baseline and the 64-slot legs share the same restore, inject, run, snapshot, and destroy helpers, which reduces the chance that the comparison is accidentally testing different workflows.

- `crates/dh-worker/tests/m6_full_api_uds.rs:546`: The test restores all 64 slots before injection/run and verifies the slot table reports 64 paused slots, which is a good direct check that the acceptance run reaches a concurrently occupied state.

- `crates/dh-worker/tests/m6_full_api_uds.rs:371`: The digest includes public restore, run, and snapshot response fields, including state hashes, snapshot hash, input log id, machine config hash, capture bytes, framebuffer bytes, and frame counter. This is a useful regression signal for deterministic API output.

- `Cargo.lock:520`: The lockfile change only adds `dh-worker` edges to already-present `hyper-util` and `tower 0.5.3`; it does not pull in a new dependency closure.
