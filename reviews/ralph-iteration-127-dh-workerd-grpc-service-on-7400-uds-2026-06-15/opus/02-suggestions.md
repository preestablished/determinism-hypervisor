# Suggestions

### S1. Add service-level tests for divergence and validation paths

Path/line: `crates/dh-worker/src/service.rs:3097`

The added test exercises the stored-input-log success path, which is useful, but it does not cover the service-specific behavior for divergent logs, missing `log`, bad `input_log_id` length, invalid SILG containers, invalid DHILOG bytes, or oversized inline logs. Lower-level replay-engine tests cover divergence at the library layer, but the new code adds service-specific mapping and streaming behavior that can regress independently.

Suggested additions:

```rust
#[cfg(target_arch = "x86_64")]
#[tokio::test]
async fn verify_replay_rejects_missing_log_before_engine_work() {
    let svc = WorkerService::new(test_config(1)).unwrap();
    let err = svc
        .verify_replay(Request::new(proto::VerifyReplayRequest {
            base: Some(proto::SnapshotRef { hash: vec![0x11; 32] }),
            log: None,
            bisect_on_divergence: false,
        }))
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}

#[cfg(target_arch = "x86_64")]
#[tokio::test]
async fn verify_replay_rejects_short_input_log_id() {
    let svc = WorkerService::new(test_config_with_resources(
        1,
        std::env::temp_dir(),
        Some(test_transport()),
    ))
    .unwrap();
    let err = svc
        .verify_replay(Request::new(proto::VerifyReplayRequest {
            base: Some(proto::SnapshotRef { hash: vec![0x11; 32] }),
            log: Some(proto::verify_replay_request::Log::InputLogId(vec![0; 31])),
            bisect_on_divergence: false,
        }))
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}
```

### S2. Stream progress as it is produced instead of buffering the whole report

Path/line: `crates/dh-worker/src/service.rs:2364`

`VerifyReplay` is a server-streaming RPC, but the implementation waits for the entire blocking verification to finish, collects every progress event into a `Vec`, and only then returns `tokio_stream::iter`. For long logs, clients receive no `EpochOk` progress until after the work is complete, and the worker buffers all progress in memory.

This is non-blocking for the current branch because `dh_verify` currently returns a collected `VerifyReport`, but the service surface should eventually use a blocking-aware channel or verifier callback so progress is visible during the long replay.

Research reference:
- `/home/infra-admin/.claude/research/tokio-spawn-blocking-service-work.md:17`
- `/home/infra-admin/.claude/research/tokio-channel-streaming-deadlocks.md:22`
- `/home/infra-admin/.claude/research/tokio-channel-streaming-deadlocks.md:49`

Suggested direction:

```rust
let (tx, rx) = tokio::sync::mpsc::channel(64);
tokio::task::spawn_blocking(move || {
    verify_replay_with_progress(..., |event| {
        tx.blocking_send(Ok(verify_progress_to_proto(event)?))
            .map_err(|_| Status::cancelled("VerifyReplay stream closed"))
    })
});
Ok(Response::new(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx))))
```

### S3. Avoid parsing the same DHILOG multiple times

Path/line: `crates/dh-worker/src/service.rs:2302`

The service parses the DHILOG to reconstruct a `LogWriter`, then `crates/dh-worker/src/verify_replay.rs:52` and `crates/dh-worker/src/replay_engine.rs:96` parse the same bytes again. The repeated parse is not a correctness bug, but it increases CPU cost and spreads header ownership across layers.

Suggested direction:
- Add a replay-engine helper that accepts a parsed header or returns the writer seed from the same parse used by replay.
- Alternatively, make `verify_replay` construct the replay `LogWriter` internally from the parsed header so the service stays thinner.

### S4. Document the coarse divergence encoding if it remains before M8

Path/line: `crates/dh-worker/src/service.rs:623`

If the branch intentionally keeps pre-M8 coarse divergence reports, document the temporary encoding in code and tests. In particular, `reg_diff` currently contains two 32-byte hashes, not the proto-described postcard `Vec<RegDiff>`, and `first_bad_epoch == u64::MAX` is an undocumented sentinel.

Suggested direction:
- Add a named helper such as `coarse_divergence_to_proto`.
- Pin the sentinel in a comment and CLI test, or avoid the sentinel by using a documented value and `suspected_cause`.
- Add a follow-up bead for replacing this with true M8 bisection.
