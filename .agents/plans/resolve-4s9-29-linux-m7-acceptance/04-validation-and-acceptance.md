# Validation And Acceptance

## Preflight

Run from the repository root on the Linux/KVM reference host.

```bash
git status --short --branch
test -c /dev/kvm && test -r /dev/kvm && test -w /dev/kvm
which nasm
bash ci/check-determinism-class.sh
```

Export artifacts:

```bash
m9_artifact_root="$HOME/.cache/dh-m9/reference-workload"
export DH_M9_BZIMAGE="$m9_artifact_root/bzImage"
export DH_M9_INITRAMFS="$m9_artifact_root/initramfs.cpio"
export DH_M9_BASE_IMAGE="$m9_artifact_root/base.img"
export DH_M9_GAME_IMAGE="$m9_artifact_root/game.img"
export DH_M9_IMAGE_CACHE="$HOME/.cache/dh-m9/image-cache"
export DH_M9_ALLOW_SKIP=0
mkdir -p "$DH_M9_IMAGE_CACHE"
test -f "$DH_M9_BZIMAGE"
test -f "$DH_M9_INITRAMFS"
test -f "$DH_M9_BASE_IMAGE"
test -f "$DH_M9_GAME_IMAGE"
test -d "$DH_M9_IMAGE_CACHE"
```

Confirm the Linux fixture contract before running M7 Linux:

```bash
DH_M9_ALLOW_SKIP=0 \
cargo test -p determinism-tests --test linux_fixture_contract -- --ignored --nocapture
```

Confirm the requested M7 acceptance cores are available to the process:

```bash
export DH_M7_ACCEPT_SLOT_CORES=2-5
python3 - <<'PY'
from pathlib import Path

def expand(spec):
    out = set()
    for part in spec.split(','):
        if '-' in part:
            lo, hi = map(int, part.split('-', 1))
            out.update(range(lo, hi + 1))
        elif part:
            out.add(int(part))
    return out

allowed = None
for line in Path('/proc/self/status').read_text().splitlines():
    if line.startswith('Cpus_allowed_list:'):
        allowed = expand(line.split(':', 1)[1].strip())
        break
online = expand(Path('/sys/devices/system/cpu/online').read_text().strip())
requested = expand('2-5')
available = online & (allowed if allowed is not None else online)
missing = sorted(requested - available)
if missing:
    raise SystemExit(f'DH_M7_ACCEPT_SLOT_CORES=2-5 unavailable in this process affinity: {missing}; run under the self-hosted runner or cpuset that exposes cores 2-5')
print('DH_M7_ACCEPT_SLOT_CORES=2-5 available')
PY
```

Confirm artifact hashes if the implementation depends on the current checked-in manifest:

```bash
b3sum "$DH_M9_BZIMAGE" "$DH_M9_INITRAMFS" "$DH_M9_BASE_IMAGE" "$DH_M9_GAME_IMAGE"
```

## Fast Rust Checks

```bash
cargo fmt --check
cargo test -p dh-worker --test m7_fork_verify -- --nocapture
```

Run a nanokernel smoke to catch regressions in the default path:

```bash
DH_M7_ACCEPT_JOBS=2 \
DH_M7_ACCEPT_SLOT_CORES=2-5 \
DH_M7_ACCEPT_ALLOW_SKIP=0 \
cargo test -p dh-worker --test m7_fork_verify --release \
  m7_accept_1000_seeded_forks_verify_replay_all \
  -- --ignored --nocapture
```

## Linux Smoke

Run a small Linux fork/verify smoke before the full gate:

```bash
DH_M9_ALLOW_SKIP=0 \
DH_M7_ACCEPT_GUEST=linux \
DH_M7_ACCEPT_JOBS=2 \
DH_M7_ACCEPT_SLOT_CORES=2-5 \
DH_M7_ACCEPT_ALLOW_SKIP=0 \
cargo test -p dh-worker --test m7_fork_verify --release \
  m7_accept_1000_seeded_forks_verify_replay_all \
  -- --ignored --nocapture
```

Expected evidence:

