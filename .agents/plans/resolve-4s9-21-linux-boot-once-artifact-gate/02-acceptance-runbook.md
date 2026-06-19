# Acceptance Runbook

This is the happy-path sequence for an agent with the M9 artifacts available.

## Start State

Start from a clean, pushed `main`:

```bash
git checkout main
git pull --ff-only origin main
git status --short --branch
bd ready
bd show determinism-hypervisor-4s9.21
bd update determinism-hypervisor-4s9.21 --claim
```

If `4s9.21` is still blocked and no code changes are expected, do not create a coding branch just to run the gates. If a test fails and code must change, create a normal Ralph iteration branch before editing.

Run the host/artifact preflight from `01-artifact-prerequisites.md`, including
the KVM dirty-ring capability test, before the release acceptance commands.

## Build the Ignored Tests

Run the release no-run builds first. These catch compile/link issues before spending time on the full artifact gate:

```bash
cargo test -p dh-worker --test restore_engine linux_boot_once --release --no-run
cargo test -p dh-worker --test replay_engine linux_boot_once --release --no-run
```

## Run Final Acceptance

Run the two final commands separately so each failure has a clear log. In
`bash` or `zsh`, use `pipefail` so `tee` does not hide a failing test:

```bash
set -o pipefail
mkdir -p /tmp/4s9.21-evidence
DH_M9_ALLOW_SKIP=0 cargo test -p dh-worker --test restore_engine linux_boot_once --release -- --ignored --nocapture 2>&1 | tee /tmp/4s9.21-evidence/restore_engine-linux_boot_once.log
```

Expected high-level behavior:

- The test boots Linux to Ready once through `CreateVm`.
- `boot_observer::bzimage_loads()` reaches `1`.
- `RestoreSnapshot` returns the same decoded `MachineConfig` hash and Ready `state_hash`.
- `Fork` returns a child whose snapshot has the same Ready `state_hash`.
- Restored and forked snapshots preserve EVTC and BLKO sections byte-for-byte.
- The final BzImage load count remains `1`.

Then run:

```bash
DH_M9_ALLOW_SKIP=0 cargo test -p dh-worker --test replay_engine linux_boot_once --release -- --ignored --nocapture 2>&1 | tee /tmp/4s9.21-evidence/replay_engine-linux_boot_once.log
```

Expected high-level behavior:

- The test boots Linux to Ready once through `CreateVm`.
- `VerifyReplay` returns `Done`.
- `Done.total_icount` equals the Ready snapshot icount.
- `Done.end_state_hash` equals the live Ready snapshot state hash.
- READY EVTC and BLKO sections remain unchanged after replay.
- The final BzImage load count remains `1`.

## Regression Gates After Acceptance

If no code changed, the two final acceptance commands are the meaningful closure evidence. Still run a cheap cleanliness check:

```bash
git status --short --branch
ls -l /tmp/4s9.21-evidence/
```

If code changed during triage, run the broader gates before review and merge:

```bash
cargo fmt
cargo test -p dh-worker --test restore_engine linux_boot_once --release --no-run
cargo test -p dh-worker --test replay_engine linux_boot_once --release --no-run
DH_M9_ALLOW_SKIP=0 cargo test -p dh-worker --test restore_engine linux_boot_once --release -- --ignored --nocapture
DH_M9_ALLOW_SKIP=0 cargo test -p dh-worker --test replay_engine linux_boot_once --release -- --ignored --nocapture
git diff --check
cargo test --workspace
cargo test --workspace
cargo test --workspace
cargo build --workspace
```

Use three consecutive full workspace runs for any code change in this area.
Restore/fork/replay and state-hash behavior is determinism-sensitive, and the
project has a recorded Ralph process lesson requiring 3+ full runs for this
class of change.
