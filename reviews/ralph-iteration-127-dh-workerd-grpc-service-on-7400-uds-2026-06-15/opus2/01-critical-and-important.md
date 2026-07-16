# Critical And Important Issues

## Critical

No Critical issues found.

## Important

### I1 - VerifyReplay runs guest work outside slot/core ownership

Severity: Important
Path/lines: `crates/dh-worker/src/service.rs:2288`, `crates/dh-worker/src/service.rs:2324`, `crates/dh-worker/src/service.rs:2334`; related contracts in `crates/dh-worker/src/runtime.rs:166` and `crates/dh-worker/src/runtime.rs:168`

The new RPC opens a fresh KVM slot, opens an `InstRetired` counter, and drives replay directly inside `blocking_lifecycle` on Tokio's blocking pool. That bypasses the daemon's slot ownership model: `SlotActor` is documented as the stable per-slot owner for the `SlotRuntime`, vCPU fd, and thread-attached counter, and the comment explicitly says RPC handlers "never run guest work on Tokio's blocking pool" (`runtime.rs:168-171`). It also bypasses `SlotManager` resource accounting and dedicated core pinning, so concurrent `VerifyReplay` calls can create unbounded replay VMs and PMU counters that are invisible to `slots_free` and not pinned to the configured slot cores.

Suggested fix: make VerifyReplay consume an internal temporary slot resource and run the replay on the slot's dedicated core, either through a short-lived actor or a dedicated pinned replay thread. The important properties are: allocate/release through `SlotManager`, pin before opening the thread-local counter, and clean up even on errors.

```rust
let manager = self.inner.manager.clone();
let events = blocking_lifecycle("VerifyReplay", move || {
    let allocated_at = lease_now_ms();
    let lease = manager.allocate(allocated_at).map_err(slot_error_to_status)?;
    let core = runtime_core(manager.as_ref(), lease.slot_id)?;

    let result = std::thread::Builder::new()
        .name(format!("dh-verify-{}", lease.slot_id))
        .spawn(move || -> Result<Vec<proto::VerifyReplayProgress>, Status> {
            dh_vmm::run::install_kick_handler()
                .map_err(|e| Status::failed_precondition(format!("install kick handler: {e}")))?;
            dh_vmm::run::pin_current_thread(core)
                .map_err(|e| Status::failed_precondition(format!("pin VerifyReplay: {e:?}")))?;
            let _ = dh_vmm::run::set_current_thread_fifo();

            // Existing KVM/counter/replay body lives here.
            run_verify_replay_body()
        })
        .map_err(|e| Status::internal(format!("start VerifyReplay thread: {e}")))?
        .join()
        .map_err(|_| Status::internal("VerifyReplay thread panicked"))?;

    let cleanup = manager.destroy(&lease, lease_now_ms()).map_err(slot_error_to_status);
    match (result, cleanup) {
        (Ok(events), Ok(())) => Ok(events),
        (Ok(_), Err(cleanup)) => Err(Status::internal(format!(
            "VerifyReplay succeeded but slot cleanup failed: {}: {}",
            cleanup.code(),
            cleanup.message()
        ))),
        (Err(err), Ok(())) => Err(err),
        (Err(err), Err(cleanup)) => Err(Status::internal(format!(
            "VerifyReplay failed with {}: {}; slot cleanup also failed with {}: {}",
            err.code(),
            err.message(),
            cleanup.code(),
            cleanup.message()
        ))),
    }
})?;
```

Research reference: `tokio-spawn-blocking-service-work.md:15-23` warns that `spawn_blocking` work should be bounded and explicitly limited when long or CPU-heavy. The local runtime contract is stricter: guest work belongs on the per-slot execution owner, not the Tokio blocking pool.

### I2 - Snapshot-store mutex is held through the whole replay and the stream is fully buffered

Severity: Important
Path/lines: `crates/dh-worker/src/service.rs:2307`, `crates/dh-worker/src/service.rs:2354`, `crates/dh-worker/src/service.rs:2364`, `crates/dh-worker/src/service.rs:2371`

After parsing the log, the service locks the shared blocking snapstore client at `service.rs:2307` and keeps that guard alive while `crate::verify_replay::verify_replay` restores the snapshot and then runs the entire guest replay. The replay engine only needs the store for restore (`crates/dh-worker/src/replay_engine.rs:121-130`); keeping the mutex for the subsequent KVM run serializes unrelated `RestoreSnapshot`, `TakeSnapshot`, and `VerifyReplay` store operations behind a long guest execution. The RPC also collects every progress event into a `Vec` before returning a `tokio_stream::iter`, so clients see no `EpochOk` progress until the full replay is already complete and request cancellation cannot stop the started blocking task.

