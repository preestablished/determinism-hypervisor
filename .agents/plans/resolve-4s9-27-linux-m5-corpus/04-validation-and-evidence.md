# Validation And Evidence

## Environment

Use this reference host and staged artifact environment:

```bash
export DH_M9_ALLOW_SKIP=0
export DH_M9_GUEST=linux
export DH_M9_BZIMAGE=/home/infra-admin/.cache/dh-m9/reference-workload/bzImage
export DH_M9_INITRAMFS=/home/infra-admin/.cache/dh-m9/reference-workload/initramfs.cpio
export DH_M9_BASE_IMAGE=/home/infra-admin/.cache/dh-m9/reference-workload/base.img
export DH_M9_GAME_IMAGE=/home/infra-admin/.cache/dh-m9/reference-workload/game.img
export DH_M9_IMAGE_CACHE=/home/infra-admin/.cache/dh-m9/image-cache
```

`DH_M9_ALLOW_SKIP=0` is mandatory for bead-closing evidence.

## Preflight Commands

```bash
git status --short --branch
test -r /dev/kvm && test -w /dev/kvm
b3sum "$DH_M9_BZIMAGE" "$DH_M9_INITRAMFS" "$DH_M9_BASE_IMAGE" "$DH_M9_GAME_IMAGE"
cargo test -p dh-worker --test m5_record_replay linux -- --ignored --list
```

The list command must show the real Linux corpus test.

## Target Acceptance

Primary command:

```bash
DH_M9_ALLOW_SKIP=0 DH_M9_GUEST=linux \
DH_M9_BZIMAGE=/home/infra-admin/.cache/dh-m9/reference-workload/bzImage \
DH_M9_INITRAMFS=/home/infra-admin/.cache/dh-m9/reference-workload/initramfs.cpio \
DH_M9_BASE_IMAGE=/home/infra-admin/.cache/dh-m9/reference-workload/base.img \
DH_M9_GAME_IMAGE=/home/infra-admin/.cache/dh-m9/reference-workload/game.img \
DH_M9_IMAGE_CACHE=/home/infra-admin/.cache/dh-m9/image-cache \
cargo test -p dh-worker --test m5_record_replay --release linux -- --ignored --nocapture
```

Required output properties:

- exactly the Linux M5 corpus test runs for the Linux filter;
- no regeneration-only test is selected by the Linux filter;
- no guard-only test remains;
- no skip output appears;
- `VerifyReplay` reports zero divergence;
- test output prints:
  - artifact hashes;
  - frame budget;
  - end icount;
  - epoch hash count;
  - END state hash;
  - optional workload proof checksum.

## Regression Commands

Run these before closing `4s9.27`:

```bash
cargo fmt --check
cargo test -p dh-worker terminal_sdk --lib
cargo test -p dh-worker --test m5_record_replay -- --nocapture
cargo test -p dh-worker --test m5_record_replay --release m5_accept_record_replay_60s_vns_pad_sequence_x100 -- --ignored --nocapture
cargo test -p dh-worker --test m5_frame_scheduling -- --nocapture
cargo test -p dh-worker --test m5_net_loopback -- --nocapture
git diff --check
```

Rationale:

- `terminal_sdk` protects the replay tail behavior used by Linux post-READY segments.
- non-ignored `m5_record_replay` protects the existing 6-second nanokernel corpus.
- the named ignored release `m5_accept_record_replay_60s_vns_pad_sequence_x100` protects the existing 60-second nanokernel M5 acceptance without selecting regeneration tests that intentionally panic unless their regen env var is set.
- `m5_frame_scheduling` and `m5_net_loopback` protect the prior Linux M5 work from regressions.

If runtime is a concern, run the nanokernel ignored release gate at least once on this reference host before closing the bead; do not claim it passed without running it.

## Optional Linux Cross-Checks

These are not required by `4s9.27`, but they are useful if the Linux corpus changes shared replay code:

```bash
DH_M9_ALLOW_SKIP=0 DH_M9_GUEST=linux ... cargo test -p dh-worker --test m5_frame_scheduling --release linux -- --ignored --nocapture
DH_M9_ALLOW_SKIP=0 DH_M9_GUEST=linux ... cargo test -p dh-worker --test m5_net_loopback --release linux -- --ignored --nocapture
DH_M9_ALLOW_SKIP=0 DH_M9_GUEST=linux ... cargo test -p dh-worker --test m4_transparency --release linux -- --ignored --nocapture
```

Use the full environment block from above; the `...` is a readability placeholder, not a literal command.

## Failure Triage

### No EpochOk Progress

Likely causes:

- frame budget too short to cross an epoch boundary;
- machine config for the Linux worker has epoch hashing disabled;
- the test is verifying the wrong segment, such as READY instead of post-READY.

Fix by increasing frame budget first. Do not accept a zero-epoch corpus.

### VerifyReplay Divergence

Collect:

- first divergence message;
- parsed DHILOG record list around the final icount;
- frame mark records;
- detchannel SDK events;
- END hash and live snapshot hash.

The prior fix in `replay_engine.rs` is specifically about final frame mark and pause-drained detchannel ordering; inspect that path before adding test-specific allowances.

### Artifact Hash Drift

If any staged artifact hash differs from this plan, decide whether it is intentional.

- If intentional, regenerate the manifest and record the new hashes in `4s9.27`.
- If accidental, restore the staged artifact from the known cache or stop before updating expected metadata.

### Oversized Corpus Files

If full Linux root/log fixtures are large, use the lightweight manifest mode. The bead acceptance allows checked-in corpus updates when fixture size policy allows; it does not require committing large snapshots.
