# Implementation Notes

## Worker BzImage Boot

File: `crates/dh-worker/src/service.rs`

Change `boot_slot` from:

```rust
ResolvedBoot::BzImage { .. } => Err(Status::unimplemented(...))
```

to a call shaped like:

```rust
ResolvedBoot::BzImage {
    kernel,
    initramfs,
    cmdline,
} => dh_vmm::boot::load_bzimage_and_enter(slot, &kernel, &initramfs, &cmdline)
    .map(|_| ())
    .map_err(|e| Status::failed_precondition(format!("BzImage boot: {e}"))),
```

Do not re-canonicalize cmdline bytes here. `proto_map` and `MachineConfig` own canonicalization before config hashing.

## Worker NextSdkEvent

Files:

- `crates/dh-worker/src/service.rs`
- `crates/dh-worker/src/proto_map.rs` only if extra mapping helpers are useful.

Implementation shape:

- Split `until_from_run_request` into a small result struct if needed:
  - `until: dh_vmm::runctl::Until`
  - `sdk_event_filter: Option<Option<u32>>`
- For `RunRequest.next_sdk_event.stream`:
  - absent means any SDK event.
  - present means exact detchannel EventKind.
  - `hard_icount_cap == 0` still maps to the worker default cap.
- Add a local matcher:

```rust
fn sdk_event_matches(filter: Option<u32>, event: &DrainedGuestEvent) -> bool {
    filter.map_or(true, |stream| event.stream == stream)
}
```

- In the `on_exit` closure:
  - drain events via `service_exit_with_detchannel`.
  - for matching events, increment the `Cell<u64>` feed.
  - store the first matching event for `RunResponse.sdk_event`.
  - append all drained events to the runtime retention list after run completes.

Important: `runctl::Until::NextSdkEvent` stops when the feed rises during that segment. Do not poll `runtime.guest_events`; the run-control feed must be driven synchronously from the doorbell exit that drained the matching event.

## RunResponse.sdk_event

When the run stops with `StopReason::NextSdkEvent`, return the first matching event as:

```rust
proto::GuestEvent {
    stream,
    icount,
    vns,
    payload,
}
```

Use the cumulative event icount that `service_exit_with_detchannel` already computes, not the segment-local perf counter value.

If the run hard-caps before a matching event, keep `sdk_event: None` and return the hard-cap stop reason.

## Hash Device Sections

Files:

- `crates/dh-worker/src/service.rs`
- `crates/dh-worker/src/replay_engine.rs`

Add or reuse a small helper with exactly one byte order:

```rust
fn runtime_hash_device_sections(
    bus: &dh_devices::MmioBus,
    lapic: &dh_vmm::lapic::LocalApic,
) -> Vec<u8> {
    let mut bytes = dh_vmm::hash::lapic_section(lapic);
    bytes.extend_from_slice(&dh_vmm::hash::device_sections(bus));
    bytes
}
```

Use it wherever the worker currently hashes only `lapic_section(&lapic)`.

Rationale:

- snapshot capture already writes bus device sections.
- replay already restores bus device sections.
- `4s9.18` requires pv-blk overlay and detchannel state to participate in snapshot/hash/replay proof.

## MachineConfig Device Set

The Linux worker API test should request only deterministic M9 surfaces needed before Ready:

- `DEVICE_ID_DETCHANNEL`
- `DEVICE_ID_PV_CLOCK`
- `DEVICE_ID_PV_PAD`
- `DEVICE_ID_PV_ENTROPY`
- `DEVICE_ID_PV_BLK`
- `DEVICE_ID_DEBUG_SERIAL`

Do not add virtio-blk. Do not make serial a readiness condition.

If device order affects bus registration or hash bytes, make the test use the same canonical order as production M9 configs.

## Artifact Cache Population

Use the existing M9 artifact helper and image resolver cache-key scheme. The test should not copy large artifacts into the repo.

Expected env vars:

- `DH_M9_BZIMAGE`
- `DH_M9_INITRAMFS`
- `DH_M9_BASE_IMAGE`
- `DH_M9_GAME_IMAGE`
- `DH_M9_IMAGE_CACHE`

The test may create hardlinks or copies inside `DH_M9_IMAGE_CACHE`, but it must not mutate the source artifact files. Use noninteractive file operations.

## Fixture Evidence

The host-visible proof for `/dev/vdb` should come from the guest-sdk control contract:

- `boot.toml` carries `game_dev = "/dev/vdb"`.
- The guest agent calls `LoadGame{dev_path}` with that value.
- Ready is withheld until `LoadGame`, `Start`, and expected region registration have succeeded.

If the fixture also emits a diagnostic evidence event or exposes a `meta` region with the loaded device path and game image digest, parse and assert it. If not, do not infer more than the documented Ready contract supports.
