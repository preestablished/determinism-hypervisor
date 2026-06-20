# Current State

## Bead State

`bd show 4s9.27` currently says:

- The existing M9 Linux fixture previously only reached READY and halted, so it could not provide a post-READY input corpus.
- A guard test was added in `crates/dh-worker/tests/m5_record_replay.rs`.
- Acceptance requires a Linux root snapshot plus a deterministic post-READY input script or corpus whose DHILOG reverify proves every `EPOCH_HASH` and the END state hash with zero divergence.

That note is partly stale after the previous M9 post-READY work:

- The staged M9 workload no longer halts immediately after READY.
- It emits deterministic pv-pad frame marks.
- It performs deterministic guest-driven pv-blk IO in the first post-READY frame.
- `m5_net_loopback` Linux pv-blk replay evidence now passes.

The remaining task is to turn that post-READY behavior into a Linux M5 record/replay corpus gate in `m5_record_replay.rs`.

## Existing Nanokernel Corpus Pattern

`crates/dh-worker/tests/m5_record_replay.rs` already provides the canonical pattern:

- `record_replay_corpus_pad_echo_6s_reverifies` loads checked-in `root-sparse.bin`, `root.dhsnap`, `recording.dhilog`, and `expected.txt`.
- `regenerate_record_replay_corpus_pad_echo_6s` regenerates those fixtures only when `DH_WORKER_REGEN_RR_CORPUS=1` is set.
- `expected.txt` pins:
  - fixture name;
  - run length;
  - memory shape;
  - determinism-class lock hash;
  - snapshot ref;
  - machine config hash;
  - root and log hashes;
  - DHILOG record count;
  - END icount/vns/hash;
  - records applied;
  - epoch hash count;
  - each epoch chain value.

Reuse this shape where practical. Do not remove or weaken the nanokernel corpus.

## Existing M9 Helpers

`crates/dh-worker/tests/common/mod.rs` already provides the right Linux worker harness:

- `m9_linux_ready_snapshot(test_name, slots)` boots BzImage to READY through the worker service and returns:
  - in-process snapstore client;
  - worker service;
  - config hash;
  - initial snapshot;
  - READY snapshot response/ref/hash;
  - lease.
- `verify_replay_done(svc, base, input_log_id)` verifies a stored DHILOG but only returns final `VerifyDone`; it ignores `EpochOk` count.
- `snapshot_section` can read DHSNAP sections from stored snapshots.
- `populate_m9_image_cache`, `m9_linux_machine_config`, and artifact validation already centralize staged M9 artifact handling.

The implementation will likely need a new helper alongside `verify_replay_done` that counts `EpochOk` progress instead of discarding it.

## Staged Artifacts On This Host

Current reference artifacts while drafting:

```text
bzImage        595466463a37efac6822ffccf3e61d0a2230e7d223a94c0bce5eb78b2f43bee9
initramfs.cpio 87edf64db22dc85ef0c6b17fdc6e58a8f73391a6053e96f7a1056da7d08b9f57
base.img       488de202f73bd976de4e7048f4e1f39a776d86d582b7348ff53bf432b987fca8
game.img       e02849845005d9d34fa3245d98fa59116a0245ed0136b496dbd2defebdc203ac
```

Use these paths unless the artifact cache is intentionally refreshed:

```bash
DH_M9_BZIMAGE=/home/infra-admin/.cache/dh-m9/reference-workload/bzImage
DH_M9_INITRAMFS=/home/infra-admin/.cache/dh-m9/reference-workload/initramfs.cpio
DH_M9_BASE_IMAGE=/home/infra-admin/.cache/dh-m9/reference-workload/base.img
DH_M9_GAME_IMAGE=/home/infra-admin/.cache/dh-m9/reference-workload/game.img
DH_M9_IMAGE_CACHE=/home/infra-admin/.cache/dh-m9/image-cache
```

## Important Prior Work

The previous M9 post-READY plan closed `4s9.28` and added:

- Linux M4 post-READY snapshot/restore/fork transparency.
- Linux M5 frame-budget restore continuity.
- Linux pv-blk IO replay coverage in `m5_net_loopback`.
- Replay support for terminal pv-pad `FRAME_MARK` AUX records before regenerated pause-drain detchannel SDK events.

Use those tests as examples for worker-service style Linux gates:

- `crates/dh-worker/tests/m4_transparency.rs`
- `crates/dh-worker/tests/m5_frame_scheduling.rs`
- `crates/dh-worker/tests/m5_net_loopback.rs`

## Non-Goals

- Do not solve `4s9.29` M7 in this plan.
- Do not update Phase 1/Phase 2 docs in this plan except as Beads evidence for `4s9.27`.
- Do not rebaseline `ci/determinism-class.lock` unless a separate reviewed hash-contract change requires it.
- Do not accept `DH_M9_ALLOW_SKIP=1` output as evidence.
