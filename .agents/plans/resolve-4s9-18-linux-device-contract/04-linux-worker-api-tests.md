# Linux Worker API Tests

Target file: `crates/dh-worker/tests/linux_worker_api.rs`

Primary command from the bead:

Requires `DH_M9_BZIMAGE`, `DH_M9_INITRAMFS`, `DH_M9_BASE_IMAGE`, `DH_M9_GAME_IMAGE`, and `DH_M9_IMAGE_CACHE` to be exported as documented in `docs/ops/test-partitioning.md`. With `DH_M9_ALLOW_SKIP=0`, missing artifacts are fatal.

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
- `MachineConfig.base_image_hash` is the BLAKE3 of `DH_M9_GAME_IMAGE`, the single pv-blk backing exposed as `/dev/vdb`.
- Selected device set includes pv-blk and detchannel and excludes any virtio-blk path.

Run to Ready:

- `RunRequest.until.next_sdk_event.stream = 14`.
- Response stop reason is `NEXT_SDK_EVENT`.
- `RunResponse.sdk_event.stream == 14`.
- Ready payload length is 16 bytes and decodes as `unit`, `region_count`, and `manifest_generation`.
- `region_count` is at least the fixture's required expected-region count.
- All expected regions and layout versions are live in the manifest, and the Ready payload `manifest_generation` matches the manifest generation observed after restore.

Ordering:

- Streamed guest events show guest-sdk publication before Ready:
  - Hello appears before Ready.
  - Expected region registration evidence appears before Ready.
  - Ready appears after the fixture's autostart/control leg.
- CHANNEL_INIT is not itself a streamed guest event; prove successful attach through `EVTC` snapshot state, logged PIO init status, or successful subsequent Hello/Ready on the attached detchannel.
- No `InjectInputs` call is made before Ready.
- The sealed input log has no external host-injected input before the Ready SDK event: no ring-C/ring-I pushes, no `PAD_SET`, and no scheduled `DeviceEvent` or `NetRx`. Expected detchannel servicing records such as PIO answers or consumer bumps may appear before Ready.

`/dev/vdb` and backing image:

- The fixture reaches Ready only after `LoadGame{dev_path="/dev/vdb"}`.
- If the fixture exposes metadata, assert the loaded game path and digest.
- Record the `DH_M9_GAME_IMAGE` source metadata before CreateVm.
- After the run and snapshot/replay sequence, assert source `DH_M9_GAME_IMAGE` bytes and mtime are unchanged.
- If the fixture exercises writes, assert they land in pv-blk overlay state or are rejected by the guest read-only path, depending on the fixture contract. Do not permit source backing file mutation.

Snapshot/hash/replay:

- Take an initial root snapshot immediately after CreateVm to rotate the active DHILOG base away from all-zero `base_snapshot_id`.
- Run to Ready.
- Take the Ready snapshot with `seal_input_log=true`.
- Restore the snapshot into a fresh slot.
- Verify replay with `base = initial_snapshot` and `log = ready_snapshot.input_log_id`; this must match the DHILOG header's `base_snapshot_id`.
- Assert end state hashes match.
- Assert DHSNAP contains the expected deterministic device sections:
  - `EVTC` for detchannel host attach and producer-seq state.
  - guest RAM pages preserve the channel rings, manifest, and indexes.
  - `BLKO` for pv-blk registers plus overlay dirty clusters, with dirty count possibly zero.
  - `CLKD`, `PADD`, and `ENTR` for clock, pad, and entropy.
  - `SERL` if debug serial remains in the selected device set.
- Assert replay and live run cover the same deterministic lAPIC plus bus-device state in the state hash.

Host entropy and host time:

- The guest-visible clock source must be `dh-pvclock` under the canonical M9 cmdline.
- The slot CPUID table must mask supported host entropy/time surfaces according to the Linux compatibility policy.
- Fixture metadata or events must not show forbidden host wall-clock or host entropy use as a readiness input.
- Replay/hash equality is part of this proof. Raw `RDTSC`, `RDRAND`, or other bypasses are guest-contract violations that should be caught by verification or filed as fixture evidence work rather than treated as proven absent by this test alone.

## Supporting Tests

Run smaller tests before the ignored Linux acceptance test:

```bash
cargo test -p dh-worker service:: -- --nocapture
cargo test -p dh-worker --test m5_record_replay -- --nocapture
cargo test -p dh-worker --test snapshot_engine -- --nocapture
cargo test -p dh-worker --test replay_engine -- --nocapture
```

For hash-sensitive changes, run the relevant worker tests more than once under normal workspace load. The project memory notes that single-pass determinism verification has missed flakes before.
