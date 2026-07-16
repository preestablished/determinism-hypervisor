# Findings

## High: M6 acceptance can pass while the service DHILOG contains no epoch hashes

The test claims to validate service DHILOG epoch records, but it only compares two extracted vectors:

- `crates/dh-worker/src/service.rs:3277` extracts `epoch_hashes` from each sealed service log.
- `crates/dh-worker/src/service.rs:4058` compares `captured.epoch_hashes` to `plain.epoch_hashes`.

There is no service-level assertion that either vector is non-empty. The only non-empty assertion is at `crates/dh-worker/src/service.rs:3984` through `crates/dh-worker/src/service.rs:3993`, but that uses the standalone `capture_epoch_leg` helper. That helper calls `dh_vmm::runctl::run_segment_with_epochs` directly at `crates/dh-worker/src/service.rs:3172`, not the `WorkerService::run` path that the bead acceptance is supposed to validate.

The production service path currently confirms the gap:

- `crates/dh-worker/src/service.rs:2449` calls `dh_vmm::runctl::run_segment_with_scheduled_inputs_and_frames`.
- `crates/dh-vmm/src/runctl.rs:296` through `crates/dh-vmm/src/runctl.rs:317` implements that wrapper with `&mut |_, _, _| Ok(())` as the epoch sink.

So the service `Run` path discards epoch callbacks. Both capture and no-capture service logs can therefore contain zero `EPOCH_HASH` records and still satisfy `[] == []`. The targeted test passes on this host despite that wiring, which makes this an acceptance false positive for the bead's "epoch hashes" requirement.

Suggested fix: either route `WorkerService::run` through a scheduled-inputs runctl entry point that accepts/logs an epoch sink, or add such an entry point. Then assert `!plain.epoch_hashes.is_empty()` and `!captured.epoch_hashes.is_empty()` in the service-level leg before comparing equality.

