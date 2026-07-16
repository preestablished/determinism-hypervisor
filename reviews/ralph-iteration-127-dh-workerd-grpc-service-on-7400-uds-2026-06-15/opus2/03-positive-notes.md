# Positive Notes

### P1 - Stored input logs are decoded through the manifest container

Path/lines: `crates/dh-worker/src/service.rs:510`, `crates/dh-worker/src/service.rs:513`

The `input_log_id` path does not treat snapshot-store bytes as raw DHILOG. It decodes `InputLogContainer` and verifies `inner_version` against `dh_inputlog::DHILOG_FORMAT_VERSION` before replaying the payload, which is the right boundary between snapstore storage format and the replay engine.

### P2 - The replay writer is seeded from the input log header

Path/lines: `crates/dh-worker/src/service.rs:523`

`log_writer_from_reader_header` rebuilds the `LogWriter` from the actual DHILOG header instead of creating a new default segment header. That preserves base snapshot, entropy seed, machine config hash, clock ratio, and encoder fingerprint, which is necessary for the replay engine's byte-identical reseal check.

### P3 - Blocking work is kept off the async reactor

Path/lines: `crates/dh-worker/src/service.rs:2288`

Even though the slot ownership of this work needs correction, the implementation correctly recognizes that KVM, snapstore, and log parsing are synchronous work and places them behind `blocking_lifecycle` rather than running them directly on tonic's async task.

### P4 - The new test exercises the real service boundary

Path/lines: `crates/dh-worker/src/service.rs:3097`, `crates/dh-worker/src/service.rs:3170`

The added test goes through `CreateVm`, `TakeSnapshot`, `RestoreSnapshot`, `Run`, `TakeSnapshot`, and finally the `VerifyReplay` RPC using a stored input log id. That is a good end-to-end shape for this service layer because it validates the snapstore container path and the tonic stream item shape together.

### P5 - Non-x86 behavior remains explicit

Path/lines: `crates/dh-worker/src/service.rs:2374`

The new implementation stays under `#[cfg(target_arch = "x86_64")]` and preserves the non-x86 `UNIMPLEMENTED` response, matching the rest of the hardware-gated service methods.
