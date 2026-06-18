# Linux Worker API Tests

Target file: `crates/dh-worker/tests/linux_worker_api.rs`

Primary command from the bead:

```bash
DH_M9_ALLOW_SKIP=0 cargo test -p dh-worker --test linux_worker_api --release pvblk_dev_vdb -- --ignored --nocapture
```

## Test Structure

Create one ignored test named `pvblk_dev_vdb`. Keep helper functions private to the test target.

Suggested helpers:

- `m9_artifacts() -> M9LinuxArtifacts`
- `populate_image_cache(&M9LinuxArtifacts) -> CachedHashes`
- `linux_machine_config(&CachedHashes) -> MachineConfig`
- `create_linux_vm(worker, config) -> Lease`
- `run_until_ready(worker, lease) -> GuestEvent`
- `stream_events(worker, lease) -> Vec<GuestEvent>`
- `take_restore_replay(worker, lease) -> ReplayEvidence`

## Acceptance Assertions

CreateVm:

- Returns a lease for BzImage Linux boot.
- Runtime `MachineConfig` hash is stable across repeated construction from identical artifacts.
- Selected device set includes pv-blk and detchannel and excludes any virtio-blk path.

Run to Ready:

- `RunRequest.until.next_sdk_event.stream = 14`.
- Response stop reason is `NEXT_SDK_EVENT`.
- `RunResponse.sdk_event.stream == 14`.
- Ready payload length is 16 bytes and decodes as `unit`, `region_count`, and `manifest_generation`.
- `region_count` matches the fixture's expected regions.

Ordering:

- Streamed guest events show the detchannel lifecycle before Ready:
  - CHANNEL_INIT attached successfully.
  - Hello appears before Ready.
  - Ready appears after the fixture's autostart/control leg.
- No `InjectInputs` call is made before Ready.
- The sealed input log for the segment has no host input records before the Ready SDK event.

`/dev/vdb` and base image:

- The fixture reaches Ready only after `LoadGame{dev_path="/dev/vdb"}`.
- If the fixture exposes metadata, assert the loaded game path and digest.
- Record the base image source metadata before CreateVm.
- After the run and snapshot/replay sequence, assert source base image bytes and mtime are unchanged.
- If the fixture exercises writes, assert they land in pv-blk overlay state or are rejected by the guest read-only path, depending on the fixture contract. Do not permit host base mutation.

Snapshot/hash/replay:

- Take a snapshot at the Ready boundary.
- Restore the snapshot into a fresh slot.
- Run or verify replay from the same base and input log.
- Assert end state hashes match.
- Assert DHSNAP contains the expected deterministic device sections:
  - `EVTC` for detchannel attachment.
  - `BLKO` for pv-blk overlay state, empty or non-empty according to fixture behavior.
  - `CLKD`, `PADD`, and `ENTR` for clock, pad, and entropy.
- Assert replay uses the same combined lAPIC plus bus-device state-hash preimage as live run.

Host entropy and host time:

- The guest-visible clock source must be `dh-pvclock` under the canonical M9 cmdline.
- The fixture should not be able to consume host wall-clock time as a readiness input.
- The fixture should not be able to consume host entropy as a readiness input.
- Prefer fixture evidence through a meta region or event. If unavailable, rely only on existing VMM gates and file a follow-up fixture evidence bead rather than weakening this assertion silently.

## Supporting Tests

Run smaller tests before the ignored Linux acceptance test:

```bash
cargo test -p dh-worker service:: -- --nocapture
cargo test -p dh-worker --test m5_record_replay -- --nocapture
cargo test -p dh-worker --test snapshot_engine -- --nocapture
cargo test -p dh-worker --test replay_engine -- --nocapture
```

For hash-sensitive changes, run the relevant worker tests more than once under normal workspace load. The project memory notes that single-pass determinism verification has missed flakes before.
