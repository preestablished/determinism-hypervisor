# Suggestions

### S1 - Validate the log header before opening KVM resources

Path/lines: `crates/dh-worker/src/service.rs:2302`, `crates/dh-worker/src/service.rs:2316`

The replay engine catches `base_snapshot_id`, machine-config hash, and clock mismatches, but the service currently resolves images and opens KVM before handing the log to the engine. After parsing the header at `service.rs:2302`, reject a base mismatch immediately and check the recovered config hash before creating the VM. This makes bad requests cheaper and keeps validation errors away from KVM side effects.

```rust
let header = reader.header();
if header.base_snapshot_id != base_snapshot.to_bytes() {
    return Err(Status::failed_precondition("DHILOG header mismatch: base_snapshot_id"));
}
let config = recover_machine_config(...)?;
if header.machine_config_hash != config.config_hash().map_err(...) ? {
    return Err(Status::failed_precondition("DHILOG header mismatch: machine_config_hash"));
}
```

### S2 - Add negative and divergence-path service tests

Path/lines: `crates/dh-worker/src/service.rs:3097`

The new service test is valuable, but it only covers the stored-log success path. Add focused tests for missing log, bad `input_log_id` length, bad container version or corrupt container bytes, inline `input_log`, and a known divergence mapping. This is especially important because the mapping code is where proto contract drift is most likely.

Research reference: `rust-integration-testing.md:29-41` calls out missing failure-path and boundary-variant coverage.

### S3 - Factor counter setup shared with `SlotActor`

Path/lines: `crates/dh-worker/src/service.rs:2321`, `crates/dh-worker/src/runtime.rs:303`

The service now duplicates part of `actor_main`'s kick-handler/counter setup. Once VerifyReplay is moved onto a dedicated slot execution path, keep that setup in one helper so future changes to signal routing, PMU arming, or FIFO policy do not drift between normal run and replay verification.

### S4 - Make the inline log size limit explicit

Path/lines: `crates/dh-worker/src/service.rs:2290`; proto context `proto/hypervisor.proto:329`

The proto comment says inline DHILOG bytes are capped at the snapshot-store segment size. Tonic's default message size may currently provide an implicit cap, but an explicit validation at the service boundary would keep the API contract local and produce a stable `INVALID_ARGUMENT` instead of depending on transport configuration.

```rust
const MAX_INLINE_INPUT_LOG_BYTES: usize = 4 * 1024 * 1024;
if bytes.len() > MAX_INLINE_INPUT_LOG_BYTES {
    return Err(Status::invalid_argument("input_log exceeds 4 MiB segment limit"));
}
```
