# Resolve 4s9.27 Linux M5 Record/Replay Corpus

Plan name: `resolve-4s9-27-linux-m5-corpus`

Selected bead: `determinism-hypervisor-4s9.27` - Add Linux M5 record replay corpus and reverify gate.

## Why This Bead

`4s9.27` is the root remaining M9 implementation blocker. Its dependencies are closed, but the bead itself remains blocked because `crates/dh-worker/tests/m5_record_replay.rs` still contains only the Linux guard `linux_m5_record_replay_requires_real_linux_corpus`.

Closing `4s9.27` unblocks the downstream M9 chain:

- `4s9.29` Linux M7 fork VerifyReplay acceptance.
- `4s9.32` Phase 1 and Phase 2 exit-gate evidence.
- Through those, `4s9.31`, `4s9.33`, `4s9.34`, and final `4s9.35`.

## Reference Host Assumption

This repository is currently on the intended Linux/KVM reference host:

- Host observed while drafting: `Linux infra-control 6.8.0-124-generic #124-Ubuntu SMP PREEMPT_DYNAMIC Tue May 26 13:00:45 UTC 2026 x86_64`.
- `/dev/kvm` is readable and writable.
- The M9 staged artifacts are present in `/home/infra-admin/.cache/dh-m9/reference-workload/`.

The implementation agent should run the Linux acceptance commands locally on this machine. Do not downgrade the plan to an operator-only or skip-allowed workflow unless this host loses KVM access.

## Desired End State

Replace the Linux guard in `crates/dh-worker/tests/m5_record_replay.rs` with a real ignored Linux M5 corpus test that:

- boots the staged M9 Linux fixture to READY;
- records a deterministic post-READY segment from the READY snapshot;
- seals the segment through `TakeSnapshot`;
- verifies replay from `(READY snapshot, DHILOG)` with `VerifyReplay`;
- observes nonzero `EpochOk` progress;
- checks `VerifyDone.end_state_hash` equals the live snapshot state hash;
- checks the recorded DHILOG has at least one `EPOCH_HASH`, the expected END state hash, and a deterministic corpus manifest;
- either writes a checked-in corpus if size policy allows, or writes a small checked-in manifest plus documented artifact cache paths and hashes.

The plan intentionally uses the post-READY M9 workload already made deterministic by the previous plan:

- pv-pad `FRAME_MARK` records are emitted after READY.
- guest-driven pv-blk IO exists in frame 0.
- replay ordering for terminal frame marks and pause-drained detchannel events is already fixed.

## File Map

- `01-current-state.md` records the current blocker, relevant files, and known artifact hashes.
- `02-corpus-contract.md` defines the Linux corpus shape and metadata.
- `03-implementation-sequence.md` gives the coding sequence.
- `04-validation-and-evidence.md` lists commands and required assertions.
- `05-bead-and-git-handoff.md` covers Beads and session closeout.
- `06-review-resolution.md` summarizes subagent review findings and the plan edits made after review.
- `07-review-acceptance-correctness.md` records the acceptance-focused subagent review.
- `08-review-implementation-feasibility.md` records the implementation-focused subagent review.
