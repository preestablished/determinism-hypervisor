# Resolution Sequence

## 1. Correct The Operational Dependency

The current bead graph is inverted:

- `4s9.20` owns "Route worker CreateVm BzImage boot through the Linux loader".
- `4s9.18` acceptance cannot run until that worker BzImage seam exists.
- `4s9.20` currently depends on `4s9.18`, so neither bead is naturally ready for `/ralph`.

Preferred session setup:

- Rewire the beads so `4s9.20` is a prerequisite of `4s9.18`.
- Finish `4s9.20` first, or explicitly fold its implementation scope into `4s9.18` and then close/supersede `4s9.20` with a clear note.

The cleaner path is to rewire and complete `4s9.20` first, because it owns the worker boot seam directly. `4s9.18` can then stay focused on the selected Linux deterministic device contract.

## 2. Make Worker Linux Boot Runnable

Implement the minimal public worker path needed by the acceptance test:

- In `boot_slot`, route `ResolvedBoot::BzImage { kernel, initramfs, cmdline }` to `dh_vmm::boot::load_bzimage_and_enter`.
- Preserve the existing ELF path and status mapping.
- Map Linux loader failures to `FAILED_PRECONDITION` with a `BzImage boot:` prefix.
- Add focused service tests that prove BzImage CreateVm succeeds with the Linux fixture and ELF CreateVm still works.

This should be done before writing `linux_worker_api.rs`, otherwise the acceptance target can only assert the known blocker.

## 3. Make Worker NextSdkEvent Runnable

Wire the public `RunRequest.next_sdk_event` field into existing VMM run control:

- Convert `WireUntil::NextSdkEvent(filter)` to `dh_vmm::runctl::Until::NextSdkEvent { hard_cap }`.
- Pass a `Cell<u64>` SDK-event feed into `Segment::sdk_events` only for this mode.
- In the worker exit handler, after `service_exit_with_detchannel` drains events, increment the feed only for events matching the optional stream filter.
- Capture the first matching event and return it in `RunResponse.sdk_event` when the stop reason is `NEXT_SDK_EVENT`.
- Retain all drained events in `runtime.guest_events` so `StreamGuestEvents` can still inspect ordering evidence.

This makes Ready EventKind 14 observable through the public worker API instead of through serial text or private test hooks.

## 4. Include Bus Device State In Run And Replay Hashes

The snapshot engine already serializes bus devices, and `dh_vmm::hash::device_sections(&bus)` already produces canonical bus-device preimage bytes for the state hash. DHSNAP and the state hash do not use identical bytes: DHSNAP writes canonical tag order and folds pv-entropy regs into `ENTR`, while `hash::device_sections` frames bus devices in bus/base order. The worker run and replay paths should have the same deterministic state coverage:

- Build a combined hash preimage from `lapic_section(&lapic)` followed by `device_sections(&bus)`.
- Use that same helper in `crates/dh-worker/src/service.rs` and every lapic-only hash site in `crates/dh-worker/src/replay_engine.rs`.
- Keep ordering identical in run and replay.

This is required for the `4s9.18` acceptance phrase "snapshot/hash/replay" to be meaningful for pv-blk overlay and detchannel attachment state.

## 5. Add The Linux Worker API Test Target

Create `crates/dh-worker/tests/linux_worker_api.rs` as an ignored, artifact-gated worker integration test.

The target should:

- Use `M9LinuxArtifacts::from_env_required("linux_worker_api")`.
- Populate `DH_M9_IMAGE_CACHE` with BLAKE3-keyed entries for the artifacts using the existing resolver cache key scheme.
- Set `MachineConfig.base_image_hash` to the BLAKE3 of `DH_M9_GAME_IMAGE`, because current worker schema has one pv-blk backing file and that backing is the Linux-visible `/dev/vdb` game image for this bead.
- Treat `DH_M9_BASE_IMAGE` as fixture context unless a prerequisite multi-disk/root-vs-game schema bead lands before `4s9.18`.
- Build a `MachineConfig` with `BootSpec::BzImage` and the selected M9 device set.
- Use the public worker API: `CreateVm`, `Run`, `StreamGuestEvents`, `TakeSnapshot`, `RestoreSnapshot`, and `VerifyReplay`.

## 6. Prove The Selected Device Contract

The initial acceptance test should be named `pvblk_dev_vdb` to match the bead.

It should prove:

- `CreateVm` succeeds with BzImage boot and selected deterministic device set.
- `Run(next_sdk_event stream=14)` stops at guest-sdk Ready, not serial output.
- Detchannel `EVTC` or equivalent logged attach evidence proves CHANNEL_INIT succeeded; streamed guest events contain Hello, expected region registration evidence, and Ready in order.
- No external host-injected input lands before Ready: no ring-C/ring-I pushes, no `PAD_SET`, and no scheduled `DeviceEvent` or `NetRx` before the Ready event.
- The Linux fixture's Ready implies the control leg reached `LoadGame{dev_path="/dev/vdb"}` and `Start{}` with expected regions registered.
- The source `DH_M9_GAME_IMAGE` bytes and mtime do not change.
- pv-blk overlay/register state, EVTC host attach/producer-seq state, and the guest-RAM channel page survive snapshot/restore and replay with stable state hashes.
- Canonical cmdline and CPUID masking remove supported host time/entropy surfaces; replay/hash equality and fixture evidence catch guest-contract violations such as raw host entropy/time use.

If the current Linux fixture does not expose enough evidence for `/dev/vdb`, host-time, or host-entropy assertions, stop and update the fixture contract rather than weakening the worker test.

## 7. Close Or Hand Off Cleanly

After the acceptance command passes, update beads:

- Close `4s9.18` with the exact command evidence.
- If `4s9.20` was completed as a prerequisite, close it separately.
- If `4s9.20` was folded into `4s9.18`, close or supersede it with a note explaining that the BzImage CreateVm seam landed in `4s9.18`.
- Re-run `bd ready` so the next session sees `4s9.21`, `4s9.22`, or `4s9.23` naturally.
