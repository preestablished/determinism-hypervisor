# Risks And Debugging

## Slot-Core Drift

Risk: using `common::m9_linux_ready_snapshot_with_config(test_name, slots, ...)` directly ignores `DH_M7_ACCEPT_SLOT_CORES=2-5` and runs on cores `0..slots`.

Mitigation:

- add the explicit-core helper in `common/mod.rs`;
- verify the worker info or log the selected slot cores in Linux M7 mode;
- keep `DH_M7_ACCEPT_SLOT_CORES=2-5` in every Linux acceptance command.

## READY Lease Lifetime

Risk: the READY root lease or store resources drop while children still need the root snapshot or snapstore.

Mitigation:

- keep the full `M9LinuxReady` object owned by the harness until all batches and cross-slot checks finish;
- destroy the root lease only after all child leases have been destroyed;
- call `GetWorkerInfo` after cleanup and assert all slots are free.

## Distinct Seeds May Not Mean Distinct Linux State

Risk: the Linux post-READY workload may not consume fork entropy. Distinct child seeds could still produce identical child state hashes and input logs.

Mitigation:

- preserve `child_seed(index)` for all Linux forks;
- require same-seed cross-slot equality;
- do not require `unique_hashes.len() == jobs` for Linux;
- print unique count for observability.

## Linux DHILOGs Are Not Pad Logs

Risk: carrying over nanokernel PAD-only validation will reject valid Linux logs.

Mitigation:

- split log validation by guest mode;
- for Linux, validate lineage, header identity, epoch hashes, frame marks, and VerifyReplay;
- let `LogReader` enforce unknown canonical rejection.

## Frame Mark Off-By-One

Risk: frame marks store absolute `FRAME_COUNTER` values, not elapsed frame counts. A Linux root at READY may already have a nonzero frame counter.

Mitigation:

- store the READY/root `frame_counter`;
- require `child.frame_counter == ready_frame_counter + M9_LINUX_CHILD_FRAMES`;
- require the parsed frame table to equal `ready_frame_counter + 1..=ready_frame_counter + M9_LINUX_CHILD_FRAMES`;
- check frame-mark icounts are strictly increasing;
- compare `RunResponse.frames_elapsed` to `M9_LINUX_CHILD_FRAMES`.

## Counter Domain Mixups

Risk: worker RPC responses expose cumulative counters, while sealed DHILOG headers, lineage, and `VerifyReplay.Done.total_icount` use the segment counters reset at `TakeSnapshot`.

Mitigation:

- store `root_cumulative_icount = ready.ready_snapshot.icount` and `root_cumulative_vns = ready.ready_snapshot.vns`;
- compute Linux child segment counters as `run.icount - root_cumulative_icount` and `run.vns - root_cumulative_vns`;
- use segment counters for DHILOG header, lineage, and VerifyReplay checks;
- use cumulative counters only for worker-position observability.

## VerifyReplay Divergence

Risk: Linux child replay diverges even though M5 post-READY replay passes.

First triage steps:

1. Re-run the small Linux smoke with `DH_M7_ACCEPT_JOBS=1`.
2. Confirm the child DHILOG header base snapshot is the READY snapshot, not the initial boot snapshot.
3. Confirm `TakeSnapshot` after READY reset the segment counter and sealed boot logs before children fork.
4. Confirm the Linux child segment end counter is computed as `run.icount - ready.ready_snapshot.icount`.
5. Compare live child snapshot hash to `VerifyReplay.Done.end_state_hash`.
6. If `VerifyReplay` emits `Divergence`, rerun with `bisect_on_divergence = Some(true)` locally to get checkpoint evidence.
7. Use existing snapshot comparison/debug helpers from the prior M9 Linux work to compare live vs replay state at the first divergent epoch.

Likely root-cause areas:

- fork restore path did not preserve a Linux device state section;
- Linux child was forked from the wrong base snapshot;
- frame marks or pause-drained detchannel events are ordered differently on replay;
- slot dirty-ring reset differs between child slots;
- the explicit-core helper accidentally changed M9 worker setup or image cache resolution.

## Nightly Runtime

Risk: adding a Linux 100-child canary makes nightly too long for the single KVM runner.

Mitigation:

- add Linux canary as a separate job depending on `determinism-class`;
- target only `m7_accept_1000_seeded_forks_verify_replay_all`;
- keep `DH_M7_ACCEPT_JOBS=100`;
- do not run cross-slot nightly unless measured runtime is acceptable;
- leave full 1000-child plus cross-slot acceptance operator-run.

## Shared Helper Regression

Risk: refactoring `common::m9_linux_ready_snapshot_with_config` breaks `m5_record_replay`.

Mitigation:

- keep the existing helper signatures;
- delegate them to the explicit-core helper;
- run the Linux M5 corpus gate after the helper refactor and before the M7 implementation is considered complete.