Suggested fix: split the replay path so the store lock is held only for `get_input_log`, machine-config recovery, and snapshot restore. Then stream progress from the blocking producer to tonic as events are produced. If the engine cannot stream today, at least drop the store guard before the long KVM run by introducing a lower-level helper that accepts an already-restored replay state.

```rust
let (tx, rx) = tokio::sync::mpsc::channel(32);
tokio::task::spawn_blocking(move || {
    let restored = {
        let store = store.lock().map_err(|_| Status::internal("snapshot-store client mutex poisoned"))?;
        restore_verify_base(&mut slot, &mut rail, &config, base_snapshot, &counter, &store)
    };

    match restored.and_then(|state| run_replay_from_restored(state, |progress| {
        tx.blocking_send(Ok(verify_progress_to_proto(progress)))
            .map_err(|_| Status::cancelled("VerifyReplay client disconnected"))
    })) {
        Ok(()) => {}
        Err(status) => {
            let _ = tx.blocking_send(Err(status));
        }
    }
});

Ok(Response::new(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx))
    as Self::VerifyReplayStream))
```

Research reference: `tokio-spawn-blocking-service-work.md:15-23` and `tokio-spawn-blocking-service-work.md:27-30` call out non-abortable blocking work, unbounded CPU-heavy requests, and coarse locks held across slow work. `tokio-channel-streaming-deadlocks.md:19-25` supports `stream::iter` only when materialized output is the desired behavior; here the proto says `EpochOk` is "streamed per epoch" (`proto/hypervisor.proto:342`).

### I3 - Divergence mapping ignores the bisection request and uses undocumented field encodings

Severity: Important
Path/lines: `crates/dh-worker/src/service.rs:2281`, `crates/dh-worker/src/service.rs:616`, `crates/dh-worker/src/service.rs:623`; proto contract at `proto/hypervisor.proto:338`, `proto/hypervisor.proto:352`

`VerifyReplayRequest.bisect_on_divergence` is consumed with the request but never read. Regardless of whether the client asks for bisection, `verify_progress_to_proto` maps a phase-1 divergence into the M8-shaped proto by setting `icount_lo == icount_hi == at_icount`, setting RIPs to zero, and stuffing the 32-byte expected hash plus 32-byte actual hash into `Divergence.reg_diff`. The proto says `reg_diff` is a postcard-encoded `Vec<RegDiff{name, expected, actual}>`, and `dh-verify` documents that phase-1 divergence only has epoch/hash-pair data while bisection fields do not exist yet (`crates/dh-verify/src/verify.rs:6-15`). The current response looks like a successful bisection result but contains bytes clients cannot decode according to the field contract. The `first_bad_epoch.unwrap_or(u64::MAX)` sentinel also invents an epoch for END-identity or reseal divergences, where `dh-verify` explicitly says no epoch should be blamed (`crates/dh-verify/src/verify.rs:27-35`).

Suggested fix: handle the request flag explicitly and return an honest phase-1 payload until M8 exists. Either reject `bisect_on_divergence = true` as unimplemented, or implement real bisection before populating M8 fields. For phase-1 non-bisected divergence, do not encode raw hash pairs in `reg_diff`; keep M8-only fields empty/zero and include the hash-pair detail in a documented string or add a proper proto field in a separate schema change.

```rust
let request = request.into_inner();
let bisect_on_divergence = request.bisect_on_divergence;
if bisect_on_divergence {
    return Err(Status::unimplemented(
        "VerifyReplay divergence bisection is M8 and is not implemented yet",
    ));
}

// Phase-1 mapping:
Msg::Divergence(proto::Divergence {
    first_bad_epoch: first_bad_epoch.unwrap_or(0),
    icount_lo: at_icount,
    icount_hi: at_icount,
    rip_expected: 0,
    rip_actual: 0,
    reg_diff: Vec::new(),
    diff_page_idx: Vec::new(),
    suspected_cause: format!("{what}; expected_hash={}; got_hash={}", hex32(expected), hex32(got)),
})
```

Research reference: `tonic-prost-codegen.md:22-30` notes that proto3 scalar fields have defaults and required semantics must be enforced at the application boundary. `tonic-prost-codegen.md:25-27` also warns against silently changing field semantics under stable tags.
