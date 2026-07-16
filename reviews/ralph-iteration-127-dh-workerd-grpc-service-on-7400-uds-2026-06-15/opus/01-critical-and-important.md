# Critical And Important

## Critical

No Critical issues found.

## Important

### I1. VerifyReplay bypasses the slot manager and dedicated slot actor for guest execution

Severity: Important

Files:
- `crates/dh-worker/src/service.rs:2288`
- `crates/dh-worker/src/service.rs:2316`
- `crates/dh-worker/src/service.rs:2324`
- `crates/dh-worker/src/service.rs:2334`
- `crates/dh-worker/src/service.rs:2354`
- `crates/dh-worker/src/runtime.rs:166`
- `crates/dh-worker/src/runtime.rs:303`

Description:
`VerifyReplay` runs the full KVM replay inside `blocking_lifecycle`, opening KVM, creating a VM, opening an `InstRetired` counter, and driving guest execution on a Tokio blocking-pool thread. This bypasses the slot manager's fixed capacity and the per-slot `SlotActor` model. The runtime actor documentation says slot actors own the vCPU fd and thread-attached counter, and RPC handlers should not run guest work on Tokio's blocking pool. The actor path also pins the thread to the slot core and attempts FIFO scheduling; the new verifier path does neither.

Concrete impact:
- Multiple concurrent `VerifyReplay` requests can create unbounded KVM VMs/counters outside `slot_count`.
- `ListSlots` cannot show this resource pressure because no slot is allocated.
- The replay executes without the dedicated-core/FIFO setup used by normal `Run`, which weakens the reference-worker execution contract.
- Tokio's blocking pool has a high upper limit, so this can oversubscribe CPU/KVM/PMU resources under request fan-out.

Research reference:
- `/home/infra-admin/.claude/research/tokio-spawn-blocking-service-work.md:14`
- `/home/infra-admin/.claude/research/tokio-spawn-blocking-service-work.md:16`
- `/home/infra-admin/.claude/research/tokio-spawn-blocking-service-work.md:22`
- `/home/infra-admin/.claude/research/tokio-spawn-blocking-service-work.md:27`

Suggested fix:
Account verifier work through the same capacity/core ownership model as slot work. One narrow fix is to allocate an ephemeral slot lease for verification, pin the blocking thread to that slot's core before opening the counter, and always destroy the lease on exit. A cleaner long-term fix is a dedicated verifier actor/semaphore pool sized by configured slot cores.

Sketch:

```rust
let manager = self.inner.manager.clone();
let events = blocking_lifecycle("VerifyReplay", move || {
    let allocated_at_ms = lease_now_ms();
    let verify_lease = manager
        .allocate(allocated_at_ms)
        .map_err(slot_error_to_status)?;
    let core = runtime_core(manager.as_ref(), verify_lease.slot_id)?;

    let result = (|| {
        dh_vmm::run::install_kick_handler()
            .map_err(|e| Status::failed_precondition(format!("install kick handler: {e}")))?;
        dh_vmm::run::pin_current_thread(core)
            .map_err(|e| Status::failed_precondition(format!("pin VerifyReplay: {e:?}")))?;
        let _ = dh_vmm::run::set_current_thread_fifo();

        // Existing KVM/counter/replay work.
        run_verify_replay_on_current_thread()
    })();

    let cleanup = manager
        .destroy(&verify_lease, lease_now_ms())
        .map_err(slot_error_to_status);
    match (result, cleanup) {
        (Ok(events), Ok(())) => Ok(events),
        (Err(e), Ok(())) => Err(e),
        (Ok(_), Err(e)) => Err(e),
        (Err(original), Err(cleanup)) => Err(Status::internal(format!(
            "VerifyReplay failed with {}: {}; cleanup also failed with {}: {}",
            original.code(),
            original.message(),
            cleanup.code(),
            cleanup.message()
        ))),
    }
})
.await?;
```

### I2. `bisect_on_divergence` is ignored and divergence fields are not contract-accurate

Severity: Important

Files:
- `crates/dh-worker/src/service.rs:597`
- `crates/dh-worker/src/service.rs:616`
- `crates/dh-worker/src/service.rs:626`
- `crates/dh-worker/src/service.rs:2281`
- `crates/dh-worker/src/service.rs:2364`
- `proto/hypervisor.proto:338`
- `proto/hypervisor.proto:352`

Description:
The service reads the request but never uses `request.bisect_on_divergence`. `verify_progress_to_proto` always maps a `dh_verify::VerifyProgress::Divergence` into proto `Divergence` with `icount_lo == icount_hi == at_icount`, `rip_expected == 0`, `rip_actual == 0`, `diff_page_idx == []`, and `reg_diff == expected_hash || got_hash`. That is not the proto contract: the proto comments define these as bisection diagnostics and a postcard-encoded `Vec<RegDiff>`. For `first_bad_epoch == None`, the code writes `u64::MAX`, but the schema does not document that sentinel.

Concrete impact:
- `dh-cli verify` defaults `bisect_on_divergence` to true, so a real divergent replay can return a stream that looks like bisection output even though no bisection ran.
- Consumers that parse `reg_diff` according to the documented postcard shape will receive raw hash bytes instead.
- Consumers cannot reliably distinguish "no bad epoch because END identity diverged" from a huge epoch index.

Research reference:
- `/home/infra-admin/.claude/research/tonic-prost-codegen.md:22`
- `/home/infra-admin/.claude/research/tonic-prost-codegen.md:29`

Suggested fix:
Either implement the bisection contract now, or make the unsupported flag explicit. Since the adjacent `dh-verify` docs say bisection is M8, the safer pre-M8 behavior is to fail only when a divergence occurs and the caller requested bisection, while continuing to support coarse divergence for `--no-bisect`.

