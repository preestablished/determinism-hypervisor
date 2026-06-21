# Reference Host Preflight

Run these checks from the repository root before any long acceptance command.
Fail closed if a required preflight fails.

## Clean Starting Point

```bash
git status --short --branch
git pull --rebase
git status --short --branch
```

If there are unrelated local changes, do not overwrite them. Either work
around them, ask for direction if they block the suite, or use a separate
worktree.

## Host Identity And Determinism Class

```bash
hostname
uname -a
grep -m1 'model name' /proc/cpuinfo
grep -m1 microcode /proc/cpuinfo
bash docs/ops/apply-host-config.sh --verify
bash ci/check-determinism-class.sh
```

Expected host class:

- Host: `infra-control`
- Kernel: Linux `6.8.0-124-generic`
- CPU: Intel(R) Core(TM) i5-8400 CPU @ 2.80GHz
- Microcode: `0xfa`
- Determinism-class check: all lock keys match.

If the live host differs, do not silently rebaseline. Follow the rebaseline
procedure in `docs/ops/host-config-intel-box.md` or file a bead.

## KVM, Tooling, And Slot Cores

```bash
test -c /dev/kvm && test -r /dev/kvm && test -w /dev/kvm
which nasm
cargo run -p dh-worker --bin dh-workerd -- --preflight
grep Cpus_allowed_list /proc/self/status
taskset -c 2-5 sh -c 'grep Cpus_allowed_list /proc/self/status'
```

The current-shell affinity must include CPUs `2-5` if any M7 command is run
without an outer `taskset`. This plan wraps every M7 acceptance command in
`taskset -c 2-5`, but still record the current shell affinity for evidence.
The taskset child check must report `Cpus_allowed_list: 2-5`. If the current
shell cannot create that taskset child, rerun from the self-hosted runner or
another shell/cpuset that exposes the slot-core set.

## Runner Reservation

Point-in-time process checks are not enough: a new `kvm-intel` workflow can
start while an operator shell is running the final suite. Reserve the host
before long M7 evidence using one of these two approaches.

Preferred approach: run the whole suite as a single manually dispatched job
on the existing single `kvm-intel` runner, if such a workflow or temporary
operator job exists. This gives GitHub's one-runner serialization for free.
Record the workflow run URL in the evidence.

Operator-shell approach: pause the repository's self-hosted runner service
while the local suite runs, then restart it during closeout:

```bash
sudo -n systemctl stop actions.runner.preestablished-determinism-hypervisor.infra-control-kvm-intel.service
systemctl is-active actions.runner.preestablished-determinism-hypervisor.infra-control-kvm-intel.service || true
```

Stopping the service can delay queued CI/nightly jobs. Do it deliberately and
record the pause in the evidence notes. Restart it in `06-beads-and-closeout.md`
before final handoff.

After either reservation path, check for active local KVM/cargo work and
queued/in-progress GitHub runs:

```bash
pgrep -a cargo || true
pgrep -a dh-workerd || true
pgrep -a qemu || true
gh run list --limit 10 --json databaseId,status,workflowName,headBranch \
  --jq '.[] | select(.status!="completed")'
```

Do not run full M7 acceptance while another `kvm-intel` job is consuming the
same host. A queued nightly can wait; a concurrent slot-core user invalidates
final determinism evidence.

## M9 Artifact Environment

Use the documented reference layout unless a reviewed artifact move has
landed.

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

Hash the live artifacts before running the suite:

```bash
b3sum "$DH_M9_BZIMAGE" "$DH_M9_INITRAMFS" "$DH_M9_BASE_IMAGE" "$DH_M9_GAME_IMAGE"
for f in "$DH_M9_BZIMAGE" "$DH_M9_INITRAMFS" "$DH_M9_BASE_IMAGE" "$DH_M9_GAME_IMAGE"; do
  h=$(b3sum "$f" | awk '{print $1}')
  test -f "$DH_M9_IMAGE_CACHE/$h" || echo "missing image-cache entry for $f: $h"
done
```

Expected current M4/M5/M7 artifact hashes:

```text
bzImage          595466463a37efac6822ffccf3e61d0a2230e7d223a94c0bce5eb78b2f43bee9
initramfs.cpio   87edf64db22dc85ef0c6b17fdc6e58a8f73391a6053e96f7a1056da7d08b9f57
base.img         488de202f73bd976de4e7048f4e1f39a776d86d582b7348ff53bf432b987fca8
game.img         e02849845005d9d34fa3245d98fa59116a0245ed0136b496dbd2defebdc203ac
```

The earlier Phase 1 producer evidence used `initramfs.cpio` hash
`f130e1a329bf934651d89dccdec0a2dccd33862319cbbe95c30e0505382d12d4`.
For final acceptance, prefer the current reference-workload initramfs
already used by M4/M5/M7. If the Phase 1 CLI gate produces different hashes
because of this newer initramfs, record those fresh final hashes explicitly
instead of mutating old producer evidence.

## Worker Image Cache

Worker-service tests require `DH_M9_IMAGE_CACHE` entries keyed by lowercase
BLAKE3 hex. The helpers populate cache entries when needed, but verify the
cache directory is writable before starting:

```bash
touch "$DH_M9_IMAGE_CACHE/.write-test"
rm -f "$DH_M9_IMAGE_CACHE/.write-test"
ls -l "$DH_M9_IMAGE_CACHE" | sed -n '1,40p'
```

Do not commit any artifact or image-cache bytes. They must remain outside the
repository.
