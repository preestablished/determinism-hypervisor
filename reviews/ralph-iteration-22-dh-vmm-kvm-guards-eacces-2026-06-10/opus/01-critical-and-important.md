# Critical and Important Findings

## Critical

None.

## Important

### I-1 — CI `kvm-intel` lane gate uses read-only `test -r`, now weaker than the rw probe

- **Severity:** Important (pre-existing; out of diff scope, but this change makes the
  inconsistency consequential)
- **File:** `.github/workflows/ci.yaml:94`
- **Description:** The `kvm-intel` self-hosted lane's preflight step gates with
  `test -c /dev/kvm && test -r /dev/kvm` (read-only). The new test guards now require
  an **rw** open (`O_RDWR`) before any live-KVM leg runs. These two predicates can
  diverge: a runner with read-but-not-write access to `/dev/kvm` would **pass the
  lane's gate** (`test -r` succeeds) yet have **every live-KVM test silently
  self-skip** (`kvm_usable()` / the inline probe returns `false` because the write
  bit is missing). The job would go green while exercising none of the hypervisor —
  exactly the "skip-on-EACCES masks a real misconfiguration" failure mode the prompt
  asked me to scrutinize. The kvm-intel box currently has rw access (so the live legs
  do gate today), but the gate does not *enforce* the invariant the tests now depend on.
- **Why it matters:** The entire purpose of the `kvm-intel` lane is to stop the
  live tests self-skipping (comment at `ci.yaml:73-75`). A read-only gate no longer
  guarantees that.
- **Fix:** Tighten the lane's gate to the same rw semantics the tests use, so a
  write-denied runner fails the job loudly instead of going green with skipped tests:

  ```yaml
  # .github/workflows/ci.yaml — kvm-intel lane preflight step
  - name: Assert /dev/kvm is rw-usable (matches the test guards' rw-open probe)
    run: |
      test -c /dev/kvm || { echo "::error::/dev/kvm missing on runner"; exit 1; }
      test -r /dev/kvm && test -w /dev/kvm \
        || { echo "::error::/dev/kvm not rw-usable; live-KVM tests would silently skip"; exit 1; }
  ```

  (`test -w` is the shell-level analogue of the crate's `.write(true)` open. This keeps
  the gate and the guards in lockstep.)
- **Note:** Reasonable to land as a fast-follow rather than block this branch, since
  the current runner has rw access and the live legs do execute today. Filed as an
  action item.