Sketch:

```rust
fn verify_progress_to_proto(
    progress: VerifyProgress,
    bisect_on_divergence: bool,
) -> Result<proto::VerifyReplayProgress, Status> {
    use proto::verify_replay_progress::Msg;
    let msg = match progress {
        VerifyProgress::Divergence { .. } if bisect_on_divergence => {
            return Err(Status::unimplemented(
                "VerifyReplay divergence bisection is not implemented yet; retry with --no-bisect",
            ));
        }
        VerifyProgress::Divergence {
            first_bad_epoch,
            at_icount,
            what,
            expected,
            got,
        } => Msg::Divergence(proto::Divergence {
            first_bad_epoch: first_bad_epoch.unwrap_or(0),
            icount_lo: at_icount,
            icount_hi: at_icount,
            rip_expected: 0,
            rip_actual: 0,
            reg_diff: coarse_hash_diff_bytes(expected, got),
            diff_page_idx: Vec::new(),
            suspected_cause: format!("coarse: {what}"),
        }),
        // Existing EpochOk/Done mappings.
    };
    Ok(proto::VerifyReplayProgress { msg: Some(msg) })
}

let bisect_on_divergence = request.bisect_on_divergence;
let events = report
    .events
    .into_iter()
    .map(|p| verify_progress_to_proto(p, bisect_on_divergence))
    .collect::<Result<Vec<_>, Status>>()?;
```

Add a service-level divergent replay test that calls `VerifyReplay` with `bisect_on_divergence: true` and asserts the chosen contract.

### I3. The inline `input_log` path has no application-level size validation

Severity: Important

Files:
- `crates/dh-worker/src/service.rs:2289`
- `crates/dh-worker/src/service.rs:2302`
- `proto/hypervisor.proto:326`
- `proto/hypervisor.proto:329`

Description:
The proto documents inline `input_log` as a bounded per-segment payload, but the service accepts `WireLog::InputLog(bytes)` and immediately parses it without checking length. gRPC transport defaults may reject large requests before this code in normal server mode, but this service implementation is also callable directly in tests and future server configuration can raise tonic message limits. The application boundary should enforce its own API contract before parsing untrusted DHILOG bytes or allocating replay resources.

Concrete impact:
- An in-process caller or future larger-message server configuration can send arbitrarily large `input_log` bytes into `LogReader::parse`.
- The error class for oversize inline logs is not pinned by service tests.
- The branch only tests the `input_log_id` happy path, so this contract gap can regress unnoticed.

Research reference:
- `/home/infra-admin/.claude/research/tonic-prost-codegen.md:22`
- `/home/infra-admin/.claude/research/tonic-prost-codegen.md:29`
- `/home/infra-admin/.claude/research/rust-integration-testing.md:29`
- `/home/infra-admin/.claude/research/rust-integration-testing.md:40`

Suggested fix:
Add a named service-side cap and reject before parse. Keep the constant aligned with the proto/snapshot-store contract, then add focused tests for oversize inline bytes and invalid IDs.

```rust
#[cfg(target_arch = "x86_64")]
const VERIFY_REPLAY_INLINE_LOG_MAX_BYTES: usize = 4 * 1024 * 1024;

let log_bytes = match wire_log {
    WireLog::InputLog(bytes) => {
        if bytes.len() > VERIFY_REPLAY_INLINE_LOG_MAX_BYTES {
            return Err(Status::invalid_argument(format!(
                "VerifyReplay.input_log exceeds {} bytes",
                VERIFY_REPLAY_INLINE_LOG_MAX_BYTES
            )));
        }
        bytes
    }
    WireLog::InputLogId(id) => {
        // Existing path.
    }
};
```

### I4. The snapshot-store mutex is held across the whole KVM replay

Severity: Important

Files:
- `crates/dh-worker/src/service.rs:2307`
- `crates/dh-worker/src/service.rs:2354`
- `crates/dh-worker/src/service.rs:2360`

Description:
After fetching/parsing the input log, `VerifyReplay` locks the shared `SnapstoreClient` mutex and keeps that guard alive through config recovery, image resolution, KVM setup, counter setup, and the full `verify_replay` execution. The replay engine only needs the store for base snapshot restore; after restore, the CPU-heavy replay walk no longer needs snapshot-store access. Holding the coarse client mutex through the full replay serializes unrelated snapshot-store users behind a potentially long verification run.

Concrete impact:
- A long `VerifyReplay` can block `RestoreSnapshot`, `TakeSnapshot`, and other `VerifyReplay` requests from using the worker's snapshot-store client.
- This creates head-of-line blocking at the service boundary and can make unrelated lifecycle RPCs look unavailable under verifier load.

Research reference:
- `/home/infra-admin/.claude/research/tokio-spawn-blocking-service-work.md:23`
- `/home/infra-admin/.claude/research/tokio-spawn-blocking-service-work.md:29`

Suggested fix:
Narrow the store lock to the actual store operations. If the current `replay_engine::verify_replay` shape forces a `&SnapstoreClient` for the whole replay, split the replay engine so the base restore happens while the client is locked and the replay walk runs after the guard is dropped, or store/connect per-request clients instead of a single mutex-protected blocking client.

Sketch:

```rust
let restored = {
    let store = store
        .lock()
        .map_err(|_| Status::internal("snapshot-store client mutex poisoned"))?;
    restore_replay_base(&mut slot, &mut rail.bus, &config, base_snapshot.clone(), &counter, &store)?
};

// No snapshot-store mutex held while the vCPU is replayed.
let report = crate::verify_replay::verify_replay_from_restored(
    &mut slot,
    rail,
    &config,
    restored,
    &counter,
    &log_bytes,
)?;
```