- Linux fixture boots to READY once.
- two children run with `FrameBudget(5)`.
- both children seal DHILOGs with the READY snapshot as base.
- both children emit at least one `EPOCH_HASH`.
- both children emit `FRAME_MARK` records.
- both children complete `VerifyReplay` with no `Divergence`.

## Linux Cross-Slot Smoke

```bash
DH_M9_ALLOW_SKIP=0 \
DH_M7_ACCEPT_GUEST=linux \
DH_M7_ACCEPT_JOBS=2 \
DH_M7_CROSS_CHECKS=2 \
DH_M7_ACCEPT_SLOT_CORES=2-5 \
DH_M7_ACCEPT_ALLOW_SKIP=0 \
cargo test -p dh-worker --test m7_fork_verify --release \
  m7_accept_cross_slot_rerun_10_seeded_forks_identical_refs \
  -- --ignored --nocapture
```

Expected evidence:

- same-seed children land on distinct child slots;
- same-seed snapshot refs match across slots;
- same-seed state hashes match across slots;
- same-seed input log ids match across slots;
- same-seed DHILOG payload bytes match across slots.

## Full Linux Acceptance

This is the bead's primary acceptance command:

```bash
DH_M9_ALLOW_SKIP=0 \
DH_M7_ACCEPT_SLOT_CORES=2-5 \
DH_M7_ACCEPT_GUEST=linux \
DH_M7_ACCEPT_JOBS=1000 \
DH_M7_CROSS_CHECKS=10 \
DH_M7_ACCEPT_ALLOW_SKIP=0 \
cargo test -p dh-worker --test m7_fork_verify --release \
  -- --ignored --nocapture --test-threads=1
```

Required result:

- 1000 fork children complete.
- 1000/1000 `VerifyReplay` streams end with `Done`.
- zero `Divergence`.
- every `Done.end_state_hash` matches the corresponding child snapshot state hash.
- cross-slot sampled same-seed children produce identical refs and logs.

If runtime is too high because the unfiltered ignored command runs both full and cross-slot tests, keep that behavior for final acceptance and use targeted commands only for iteration.

Also record the targeted full cross-slot command separately so the evidence is not hidden inside the unfiltered run:

```bash
DH_M9_ALLOW_SKIP=0 \
DH_M7_ACCEPT_SLOT_CORES=2-5 \
DH_M7_ACCEPT_GUEST=linux \
DH_M7_ACCEPT_JOBS=1000 \
DH_M7_CROSS_CHECKS=10 \
DH_M7_ACCEPT_ALLOW_SKIP=0 \
cargo test -p dh-worker --test m7_fork_verify --release \
  m7_accept_cross_slot_rerun_10_seeded_forks_identical_refs \
  -- --ignored --nocapture
```

Run the unfiltered full acceptance and the targeted full cross-slot evidence sequentially. Do not run M7 acceptance commands in parallel on the reference host: both allocate from `DH_M7_ACCEPT_SLOT_CORES=2-5`, and the runner assumes exclusive slot cores during determinism gates.

## Nightly Canary Validation

After updating `.github/workflows/nightly-drift.yaml`, run the equivalent local command:

```bash
DH_M9_ALLOW_SKIP=0 \
DH_M7_ACCEPT_GUEST=linux \
DH_M7_ACCEPT_JOBS=100 \
DH_M7_ACCEPT_SLOT_CORES=2-5 \
DH_M7_ACCEPT_ALLOW_SKIP=0 \
cargo test -p dh-worker --test m7_fork_verify --release \
  m7_accept_1000_seeded_forks_verify_replay_all \
  -- --ignored --nocapture
```

If a separate nightly Linux job is added, its cargo command should target only the full 100-child test. The cross-slot test remains operator-run unless runtime proves it is cheap enough for nightly.

## Documentation Verification

After editing docs:

```bash
rg -n "DH_M7_ACCEPT_GUEST|M7 Linux|m7_fork_verify|DH_M9_ALLOW_SKIP" \
  docs/ops/test-partitioning.md .github/workflows/nightly-drift.yaml
```

Before committing:

```bash
git diff --check
git status --short --branch
```
