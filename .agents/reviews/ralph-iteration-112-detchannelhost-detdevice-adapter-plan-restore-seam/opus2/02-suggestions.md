# Suggestions

## Suggestion: add fork coverage for detchannel-equipped buses

- File: `crates/dh-worker/src/fork_engine.rs:128`
- File: `crates/dh-worker/tests/fork_engine.rs:107`

`fork_slot` applies the same DHSNAP through `apply_dhsnap`, but the new EVTC coverage is only in `restore_engine.rs`. A fork-specific test with parent and child buses each containing `DetChannelDevice<VmMem, ...>` would protect the important memory-handle assumption: the child adapter must be built over the child slot's `guest_mem`, not cloned from the parent host. That is especially easy to get wrong once slot-manager composition starts wiring detchannel into real runtime buses.

## Suggestion: make the detchannel bus composition seam explicit

- File: `crates/dh-vmm/src/recording.rs:19`
- File: `crates/dh-worker/src/service.rs:1040`
- File: `crates/dh-worker/tests/restore_engine.rs:37`

The adapter is implemented, but production/test runtime bus composition still has scattered knowledge: `recording.rs` says detchannel is "NOT HERE", `runtime_test_bus` omits it, and the restore test picks `0xD000_5000` locally. If EVTC is intentionally adapter-only for now, a short comment or constant would reduce future confusion. If it is meant to be part of runtime composition soon, centralizing the base and construction helper would make the slot-manager handoff less fragile.

## Suggestion: avoid invoking the restore-plan factory before basic EVTC validation

- File: `crates/dh-devices/src/detchannel.rs:199`

`DetChannelDevice::restore` calls `restore_plan` before `DetChannelHost::restore` has checked version and length. The current test factory is side-effect-light, but a future replay plan factory may own cursors or counters. Keeping factory invocation after cheap section validation would make malformed sections less likely to perturb retry/debug state after a failed restore.

## Suggestion: cover malformed EVTC through the generic restore-engine path

- File: `crates/dh-worker/tests/restore_engine.rs:694`

The engine's `mis_shaped_containers_are_rejected_loudly` suite mutates generic sections, but it does not corrupt EVTC contents. Once `DetChannelHost::restore` is stricter, add a crafted EVTC mutation there too. That proves the generic device loop surfaces detchannel section rejection with the same loud `Codec` behavior as `CLKD`, `ENTR`, and `VCPU`.
