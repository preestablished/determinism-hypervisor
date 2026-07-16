## Action Items

### Critical

- None.

### Important

- Change `VerifyReplay` so guest replay execution is accounted for by the worker's slot/core ownership model. Allocate an internal temporary slot through `SlotManager`, run the replay on a dedicated pinned slot/replay thread rather than Tokio's blocking pool, and release the slot on every success/error path. Add a test that a worker with no free slots rejects or queues an additional `VerifyReplay` instead of opening another invisible KVM VM.

- Refactor the `VerifyReplay` service/engine boundary so the snapstore client mutex is not held during the long KVM replay. Hold it only while fetching the input-log container, recovering machine config, and restoring snapshot data. Return a real server stream, or at minimum make cancellation and buffering behavior explicit and bounded.

- Handle `VerifyReplayRequest.bisect_on_divergence` explicitly and stop populating M8 divergence fields with undocumented phase-1 hash-pair bytes. Until bisection exists, either reject bisection requests as `UNIMPLEMENTED` or document and encode a phase-1 divergence payload without abusing `reg_diff` or `u64::MAX` as a missing-epoch sentinel.

### Suggestions

- Validate DHILOG header identity before opening KVM resources: check `base_snapshot_id` immediately after parsing, then check `machine_config_hash` and clock ratio after recovering the config but before creating the slot VM.

- Extend service tests to cover missing log, malformed log id, corrupt or wrong-version input-log container, inline `input_log`, and a divergence response. These tests should assert tonic status codes and proto field shapes, not only the happy path.

- Factor kick-handler and `InstRetired` setup into one helper shared by normal slot actors and VerifyReplay so signal routing and PMU setup do not drift.

- Add an explicit service-boundary size check for inline `input_log` bytes so the proto's segment-size cap is enforced independently of tonic transport defaults.
