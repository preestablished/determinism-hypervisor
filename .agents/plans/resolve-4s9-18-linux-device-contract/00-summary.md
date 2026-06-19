# Resolve determinism-hypervisor-4s9.18

Plan name: `resolve-4s9-18-linux-device-contract`

Target bead: `determinism-hypervisor-4s9.18` - Implement selected Linux deterministic device contract.

## Current Blocker

`4s9.18` is correctly scoped around the Linux-visible deterministic device contract, but its acceptance cannot run in the current repository because two lower-level worker seams are missing:

- `crates/dh-worker/tests/linux_worker_api.rs` does not exist.
- `crates/dh-worker/src/service.rs::boot_slot` still returns `UNIMPLEMENTED` for `ResolvedBoot::BzImage`.
- Worker `RunRequest.until.next_sdk_event` is also still `UNIMPLEMENTED`, so the worker cannot stop on guest-sdk Ready EventKind 14 through the public API.

There is already a deterministic pv-blk model at MMIO base `0xD000_4000`, and the worker bus builder already registers it when `DEVICE_ID_PV_BLK` is requested. The plan is therefore to make the selected pv-blk and detchannel contract executable through the worker API, not to add virtio-blk or accept serial-only readiness.

## Non-Negotiable Contract

- Linux boot is `bzImage + initramfs` through the deterministic Linux boot loader already present in `dh-vmm`.
- The game image path is the existing deterministic pv-blk device at `0xD000_4000`; the Linux guest fixture maps that device to `/dev/vdb`.
- Current worker schema has one pv-blk backing file through `MachineConfig.base_image_hash`. For this bead, `/dev/vdb` is backed by `DH_M9_GAME_IMAGE`; if `DH_M9_BASE_IMAGE` must also be attached as a separate disk/root image, file and complete a prerequisite multi-disk/schema bead first.
- The selected pv-blk backing file is immutable host input. Writes, if the fixture exercises them, must be overlay-only or rejected by the guest read-only path and must not mutate the source file.
- The accepted READY point is guest-sdk EventKind 14 `Ready{unit, region_count, manifest_generation}` on detchannel.
- Serial output is diagnostic only. It cannot satisfy M9 READY.
- No external host-injected input may land before the Ready event is drained.
- Snapshot, state hash, and replay must include deterministic device state relevant to pv-blk and detchannel.

## Plan Files

- `01-contract-sources.md` ties the plan to upstream and local authority documents.
- `02-sequence.md` orders the work so the bead becomes runnable before device assertions are added.
- `03-implementation-notes.md` names the concrete code seams and preferred implementation shape.
- `04-linux-worker-api-tests.md` defines the acceptance test target and assertions.
- `05-bead-graph-handoff.md` captures the current beads dependency inversion and the exact handoff for the next session.
- `06-exit-criteria.md` defines what must be true before closing `4s9.18`.
