# Positive Notes

### P1. Stored input-log IDs are validated before use

Path/line: `crates/dh-worker/src/service.rs:501`

`log_id_from_bytes` requires exactly 32 bytes and returns `InvalidArgument` before constructing a `snapstore_types::LogId`. This preserves the wire boundary and avoids passing malformed IDs into snapshot-store.

### P2. SILG containers are decoded and version-checked before extracting DHILOG payloads

Path/line: `crates/dh-worker/src/service.rs:510`

The `input_log_id` path fetches a SILG container, decodes it with `snapstore_manifest::input_log::InputLogContainer`, checks the inner DHILOG format version, and only then passes the payload to the replay path. That is the right boundary between snapshot-store container format and worker DHILOG parsing.

### P3. The replay log writer is seeded from the actual recording header

Path/line: `crates/dh-worker/src/service.rs:523`

`log_writer_from_reader_header` copies the original segment header fields into the writer used during replay. That preserves the replay engine's byte-identical reseal check instead of accidentally resealing with a service-generated header.

### P4. Blocking KVM work is at least kept off the async runtime worker

Path/line: `crates/dh-worker/src/service.rs:2288`

The service uses the existing `blocking_lifecycle` helper rather than running KVM and store work directly inside the tonic async method. The resource accounting needs tightening, but the async boundary itself follows the right high-level pattern.

### P5. The new test exercises the real KVM and snapshot-store path

Path/line: `crates/dh-worker/src/service.rs:3097`

`verify_replay_rpc_streams_done_for_stored_input_log` creates a VM, snapshots it, restores it into another slot, runs a segment, seals an input log into snapshot-store, and verifies by `input_log_id`. That is a much more valuable test than a pure mapper/unit test for this RPC's happy path.
