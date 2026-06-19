# Test And Acceptance Gates

## Fixture Preflight

Run before any KVM-heavy gate:

```bash
cpio -it < "$DH_M9_INITRAMFS" | rg 'etc/detguest/boot.toml|detguest-agent|refwork|init'
cpio -i --to-stdout etc/detguest/boot.toml < "$DH_M9_INITRAMFS"
```

The manifest must not be the smoke manifest. It must contain the control and
expected-region contract.

## Existing Gate That Must Stay Green

```bash
DH_M9_ALLOW_SKIP=0 \
DH_M9_BZIMAGE="$DH_M9_BZIMAGE" \
DH_M9_INITRAMFS="$DH_M9_INITRAMFS" \
DH_M9_BASE_IMAGE="$DH_M9_BASE_IMAGE" \
DH_M9_GAME_IMAGE="$DH_M9_GAME_IMAGE" \
DH_M9_IMAGE_CACHE="$DH_M9_IMAGE_CACHE" \
cargo test -p determinism-tests --test linux_ready --release -- --ignored --nocapture
```

This must still show equal READY identity across cold boots.

## Gates Unblocked By This Plan

## Universal Linux Evidence Guard

Before counting any Linux-filtered worker or M7 command as evidence, prove the
test selector is real:

```bash
cargo test -p <package> --test <test-target> <linux-filter> -- --ignored --list
```

The list output must include at least one Linux-specific ignored test. If it
prints `0 tests`, the command is a false positive and cannot close a bead.

Every Linux test target added for this plan must also fail loudly when the
Linux selector/env var is unsupported. For example, if `DH_M9_GUEST=linux` or
`DH_M7_ACCEPT_GUEST=linux` is accepted by a command, the test code must read
that env var and must boot the Linux fixture. It must not silently run the
nanokernel path.

### Linux landing/counting

```bash
DH_M9_ALLOW_SKIP=0 \
DH_M9_BZIMAGE="$DH_M9_BZIMAGE" \
DH_M9_INITRAMFS="$DH_M9_INITRAMFS" \
DH_M9_BASE_IMAGE="$DH_M9_BASE_IMAGE" \
DH_M9_GAME_IMAGE="$DH_M9_GAME_IMAGE" \
DH_M9_IMAGE_CACHE="$DH_M9_IMAGE_CACHE" \
cargo test -p determinism-tests --test linux_landing_counting --release -- --ignored --nocapture
```

Required evidence:

- at least 100 post-READY targets;
- exact `icount == target` for all targets;
- identical `(icount, rip, rcx, state_hash)` across two cold boots;
- at least one interrupt-adjacent target;
- zero overshoots;
- zero skipped runs;
- guest-instruction count, not host exits, is the comparison axis.

### Linux timer/IRQ

```bash
DH_M9_ALLOW_SKIP=0 \
DH_M9_BZIMAGE="$DH_M9_BZIMAGE" \
DH_M9_INITRAMFS="$DH_M9_INITRAMFS" \
DH_M9_BASE_IMAGE="$DH_M9_BASE_IMAGE" \
DH_M9_GAME_IMAGE="$DH_M9_GAME_IMAGE" \
DH_M9_IMAGE_CACHE="$DH_M9_IMAGE_CACHE" \
cargo test -p determinism-tests --test linux_timer_determinism --release -- --ignored --nocapture
```

Required evidence:

- 100 Linux cases;
- identical delivered icount list;
- identical vector/source metadata;
- identical final state hash;
- no host-time timer source is created or advertised.

### Linux worker API

```bash
DH_M9_ALLOW_SKIP=0 \
DH_M9_BZIMAGE="$DH_M9_BZIMAGE" \
DH_M9_INITRAMFS="$DH_M9_INITRAMFS" \
DH_M9_BASE_IMAGE="$DH_M9_BASE_IMAGE" \
DH_M9_GAME_IMAGE="$DH_M9_GAME_IMAGE" \
DH_M9_IMAGE_CACHE="$DH_M9_IMAGE_CACHE" \
cargo test -p dh-worker --test linux_worker_api --release -- --ignored --nocapture
```

Phase A fixture evidence:

- manifest preflight accepts `[unit.control]`, `refwork-ctl`, `/dev/vdb`, and
  expected regions;
- `CreateVm` BzImage;
- `Run` until NextSdkEvent Ready kind 14;
- `StreamGuestEvents` filtering;
- `ReadGuestMemory region_ranges` can read declared regions.

Phase B full close evidence:

- `TakeSnapshot`;
- `RestoreSnapshot`;
- `Fork`;
- child run;
- `VerifyReplay`;
- restored region manifest generation and layout versions match Ready payload.

### Linux M4/M5 frame and IO

```bash
DH_M9_ALLOW_SKIP=0 DH_M9_GUEST=linux \
cargo test -p dh-worker --test m4_transparency --release linux -- --ignored --nocapture

DH_M9_ALLOW_SKIP=0 DH_M9_GUEST=linux \
cargo test -p dh-worker --test m5_frame_scheduling --release linux -- --ignored --nocapture

DH_M9_ALLOW_SKIP=0 DH_M9_GUEST=linux \
cargo test -p dh-worker --test m5_net_loopback --release linux -- --ignored --nocapture
```

Required evidence:

- `-- --ignored --list` first shows nonzero Linux tests for each target;
- zero divergence;
- real post-READY frame marks;
- real guest-driven IO in the same worker segment being recorded/replayed;
- final state hashes match.

### Linux M5 corpus

```bash
DH_M9_ALLOW_SKIP=0 \
cargo test -p dh-worker --test m5_record_replay --release linux -- --ignored --nocapture
```

Required evidence:

- `-- --ignored --list` first shows at least one Linux corpus test;
- nonzero `EPOCH_HASH` verification;
- END state hash equals recorded child snapshot;
- zero Divergence;
- no accepted skipped runs.
- corpus metadata records expected hashes, determinism-class lock reference,
  fixture artifact hashes, and fixture README/storage policy.

### Linux M7

```bash
DH_M9_ALLOW_SKIP=0 DH_M7_ACCEPT_SLOT_CORES=2-5 DH_M7_ACCEPT_GUEST=linux \
cargo test -p dh-worker --test m7_fork_verify --release -- --ignored --nocapture
```

Required evidence:

- `m7_fork_verify` code reads `DH_M7_ACCEPT_GUEST=linux` and boots the Linux
  fixture. If it still boots `nanokernel::pad_echo_elf()`, the command is not
  evidence.
- 1000 fork children;
- 1000/1000 `VerifyDone`;
- zero Divergence;
- matching `end_state_hash`.

Cross-slot evidence:

```bash
DH_M9_ALLOW_SKIP=0 DH_M7_ACCEPT_SLOT_CORES=2-5 DH_M7_ACCEPT_GUEST=linux \
cargo test -p dh-worker --test m7_fork_verify --release \
  m7_accept_cross_slot_rerun_10_seeded_forks_identical_refs -- --ignored --nocapture
```

Nightly canary evidence:

- nightly workflow runs a 100-child Linux canary;
- the workflow proves `DH_M7_ACCEPT_GUEST=linux` selects Linux code;
- nanokernel nightly lanes remain intact.

## Final Smoke Before Closing The Root Fixture Work

Run these quick non-Linux checks to catch unrelated regressions:

```bash
cargo fmt --check
cargo test --workspace
git diff --check
```

For a code-changing Ralph iteration, run the two-subagent review before merge.
